use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::dates::{Date, DateRange};
use super::ids::{MilestoneId, TaskId};

/// How a task is placed on the calendar (data-model.md §基础类型).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// No date information at all.
    #[default]
    None,
    /// Explicit inclusive start/end dates.
    Explicit { start: Date, end: Date },
    /// Working-day duration; start derives from predecessors or project start.
    Duration { days: u32 },
}

impl Schedule {
    #[must_use]
    pub fn explicit_range(&self) -> Option<DateRange> {
        match *self {
            Schedule::Explicit { start, end } => Some(DateRange::new(start, end)),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, Schedule::None)
    }
}

/// A single task node in the WBS forest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub parent: Option<TaskId>,
    /// Sibling ordering; document order is authoritative.
    pub order: u32,
    #[serde(default)]
    pub schedule: Schedule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub done: bool,
}

impl Task {
    #[must_use]
    pub fn new(id: TaskId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            parent: None,
            order: 0,
            schedule: Schedule::None,
            assignee: None,
            notes: None,
            done: false,
        }
    }
}

/// Dependency kind. v1 只支持完成-开始（data-model.md）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    #[default]
    FinishToStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Dependency {
    pub predecessor: TaskId,
    pub successor: TaskId,
    #[serde(default)]
    pub kind: DependencyKind,
}

impl Dependency {
    #[must_use]
    pub fn new(predecessor: TaskId, successor: TaskId) -> Self {
        Self {
            predecessor,
            successor,
            kind: DependencyKind::FinishToStart,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Milestone {
    pub id: MilestoneId,
    pub name: String,
    pub date: Date,
    #[serde(default)]
    pub linked_tasks: Vec<TaskId>,
}

/// Where a validation issue points. `Line` is used by parse errors that cannot
/// be attached to a model element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ElementRef {
    Plan,
    Task {
        id: TaskId,
    },
    Dependency {
        predecessor: TaskId,
        successor: TaskId,
    },
    Milestone {
        id: MilestoneId,
    },
    Line {
        line: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Error,
}

/// One validation or parse finding. `fix_hint` is mandatory (spec FR-004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: String,
    pub target: ElementRef,
    pub message: String,
    pub fix_hint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_path: Option<Vec<TaskId>>,
}

impl ValidationIssue {
    pub fn error(
        code: &str,
        target: ElementRef,
        message: impl Into<String>,
        fix_hint: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Error,
            code: code.to_owned(),
            target,
            message: message.into(),
            fix_hint: fix_hint.into(),
            cycle_path: None,
        }
    }

    pub fn warning(
        code: &str,
        target: ElementRef,
        message: impl Into<String>,
        fix_hint: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.to_owned(),
            target,
            message: message.into(),
            fix_hint: fix_hint.into(),
            cycle_path: None,
        }
    }

    #[must_use]
    pub fn with_cycle_path(mut self, path: Vec<TaskId>) -> Self {
        self.cycle_path = Some(path);
        self
    }

    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Comment lines (`# ...`) preserved verbatim and re-emitted before the element
/// they were attached to (contracts/plan-file-format.md §规范化与人工编辑).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comments {
    /// Comments attached before the plan header block.
    #[serde(default)]
    pub leading: Vec<String>,
    /// Comments attached before a task/milestone, keyed by element.
    #[serde(default)]
    pub trailing: Vec<String>,
}

/// Root aggregate: the single source of truth for every view and export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub format_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_start: Option<Date>,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub milestones: Vec<Milestone>,
    /// Comment lines recovered from the source document, in document order.
    #[serde(default)]
    pub comments: Vec<PositionedComment>,
    /// Lines that could not be parsed and were quarantined (FR-015).
    #[serde(default)]
    pub recovered_lines: Vec<String>,
}

/// A comment plus the element it precedes, so serialization can restore it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionedComment {
    pub text: String,
    /// `None` means the comment trails the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<ElementRef>,
}

impl Default for Plan {
    fn default() -> Self {
        Self::empty()
    }
}

impl Plan {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            title: "未命名规划".to_owned(),
            description: None,
            format_version: crate::FORMAT_VERSION,
            project_start: None,
            tasks: Vec::new(),
            dependencies: Vec::new(),
            milestones: Vec::new(),
            comments: Vec::new(),
            recovered_lines: Vec::new(),
        }
    }

    #[must_use]
    pub fn task(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn task_mut(&mut self, id: TaskId) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|task| task.id == id)
    }

    #[must_use]
    pub fn milestone(&self, id: MilestoneId) -> Option<&Milestone> {
        self.milestones.iter().find(|milestone| milestone.id == id)
    }

    #[must_use]
    pub fn has_task(&self, id: TaskId) -> bool {
        self.task(id).is_some()
    }

    /// Direct children of `parent` in stable document order.
    #[must_use]
    pub fn children_of(&self, parent: Option<TaskId>) -> Vec<&Task> {
        let mut children: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|task| task.parent == parent)
            .collect();
        children.sort_by_key(|task| (task.order, task.id.0));
        children
    }

    /// Depth-first document order over the whole forest.
    ///
    /// Children are grouped in one pass so the walk stays O(n log n) even on
    /// plans with thousands of tasks (performance budget in plan.md).
    #[must_use]
    pub fn tasks_in_document_order(&self) -> Vec<&Task> {
        let mut by_parent: BTreeMap<Option<TaskId>, Vec<&Task>> = BTreeMap::new();
        for task in &self.tasks {
            by_parent.entry(task.parent).or_default().push(task);
        }
        for group in by_parent.values_mut() {
            group.sort_by_key(|task| (task.order, task.id.0));
        }

        let mut ordered = Vec::with_capacity(self.tasks.len());
        let mut stack: Vec<&Task> = by_parent
            .get(&None)
            .map(|roots| roots.iter().rev().copied().collect())
            .unwrap_or_default();
        // A malformed parent chain could otherwise loop forever.
        let mut budget = self.tasks.len().saturating_mul(2) + 1;
        while let Some(task) = stack.pop() {
            if budget == 0 {
                break;
            }
            budget -= 1;
            ordered.push(task);
            if let Some(children) = by_parent.get(&Some(task.id)) {
                for child in children.iter().rev() {
                    stack.push(child);
                }
            }
        }
        ordered
    }

    /// Walks up the parent chain, nearest ancestor first.
    #[must_use]
    pub fn ancestors_of(&self, id: TaskId) -> Vec<TaskId> {
        let mut chain = Vec::new();
        let mut cursor = self.task(id).and_then(|task| task.parent);
        // Guard against malformed cycles in the parent chain.
        let mut guard = self.tasks.len() + 1;
        while let Some(parent) = cursor {
            if guard == 0 {
                break;
            }
            guard -= 1;
            chain.push(parent);
            cursor = self.task(parent).and_then(|task| task.parent);
        }
        chain
    }

    #[must_use]
    pub fn is_ancestor_of(&self, ancestor: TaskId, descendant: TaskId) -> bool {
        self.ancestors_of(descendant).contains(&ancestor)
    }

    #[must_use]
    pub fn depth_of(&self, id: TaskId) -> usize {
        self.ancestors_of(id).len()
    }

    /// Builds lookup tables once so callers that touch every task (validation,
    /// layout) stay linear instead of quadratic.
    #[must_use]
    pub fn index(&self) -> PlanIndex<'_> {
        let mut by_id = BTreeMap::new();
        let mut child_count: BTreeMap<TaskId, usize> = BTreeMap::new();
        for task in &self.tasks {
            by_id.insert(task.id, task);
            if let Some(parent) = task.parent {
                *child_count.entry(parent).or_default() += 1;
            }
        }
        PlanIndex { by_id, child_count }
    }
}

/// Read-only lookup tables over a plan.
#[derive(Debug, Clone)]
pub struct PlanIndex<'a> {
    by_id: BTreeMap<TaskId, &'a Task>,
    child_count: BTreeMap<TaskId, usize>,
}

impl<'a> PlanIndex<'a> {
    #[must_use]
    pub fn task(&self, id: TaskId) -> Option<&'a Task> {
        self.by_id.get(&id).copied()
    }

    #[must_use]
    pub fn has_task(&self, id: TaskId) -> bool {
        self.by_id.contains_key(&id)
    }

    #[must_use]
    pub fn is_leaf(&self, id: TaskId) -> bool {
        self.child_count.get(&id).copied().unwrap_or(0) == 0
    }

    /// Nearest ancestor first, using the index instead of a linear scan.
    #[must_use]
    pub fn ancestors_of(&self, id: TaskId) -> Vec<TaskId> {
        let mut chain = Vec::new();
        let mut cursor = self.task(id).and_then(|task| task.parent);
        let mut guard = self.by_id.len() + 1;
        while let Some(parent) = cursor {
            if guard == 0 {
                break;
            }
            guard -= 1;
            chain.push(parent);
            cursor = self.task(parent).and_then(|task| task.parent);
        }
        chain
    }

    #[must_use]
    pub fn is_ancestor_of(&self, ancestor: TaskId, descendant: TaskId) -> bool {
        self.ancestors_of(descendant).contains(&ancestor)
    }

    #[must_use]
    pub fn depth_of(&self, id: TaskId) -> usize {
        self.ancestors_of(id).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::dates::parse_date;

    fn plan_with_tree() -> Plan {
        let mut plan = Plan::empty();
        let mut root = Task::new(TaskId(1), "需求阶段");
        root.order = 0;
        let mut child_a = Task::new(TaskId(2), "用户访谈");
        child_a.parent = Some(TaskId(1));
        child_a.order = 0;
        let mut child_b = Task::new(TaskId(3), "竞品分析");
        child_b.parent = Some(TaskId(1));
        child_b.order = 1;
        let mut second_root = Task::new(TaskId(4), "设计阶段");
        second_root.order = 1;
        plan.tasks = vec![root, child_a, child_b, second_root];
        plan
    }

    #[test]
    fn empty_plan_uses_current_format_version() {
        let plan = Plan::empty();
        assert_eq!(plan.format_version, crate::FORMAT_VERSION);
        assert!(plan.tasks.is_empty());
    }

    #[test]
    fn document_order_is_depth_first_and_stable() {
        let plan = plan_with_tree();
        let ids: Vec<TaskId> = plan
            .tasks_in_document_order()
            .iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec![TaskId(1), TaskId(2), TaskId(3), TaskId(4)]);
    }

    #[test]
    fn ancestors_and_depth() {
        let plan = plan_with_tree();
        assert_eq!(plan.ancestors_of(TaskId(2)), vec![TaskId(1)]);
        assert_eq!(plan.depth_of(TaskId(2)), 1);
        assert_eq!(plan.depth_of(TaskId(1)), 0);
        assert!(plan.is_ancestor_of(TaskId(1), TaskId(3)));
        assert!(!plan.is_ancestor_of(TaskId(3), TaskId(1)));
    }

    #[test]
    fn schedule_serde_uses_tagged_representation() {
        let start = parse_date("2026-09-01").unwrap();
        let end = parse_date("2026-09-05").unwrap();
        let schedule = Schedule::Explicit { start, end };
        let json = serde_json::to_string(&schedule).unwrap();
        assert!(
            json.contains("\"kind\":\"explicit\""),
            "unexpected json: {json}"
        );
        let restored: Schedule = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, schedule);
    }

    #[test]
    fn validation_issue_carries_fix_hint_and_cycle_path() {
        let issue = ValidationIssue::error(
            "V-CYCLE",
            ElementRef::Dependency {
                predecessor: TaskId(1),
                successor: TaskId(2),
            },
            "依赖成环",
            "断开环中任一依赖",
        )
        .with_cycle_path(vec![TaskId(1), TaskId(2), TaskId(1)]);
        assert!(issue.is_error());
        assert!(!issue.fix_hint.is_empty());
        assert_eq!(issue.cycle_path.as_ref().map(Vec::len), Some(3));
    }

    #[test]
    fn plan_round_trips_through_json() {
        let plan = plan_with_tree();
        let json = serde_json::to_string(&plan).unwrap();
        let restored: Plan = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, plan);
    }
}
