//! Deterministic validation engine (data-model.md §校验规则).
//!
//! Rules run in a fixed order and every issue carries a locatable target plus a
//! non-empty fix hint (spec FR-004).

pub mod derive;

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{
    DateRange, ElementRef, Plan, PlanIndex, TaskId, ValidationIssue, format_date,
    working_days_between,
};

pub use derive::{DerivedDates, derive_dates};

/// Runs every rule and returns issues in a stable order.
#[must_use]
pub fn validate(plan: &Plan) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    // One index shared by every rule keeps validation linear (plan.md budgets).
    let index = plan.index();

    check_titles(plan, &mut issues);
    check_duplicate_ids(plan, &mut issues);
    check_references(plan, &index, &mut issues);
    check_self_dependencies(plan, &mut issues);
    check_hierarchy_dependencies(plan, &index, &mut issues);
    check_explicit_ranges(plan, &mut issues);

    // Cycle detection gates date derivation: without it the topological walk
    // would not terminate (outline-grammar §时间推导).
    let cycles = check_cycles(plan, &index, &mut issues);
    if !cycles {
        let derived = derive_dates(plan);
        check_parent_containment(plan, &index, &derived, &mut issues);
        check_dependency_order(plan, &derived, &mut issues);
        check_milestones(plan, &derived, &mut issues);
        check_missing_dates(plan, &index, &derived, &mut issues);
    }

    check_orphan_clusters(plan, &index, &mut issues);
    issues
}

fn check_titles(plan: &Plan, issues: &mut Vec<ValidationIssue>) {
    for task in &plan.tasks {
        if task.title.trim().is_empty() {
            issues.push(ValidationIssue::error(
                "V-TITLE",
                ElementRef::Task { id: task.id },
                format!("任务 {} 没有名称", task.id),
                "补写任务名称",
            ));
        }
    }
    for milestone in &plan.milestones {
        if milestone.name.trim().is_empty() {
            issues.push(ValidationIssue::error(
                "V-TITLE",
                ElementRef::Milestone { id: milestone.id },
                format!("里程碑 {} 没有名称", milestone.id),
                "补写里程碑名称",
            ));
        }
    }
}

fn check_duplicate_ids(plan: &Plan, issues: &mut Vec<ValidationIssue>) {
    let mut seen = BTreeSet::new();
    for task in &plan.tasks {
        if !seen.insert(task.id) {
            issues.push(ValidationIssue::error(
                "V-DUP",
                ElementRef::Task { id: task.id },
                format!("任务标识 {} 重复", task.id),
                "为其中一个任务更换 #id",
            ));
        }
    }
    let mut seen_milestones = BTreeSet::new();
    for milestone in &plan.milestones {
        if !seen_milestones.insert(milestone.id) {
            issues.push(ValidationIssue::error(
                "V-DUP",
                ElementRef::Milestone { id: milestone.id },
                format!("里程碑标识 {} 重复", milestone.id),
                "为其中一个里程碑更换 #id",
            ));
        }
    }
}

fn check_references(plan: &Plan, index: &PlanIndex<'_>, issues: &mut Vec<ValidationIssue>) {
    for task in &plan.tasks {
        if let Some(parent) = task.parent {
            if !index.has_task(parent) {
                issues.push(ValidationIssue::error(
                    "V-REF",
                    ElementRef::Task { id: task.id },
                    format!("任务 {} 的父任务 {parent} 不存在", task.id),
                    "改为现有任务，或将其提升为顶层任务",
                ));
            }
        }
    }
    for dep in &plan.dependencies {
        for (id, role) in [(dep.predecessor, "前置"), (dep.successor, "后继")] {
            if !index.has_task(id) {
                issues.push(ValidationIssue::error(
                    "V-REF",
                    ElementRef::Dependency {
                        predecessor: dep.predecessor,
                        successor: dep.successor,
                    },
                    format!("依赖引用的{role}任务 {id} 不存在"),
                    "删除该依赖或改为现有任务 ID",
                ));
            }
        }
    }
    for milestone in &plan.milestones {
        for task in &milestone.linked_tasks {
            if !index.has_task(*task) {
                issues.push(ValidationIssue::error(
                    "V-REF",
                    ElementRef::Milestone { id: milestone.id },
                    format!("里程碑关联的任务 {task} 不存在"),
                    "删除该关联或改为现有任务 ID",
                ));
            }
        }
    }
}

fn check_self_dependencies(plan: &Plan, issues: &mut Vec<ValidationIssue>) {
    for dep in &plan.dependencies {
        if dep.predecessor == dep.successor {
            issues.push(ValidationIssue::error(
                "V-SELF",
                ElementRef::Dependency {
                    predecessor: dep.predecessor,
                    successor: dep.successor,
                },
                format!("任务 {} 依赖自身", dep.predecessor),
                "删除该自依赖",
            ));
        }
    }
}

fn check_hierarchy_dependencies(
    plan: &Plan,
    index: &PlanIndex<'_>,
    issues: &mut Vec<ValidationIssue>,
) {
    for dep in &plan.dependencies {
        if dep.predecessor == dep.successor {
            continue;
        }
        let ancestor_link = index.is_ancestor_of(dep.predecessor, dep.successor)
            || index.is_ancestor_of(dep.successor, dep.predecessor);
        if ancestor_link {
            issues.push(ValidationIssue::error(
                "V-HIER",
                ElementRef::Dependency {
                    predecessor: dep.predecessor,
                    successor: dep.successor,
                },
                format!(
                    "{} 与 {} 是祖先/后代关系，不能建立依赖",
                    dep.predecessor, dep.successor
                ),
                "依赖应建立在同层可比任务之间",
            ));
        }
    }
}

fn check_explicit_ranges(plan: &Plan, issues: &mut Vec<ValidationIssue>) {
    for task in &plan.tasks {
        if let Some(range) = task.schedule.explicit_range() {
            if !range.is_ordered() {
                issues.push(ValidationIssue::error(
                    "V-RANGE",
                    ElementRef::Task { id: task.id },
                    format!(
                        "任务 {} 的开始日期 {} 晚于结束日期 {}",
                        task.id,
                        format_date(range.start),
                        format_date(range.end)
                    ),
                    "交换或修正起止日期",
                ));
            }
        }
    }
}

/// Returns true when at least one cycle was reported.
fn check_cycles(plan: &Plan, index: &PlanIndex<'_>, issues: &mut Vec<ValidationIssue>) -> bool {
    let mut adjacency: BTreeMap<TaskId, Vec<TaskId>> = BTreeMap::new();
    for dep in &plan.dependencies {
        if dep.predecessor == dep.successor {
            continue;
        }
        if index.has_task(dep.predecessor) && index.has_task(dep.successor) {
            adjacency
                .entry(dep.predecessor)
                .or_default()
                .push(dep.successor);
        }
    }
    for targets in adjacency.values_mut() {
        targets.sort_unstable();
        targets.dedup();
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Unvisited,
        InProgress,
        Done,
    }

    let mut marks: BTreeMap<TaskId, Mark> = plan
        .tasks_in_document_order()
        .iter()
        .map(|task| (task.id, Mark::Unvisited))
        .collect();
    let mut found = false;
    let mut reported: BTreeSet<Vec<TaskId>> = BTreeSet::new();

    // Iterative DFS keeps the explicit path available for reporting.
    let roots: Vec<TaskId> = marks.keys().copied().collect();
    for root in roots {
        if marks.get(&root).copied() != Some(Mark::Unvisited) {
            continue;
        }
        let mut path: Vec<TaskId> = Vec::new();
        let mut stack: Vec<(TaskId, usize)> = vec![(root, 0)];
        marks.insert(root, Mark::InProgress);
        path.push(root);

        while let Some((node, child_index)) = stack.pop() {
            let children = adjacency.get(&node).cloned().unwrap_or_default();
            if child_index < children.len() {
                stack.push((node, child_index + 1));
                let Some(next) = children.get(child_index).copied() else {
                    continue;
                };
                match marks.get(&next).copied() {
                    Some(Mark::InProgress) => {
                        // Cycle: slice the path from `next` and close the loop.
                        if let Some(position) = path.iter().position(|id| *id == next) {
                            let mut cycle = path[position..].to_vec();
                            cycle.push(next);
                            if reported.insert(cycle.clone()) {
                                let rendered: Vec<String> =
                                    cycle.iter().map(|id| id.as_token()).collect();
                                issues.push(
                                    ValidationIssue::error(
                                        "V-CYCLE",
                                        ElementRef::Dependency {
                                            predecessor: node,
                                            successor: next,
                                        },
                                        format!("依赖成环：{}", rendered.join(" → ")),
                                        format!(
                                            "断开环中任一依赖，例如 {} → {}",
                                            node.as_token(),
                                            next.as_token()
                                        ),
                                    )
                                    .with_cycle_path(cycle),
                                );
                                found = true;
                            }
                        }
                    }
                    Some(Mark::Unvisited) => {
                        marks.insert(next, Mark::InProgress);
                        path.push(next);
                        stack.push((next, 0));
                    }
                    _ => {}
                }
            } else {
                marks.insert(node, Mark::Done);
                if path.last() == Some(&node) {
                    path.pop();
                }
            }
        }
    }
    found
}

fn check_parent_containment(
    plan: &Plan,
    index: &PlanIndex<'_>,
    derived: &DerivedDates,
    issues: &mut Vec<ValidationIssue>,
) {
    for task in &plan.tasks {
        let Some(parent_id) = task.parent else {
            continue;
        };
        let Some(parent) = index.task(parent_id) else {
            continue;
        };
        let Some(parent_range) = parent.schedule.explicit_range() else {
            continue;
        };
        let Some(child_range) = derived.range(task.id) else {
            continue;
        };
        if !parent_range.contains(&child_range) {
            issues.push(ValidationIssue::error(
                "V-PARENT",
                ElementRef::Task { id: task.id },
                format!(
                    "子任务 {} 的日期 {}..{} 超出父任务 {} 的范围 {}..{}",
                    task.id,
                    format_date(child_range.start),
                    format_date(child_range.end),
                    parent_id,
                    format_date(parent_range.start),
                    format_date(parent_range.end)
                ),
                "扩大父任务日期范围，或调整子任务日期",
            ));
        }
    }
}

fn check_dependency_order(plan: &Plan, derived: &DerivedDates, issues: &mut Vec<ValidationIssue>) {
    for dep in &plan.dependencies {
        if dep.predecessor == dep.successor {
            continue;
        }
        let (Some(pred), Some(succ)) =
            (derived.range(dep.predecessor), derived.range(dep.successor))
        else {
            continue;
        };
        if succ.start < pred.end {
            issues.push(ValidationIssue::error(
                "V-ORDER",
                ElementRef::Dependency {
                    predecessor: dep.predecessor,
                    successor: dep.successor,
                },
                format!(
                    "后继任务 {} 在 {} 开始，早于前置任务 {} 的结束日期 {}",
                    dep.successor,
                    format_date(succ.start),
                    dep.predecessor,
                    format_date(pred.end)
                ),
                "推迟后继任务的开始日期，或缩短前置任务",
            ));
        }
    }
}

fn check_milestones(plan: &Plan, derived: &DerivedDates, issues: &mut Vec<ValidationIssue>) {
    for milestone in &plan.milestones {
        for task_id in &milestone.linked_tasks {
            let Some(range) = derived.range(*task_id) else {
                continue;
            };
            if milestone.date < range.end {
                issues.push(ValidationIssue::error(
                    "V-MSTONE",
                    ElementRef::Milestone { id: milestone.id },
                    format!(
                        "里程碑 {} 的日期 {} 早于关联任务 {} 的结束日期 {}",
                        milestone.id,
                        format_date(milestone.date),
                        task_id,
                        format_date(range.end)
                    ),
                    "推迟里程碑日期，或提前关联任务",
                ));
            }
        }
    }
}

fn check_missing_dates(
    plan: &Plan,
    index: &PlanIndex<'_>,
    derived: &DerivedDates,
    issues: &mut Vec<ValidationIssue>,
) {
    for task in &plan.tasks {
        let is_leaf = index.is_leaf(task.id);
        if is_leaf && derived.range(task.id).is_none() {
            issues.push(ValidationIssue::warning(
                "W-NODATE",
                ElementRef::Task { id: task.id },
                format!("任务 {} 没有任何日期信息，无法在时间线中定位", task.id),
                "补充 [起止日期] 或 [Nd] 工期",
            ));
        }
    }
}

fn check_orphan_clusters(plan: &Plan, index: &PlanIndex<'_>, issues: &mut Vec<ValidationIssue>) {
    // Only meaningful once dependencies exist at all.
    if plan.dependencies.is_empty() || plan.tasks.len() < 2 {
        return;
    }
    let connected: BTreeSet<TaskId> = plan
        .dependencies
        .iter()
        .flat_map(|dep| [dep.predecessor, dep.successor])
        .filter(|id| index.has_task(*id))
        .collect();
    for task in &plan.tasks {
        let is_leaf = index.is_leaf(task.id);
        if is_leaf && !connected.contains(&task.id) {
            issues.push(ValidationIssue::warning(
                "W-ORPHAN",
                ElementRef::Task { id: task.id },
                format!("任务 {} 与依赖网络中的其他任务没有连接", task.id),
                "确认是否遗漏了前置或后继依赖",
            ));
        }
    }
}

/// Convenience helper for callers that only need the working-day span.
#[must_use]
pub fn span_working_days(range: DateRange) -> u32 {
    working_days_between(range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::parse;

    fn issues_for(source: &str) -> Vec<ValidationIssue> {
        validate(&parse(source).plan)
    }

    fn codes(source: &str) -> Vec<String> {
        issues_for(source).into_iter().map(|i| i.code).collect()
    }

    #[test]
    fn clean_plan_has_no_issues() {
        let source =
            "%mcm 1\n- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2026-09-03..2026-09-04] <-t1\n";
        let issues = issues_for(source);
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn v_title_flags_empty_names() {
        let mut plan = parse("- 占位 #t1\n").plan;
        plan.task_mut(TaskId(1)).unwrap().title = "  ".to_owned();
        let issues = validate(&plan);
        assert!(issues.iter().any(|i| i.code == "V-TITLE"));
    }

    #[test]
    fn v_dup_flags_repeated_ids() {
        assert!(codes("- 甲 #t1\n- 乙 #t1\n").contains(&"V-DUP".to_owned()));
    }

    #[test]
    fn v_ref_flags_missing_dependency_target() {
        let issues = issues_for("- 甲 #t1 <-t9\n");
        let issue = issues.iter().find(|i| i.code == "V-REF").expect("V-REF");
        assert!(!issue.fix_hint.is_empty());
    }

    #[test]
    fn v_self_flags_self_dependency() {
        assert!(codes("- 甲 #t1 <-t1\n").contains(&"V-SELF".to_owned()));
    }

    #[test]
    fn v_cycle_reports_the_full_path() {
        let issues = issues_for("- 甲 #t1 <-t3\n- 乙 #t2 <-t1\n- 丙 #t3 <-t2\n");
        let issue = issues
            .iter()
            .find(|i| i.code == "V-CYCLE")
            .expect("V-CYCLE");
        let path = issue.cycle_path.as_ref().expect("cycle path");
        assert!(path.len() >= 4, "path too short: {path:?}");
        assert_eq!(path.first(), path.last(), "cycle must be closed: {path:?}");
        assert!(!issue.fix_hint.is_empty());
    }

    #[test]
    fn v_hier_flags_ancestor_dependencies() {
        let issues = issues_for("- 父 #t1\n  - 子 #t2 <-t1\n");
        assert!(issues.iter().any(|i| i.code == "V-HIER"));
    }

    #[test]
    fn v_range_flags_reversed_dates() {
        let issues = issues_for("- 甲 #t1 [2026-09-10..2026-09-01]\n");
        assert!(issues.iter().any(|i| i.code == "V-RANGE"));
    }

    #[test]
    fn v_parent_flags_child_outside_parent_window() {
        let source = "- 父 #t1 [2026-09-01..2026-09-05]\n  - 子 #t2 [2026-09-04..2026-09-20]\n";
        let issues = issues_for(source);
        let issue = issues
            .iter()
            .find(|i| i.code == "V-PARENT")
            .expect("V-PARENT");
        assert!(matches!(issue.target, ElementRef::Task { id } if id == TaskId(2)));
    }

    #[test]
    fn v_order_flags_successor_starting_too_early() {
        let source = "- 甲 #t1 [2026-09-10..2026-09-20]\n- 乙 #t2 [2026-09-01..2026-09-05] <-t1\n";
        assert!(codes(source).contains(&"V-ORDER".to_owned()));
    }

    #[test]
    fn v_mstone_flags_early_milestone() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-20]\n! 冻结 #m1 [2026-09-05] <-t1\n";
        assert!(codes(source).contains(&"V-MSTONE".to_owned()));
    }

    #[test]
    fn w_nodate_warns_on_undated_leaf() {
        let issues = issues_for("- 甲 #t1\n");
        let issue = issues
            .iter()
            .find(|i| i.code == "W-NODATE")
            .expect("W-NODATE");
        assert!(!issue.is_error());
    }

    #[test]
    fn w_orphan_warns_on_disconnected_leaf() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2026-09-03..2026-09-04] <-t1\n- 孤立 #t3 [2026-09-01..2026-09-02]\n";
        let issues = issues_for(source);
        let issue = issues
            .iter()
            .find(|i| i.code == "W-ORPHAN")
            .expect("W-ORPHAN");
        assert!(!issue.is_error());
    }

    #[test]
    fn every_issue_has_a_locatable_target_and_hint() {
        let source = "- 甲 #t1 <-t9\n- 乙 #t1 [2026-09-10..2026-09-01]\n";
        for issue in issues_for(source) {
            assert!(!issue.message.is_empty(), "{issue:?}");
            assert!(!issue.fix_hint.is_empty(), "{issue:?}");
            assert!(!matches!(issue.target, ElementRef::Plan), "{issue:?}");
        }
    }

    #[test]
    fn validation_is_deterministic() {
        let source = "- 甲 #t1 <-t3\n- 乙 #t2 <-t1\n- 丙 #t3 <-t2\n- 孤立 #t4\n";
        let baseline = issues_for(source);
        for _ in 0..50 {
            assert_eq!(issues_for(source), baseline);
        }
    }

    #[test]
    fn cycles_suppress_date_dependent_rules_without_hanging() {
        let issues = issues_for("- 甲 #t1 <-t2 [1d]\n- 乙 #t2 <-t1 [1d]\n");
        assert!(issues.iter().any(|i| i.code == "V-CYCLE"));
        assert!(!issues.iter().any(|i| i.code == "V-ORDER"));
    }
}
