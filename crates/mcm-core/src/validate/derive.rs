//! Deterministic date derivation (contracts/outline-grammar.md §时间推导).
//!
//! Precondition: the dependency graph is acyclic (guaranteed by `V-CYCLE`
//! running first). The walk is a single topological pass, so the result is
//! unique for any input.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::model::{
    Date, DateRange, Plan, PlanIndex, Schedule, TaskId, next_working_day_after,
    next_working_day_on_or_after, working_day_end,
};

/// Effective date ranges keyed by task.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DerivedDates {
    ranges: BTreeMap<TaskId, DateRange>,
}

impl DerivedDates {
    #[must_use]
    pub fn range(&self, id: TaskId) -> Option<DateRange> {
        self.ranges.get(&id).copied()
    }

    #[must_use]
    pub fn start(&self, id: TaskId) -> Option<Date> {
        self.range(id).map(|r| r.start)
    }

    #[must_use]
    pub fn end(&self, id: TaskId) -> Option<Date> {
        self.range(id).map(|r| r.end)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Overall envelope across all dated tasks (timeline bounds).
    #[must_use]
    pub fn envelope(&self) -> Option<DateRange> {
        self.ranges.values().copied().reduce(DateRange::envelope)
    }
}

/// Computes effective dates for every task.
#[must_use]
pub fn derive_dates(plan: &Plan) -> DerivedDates {
    let mut derived = DerivedDates::default();
    if plan.tasks.is_empty() {
        return derived;
    }
    let index = plan.index();

    // 1) Explicit dates win outright.
    for task in &plan.tasks {
        if let Some(range) = task.schedule.explicit_range() {
            // Reversed ranges are reported as V-RANGE; normalise so downstream
            // rules still see a usable window.
            let normalised = if range.is_ordered() {
                range
            } else {
                DateRange::new(range.end, range.start)
            };
            derived.ranges.insert(task.id, normalised);
        }
    }

    // 2) Duration tasks resolve in dependency order.
    let predecessors = predecessor_map(plan);
    let order = topological_order(plan, &predecessors);
    let anchor = plan.project_start;

    for id in order {
        let Some(task) = plan.task(id) else { continue };
        let Schedule::Duration { days } = task.schedule else {
            continue;
        };

        let latest_predecessor_end = predecessors
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|pred| derived.end(*pred))
            .max();

        let start = match latest_predecessor_end {
            Some(end) => next_working_day_after(end),
            None => match anchor {
                Some(date) => next_working_day_on_or_after(date),
                // No anchor: leave undated rather than inventing "today", which
                // would make derivation non-deterministic.
                None => continue,
            },
        };
        derived
            .ranges
            .insert(id, DateRange::new(start, working_day_end(start, days)));
    }

    // 3) Parents without explicit dates take the envelope of their children.
    roll_up_parents(plan, &index, &mut derived);

    derived
}

fn predecessor_map(plan: &Plan) -> BTreeMap<TaskId, Vec<TaskId>> {
    let index = plan.index();
    let mut map: BTreeMap<TaskId, Vec<TaskId>> = BTreeMap::new();
    for dep in &plan.dependencies {
        if dep.predecessor == dep.successor {
            continue;
        }
        if index.has_task(dep.predecessor) && index.has_task(dep.successor) {
            map.entry(dep.successor).or_default().push(dep.predecessor);
        }
    }
    for list in map.values_mut() {
        list.sort_unstable();
        list.dedup();
    }
    map
}

/// Kahn's algorithm with a deterministic tie-break on document order.
fn topological_order(plan: &Plan, predecessors: &BTreeMap<TaskId, Vec<TaskId>>) -> Vec<TaskId> {
    let document_order: Vec<TaskId> = plan
        .tasks_in_document_order()
        .iter()
        .map(|t| t.id)
        .collect();
    let position: BTreeMap<TaskId, usize> = document_order
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();

    let mut indegree: BTreeMap<TaskId, usize> =
        document_order.iter().map(|id| (*id, 0usize)).collect();
    let mut successors: BTreeMap<TaskId, Vec<TaskId>> = BTreeMap::new();
    for (successor, preds) in predecessors {
        indegree.insert(*successor, preds.len());
        for pred in preds {
            successors.entry(*pred).or_default().push(*successor);
        }
    }

    let mut ready: Vec<TaskId> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| *id)
        .collect();
    ready.sort_by_key(|id| position.get(id).copied().unwrap_or(usize::MAX));
    let mut queue: VecDeque<TaskId> = ready.into();

    let mut order = Vec::with_capacity(document_order.len());
    let mut emitted: BTreeSet<TaskId> = BTreeSet::new();
    while let Some(id) = queue.pop_front() {
        if !emitted.insert(id) {
            continue;
        }
        order.push(id);
        let mut next_ready = Vec::new();
        for successor in successors.get(&id).cloned().unwrap_or_default() {
            if let Some(degree) = indegree.get_mut(&successor) {
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    next_ready.push(successor);
                }
            }
        }
        next_ready.sort_by_key(|id| position.get(id).copied().unwrap_or(usize::MAX));
        for successor in next_ready {
            queue.push_back(successor);
        }
    }

    // Any node left out (only possible with a cycle) still gets a slot so the
    // caller never loses tasks.
    for id in document_order {
        if emitted.insert(id) {
            order.push(id);
        }
    }
    order
}

/// Deepest-first so grandparents see already-rolled-up children.
fn roll_up_parents(plan: &Plan, index: &PlanIndex<'_>, derived: &mut DerivedDates) {
    // Group children once instead of rescanning the task list per parent.
    let mut children_of: BTreeMap<TaskId, Vec<TaskId>> = BTreeMap::new();
    for task in &plan.tasks {
        if let Some(parent) = task.parent {
            children_of.entry(parent).or_default().push(task.id);
        }
    }
    let mut tasks: Vec<(usize, TaskId)> = plan
        .tasks
        .iter()
        .map(|task| (index.depth_of(task.id), task.id))
        .collect();
    tasks.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    for (_, id) in tasks {
        let Some(task) = index.task(id) else { continue };
        if task.schedule.explicit_range().is_some() {
            continue;
        }
        let Some(children) = children_of.get(&id) else {
            continue;
        };
        if children.is_empty() {
            continue;
        }
        let envelope = children
            .iter()
            .filter_map(|child| derived.range(*child))
            .reduce(DateRange::envelope);
        if let Some(range) = envelope {
            derived.ranges.insert(id, range);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{format_date, parse_date};
    use crate::outline::parse;

    fn derive(source: &str) -> (Plan, DerivedDates) {
        let plan = parse(source).plan;
        let derived = derive_dates(&plan);
        (plan, derived)
    }

    fn range_text(derived: &DerivedDates, id: u32) -> String {
        let range = derived.range(TaskId(id)).expect("range");
        format!("{}..{}", format_date(range.start), format_date(range.end))
    }

    #[test]
    fn explicit_dates_are_used_verbatim() {
        let (_, derived) = derive("- 甲 #t1 [2026-09-01..2026-09-05]\n");
        assert_eq!(range_text(&derived, 1), "2026-09-01..2026-09-05");
    }

    #[test]
    fn duration_starts_at_project_start() {
        let (_, derived) = derive("%start 2026-09-01\n- 甲 #t1 [3d]\n");
        // 2026-09-01 is a Tuesday: Tue, Wed, Thu.
        assert_eq!(range_text(&derived, 1), "2026-09-01..2026-09-03");
    }

    #[test]
    fn duration_follows_predecessor_end() {
        let source = "%start 2026-09-01\n- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2d] <-t1\n";
        let (_, derived) = derive(source);
        assert_eq!(range_text(&derived, 2), "2026-09-03..2026-09-04");
    }

    #[test]
    fn duration_skips_weekends() {
        // 2026-09-04 is a Friday; a 3-day task starting after it runs Mon–Wed.
        let source = "- 甲 #t1 [2026-09-01..2026-09-04]\n- 乙 #t2 [3d] <-t1\n";
        let (_, derived) = derive(source);
        assert_eq!(range_text(&derived, 2), "2026-09-07..2026-09-09");
    }

    #[test]
    fn multiple_predecessors_use_the_latest_end() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2026-09-01..2026-09-08]\n- 丙 #t3 [1d] <-t1 <-t2\n";
        let (_, derived) = derive(source);
        assert_eq!(range_text(&derived, 3), "2026-09-09..2026-09-09");
    }

    #[test]
    fn parents_roll_up_from_children() {
        let source = "- 父 #t1\n  - 子甲 #t2 [2026-09-03..2026-09-04]\n  - 子乙 #t3 [2026-09-01..2026-09-02]\n";
        let (_, derived) = derive(source);
        assert_eq!(range_text(&derived, 1), "2026-09-01..2026-09-04");
    }

    #[test]
    fn explicit_parent_dates_beat_rollup() {
        let source = "- 父 #t1 [2026-09-01..2026-09-30]\n  - 子 #t2 [2026-09-03..2026-09-04]\n";
        let (_, derived) = derive(source);
        assert_eq!(range_text(&derived, 1), "2026-09-01..2026-09-30");
    }

    #[test]
    fn nested_parents_roll_up_deepest_first() {
        let source = "- 祖 #t1\n  - 父 #t2\n    - 子 #t3 [2026-09-05..2026-09-06]\n";
        let (_, derived) = derive(source);
        assert_eq!(range_text(&derived, 2), "2026-09-05..2026-09-06");
        assert_eq!(range_text(&derived, 1), "2026-09-05..2026-09-06");
    }

    #[test]
    fn undated_leaf_stays_undated() {
        let (_, derived) = derive("- 甲 #t1\n");
        assert!(derived.range(TaskId(1)).is_none());
    }

    #[test]
    fn duration_without_anchor_or_predecessor_stays_undated() {
        let (_, derived) = derive("- 甲 #t1 [3d]\n");
        assert!(derived.range(TaskId(1)).is_none());
    }

    #[test]
    fn derivation_is_deterministic() {
        let source = "%start 2026-09-01\n- 甲 #t1 [2d]\n- 乙 #t2 [3d] <-t1\n- 丙 #t3 [1d] <-t2\n";
        let (plan, baseline) = derive(source);
        for _ in 0..100 {
            assert_eq!(derive_dates(&plan), baseline);
        }
    }

    #[test]
    fn envelope_spans_all_dated_tasks() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2026-09-10..2026-09-11]\n";
        let (_, derived) = derive(source);
        let envelope = derived.envelope().expect("envelope");
        assert_eq!(envelope.start, parse_date("2026-09-01").unwrap());
        assert_eq!(envelope.end, parse_date("2026-09-11").unwrap());
    }

    #[test]
    fn chained_durations_accumulate_across_weekends() {
        let source = "%start 2026-09-03\n- 甲 #t1 [2d]\n- 乙 #t2 [2d] <-t1\n";
        let (_, derived) = derive(source);
        // Thu+Fri, then Mon+Tue.
        assert_eq!(range_text(&derived, 1), "2026-09-03..2026-09-04");
        assert_eq!(range_text(&derived, 2), "2026-09-07..2026-09-08");
    }
}
