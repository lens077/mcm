//! Scene projection: `scene(plan, view) -> SceneGraph` is a pure function, so
//! rendering can never invent facts that are not in the model (宪法 IV).

use serde::{Deserialize, Serialize};

use crate::layout::{layout_depgraph, layout_milestones, layout_timeline, layout_wbs};
use crate::model::{ElementRef, Plan, PlanIndex, Severity, TaskId, ValidationIssue, format_date};
use crate::validate::derive_dates;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    Wbs,
    DepGraph,
    Timeline,
    Milestones,
}

impl ViewKind {
    /// Every view, used when a change invalidates all projections.
    #[must_use]
    pub fn all() -> Vec<ViewKind> {
        vec![
            ViewKind::Wbs,
            ViewKind::DepGraph,
            ViewKind::Timeline,
            ViewKind::Milestones,
        ]
    }
}

/// Semantic style role; the front end resolves it through design tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleRole {
    Task,
    TaskDone,
    TaskError,
    TaskWarning,
    Milestone,
    HierarchyEdge,
    DependencyEdge,
    MilestoneEdge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeKind {
    Error,
    Warning,
    Done,
    Milestone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneNode {
    #[serde(rename = "ref")]
    pub element: ElementRef,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub style_role: StyleRole,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_text: Option<String>,
    pub badges: Vec<BadgeKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEdge {
    pub from: ElementRef,
    pub to: ElementRef,
    /// Flat `[x0, y0, x1, y1, ...]` polyline for compact transfer.
    pub points: Vec<f64>,
    pub style_role: StyleRole,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneGraph {
    pub view: ViewKind,
    pub nodes: Vec<SceneNode>,
    pub edges: Vec<SceneEdge>,
    pub bounds: SceneBounds,
}

impl SceneGraph {
    fn empty(view: ViewKind) -> Self {
        Self {
            view,
            nodes: Vec::new(),
            edges: Vec::new(),
            bounds: SceneBounds::default(),
        }
    }

    fn recompute_bounds(&mut self) {
        let mut bounds = SceneBounds {
            min_x: f64::MAX,
            min_y: f64::MAX,
            max_x: f64::MIN,
            max_y: f64::MIN,
        };
        let mut seen = false;
        for node in &self.nodes {
            seen = true;
            bounds.min_x = bounds.min_x.min(node.x);
            bounds.min_y = bounds.min_y.min(node.y);
            bounds.max_x = bounds.max_x.max(node.x + node.w);
            bounds.max_y = bounds.max_y.max(node.y + node.h);
        }
        self.bounds = if seen { bounds } else { SceneBounds::default() };
    }
}

/// Worst severity recorded against a task, used to pick its style role.
fn task_severity(issues: &[ValidationIssue], id: TaskId) -> Option<Severity> {
    issues
        .iter()
        .filter(|issue| match &issue.target {
            ElementRef::Task { id: target } => *target == id,
            _ => false,
        })
        .map(|issue| issue.severity)
        .max()
}

/// Style role and badges for a task, shared by every view so a task looks the
/// same wherever it appears.
fn task_appearance(
    index: &PlanIndex<'_>,
    issues: &[ValidationIssue],
    id: TaskId,
) -> (StyleRole, Vec<BadgeKind>) {
    let severity = task_severity(issues, id);
    let done = index.task(id).is_some_and(|task| task.done);
    let style_role = match (severity, done) {
        (Some(Severity::Error), _) => StyleRole::TaskError,
        (Some(Severity::Warning), _) => StyleRole::TaskWarning,
        (None, true) => StyleRole::TaskDone,
        (None, false) => StyleRole::Task,
    };
    let mut badges = Vec::new();
    match severity {
        Some(Severity::Error) => badges.push(BadgeKind::Error),
        Some(Severity::Warning) => badges.push(BadgeKind::Warning),
        None => {}
    }
    if done {
        badges.push(BadgeKind::Done);
    }
    (style_role, badges)
}

/// Projects a plan into the requested view.
#[must_use]
pub fn scene(plan: &Plan, view: ViewKind, issues: &[ValidationIssue]) -> SceneGraph {
    match view {
        ViewKind::Wbs => scene_wbs(plan, issues),
        ViewKind::DepGraph => scene_depgraph(plan, issues),
        ViewKind::Timeline => scene_timeline(plan, issues),
        ViewKind::Milestones => scene_milestones(plan, issues),
    }
}

fn scene_depgraph(plan: &Plan, issues: &[ValidationIssue]) -> SceneGraph {
    let index = plan.index();
    let layout = layout_depgraph(plan);
    let mut graph = SceneGraph::empty(ViewKind::DepGraph);

    for node in &layout.nodes {
        let Some(task) = index.task(node.id) else {
            continue;
        };
        let (style_role, badges) = task_appearance(&index, issues, node.id);
        graph.nodes.push(SceneNode {
            element: ElementRef::Task { id: node.id },
            x: node.x,
            y: node.y,
            w: node.w,
            h: node.h,
            style_role,
            text: task.title.clone(),
            sub_text: task.assignee.as_ref().map(|owner| format!("@{owner}")),
            badges,
        });
    }

    for edge in &layout.edges {
        // A dependency inherits the error styling of the cycle it belongs to.
        let has_error = issues.iter().any(|issue| {
            issue.is_error()
                && matches!(
                    &issue.target,
                    ElementRef::Dependency { predecessor, successor }
                        if *predecessor == edge.from && *successor == edge.to
                )
        });
        graph.edges.push(SceneEdge {
            from: ElementRef::Task { id: edge.from },
            to: ElementRef::Task { id: edge.to },
            points: edge.points.clone(),
            style_role: if has_error {
                StyleRole::TaskError
            } else {
                StyleRole::DependencyEdge
            },
        });
    }

    graph.recompute_bounds();
    graph
}

fn scene_timeline(plan: &Plan, issues: &[ValidationIssue]) -> SceneGraph {
    let index = plan.index();
    let layout = layout_timeline(plan);
    let derived = derive_dates(plan);
    let mut graph = SceneGraph::empty(ViewKind::Timeline);

    for bar in &layout.bars {
        let Some(task) = index.task(bar.id) else {
            continue;
        };
        let (style_role, badges) = task_appearance(&index, issues, bar.id);
        let sub_text = derived
            .range(bar.id)
            .map(|range| format!("{}..{}", format_date(range.start), format_date(range.end)))
            .or_else(|| Some("无日期".to_owned()));
        graph.nodes.push(SceneNode {
            element: ElementRef::Task { id: bar.id },
            x: bar.x,
            y: bar.y,
            w: bar.w,
            h: bar.h,
            style_role,
            text: task.title.clone(),
            sub_text,
            badges,
        });
    }

    // Dependencies are drawn as thin links between bar ends.
    for dep in &plan.dependencies {
        let (Some(from), Some(to)) = (layout.bar(dep.predecessor), layout.bar(dep.successor))
        else {
            continue;
        };
        graph.edges.push(SceneEdge {
            from: ElementRef::Task {
                id: dep.predecessor,
            },
            to: ElementRef::Task { id: dep.successor },
            points: vec![
                from.x + from.w,
                from.y + from.h / 2.0,
                to.x,
                to.y + to.h / 2.0,
            ],
            style_role: StyleRole::DependencyEdge,
        });
    }

    graph.recompute_bounds();
    graph
}

fn scene_milestones(plan: &Plan, issues: &[ValidationIssue]) -> SceneGraph {
    let index = plan.index();
    let layout = layout_milestones(plan);
    let mut graph = SceneGraph::empty(ViewKind::Milestones);

    for marker in &layout.markers {
        let Some(milestone) = plan.milestone(marker.id) else {
            continue;
        };
        let has_error = issues.iter().any(|issue| {
            issue.is_error()
                && matches!(&issue.target, ElementRef::Milestone { id } if *id == marker.id)
        });
        graph.nodes.push(SceneNode {
            element: ElementRef::Milestone { id: marker.id },
            x: marker.x,
            y: marker.y,
            w: marker.w,
            h: marker.h,
            style_role: if has_error {
                StyleRole::TaskError
            } else {
                StyleRole::Milestone
            },
            text: milestone.name.clone(),
            sub_text: Some(format_date(milestone.date)),
            badges: if has_error {
                vec![BadgeKind::Error, BadgeKind::Milestone]
            } else {
                vec![BadgeKind::Milestone]
            },
        });
    }

    for chip in &layout.task_chips {
        let Some(task) = index.task(chip.id) else {
            continue;
        };
        let (style_role, badges) = task_appearance(&index, issues, chip.id);
        graph.nodes.push(SceneNode {
            element: ElementRef::Task { id: chip.id },
            x: chip.x,
            y: chip.y,
            w: chip.w,
            h: chip.h,
            style_role,
            text: task.title.clone(),
            sub_text: None,
            badges,
        });
    }

    for link in &layout.links {
        graph.edges.push(SceneEdge {
            from: ElementRef::Milestone { id: link.milestone },
            to: ElementRef::Task { id: link.task },
            points: link.points.clone(),
            style_role: StyleRole::MilestoneEdge,
        });
    }

    graph.recompute_bounds();
    graph
}

fn scene_wbs(plan: &Plan, issues: &[ValidationIssue]) -> SceneGraph {
    let index = plan.index();
    let layout = layout_wbs(plan);
    let derived = derive_dates(plan);
    let mut graph = SceneGraph::empty(ViewKind::Wbs);

    for node in &layout.nodes {
        let Some(task) = index.task(node.id) else {
            continue;
        };
        let (style_role, badges) = task_appearance(&index, issues, node.id);

        // Sub text carries only facts already in the model.
        let mut fragments = Vec::new();
        if let Some(range) = derived.range(node.id) {
            fragments.push(format!(
                "{}..{}",
                format_date(range.start),
                format_date(range.end)
            ));
        }
        if let Some(assignee) = &task.assignee {
            fragments.push(format!("@{assignee}"));
        }
        let sub_text = if fragments.is_empty() {
            None
        } else {
            Some(fragments.join(" · "))
        };

        graph.nodes.push(SceneNode {
            element: ElementRef::Task { id: node.id },
            x: node.x,
            y: node.y,
            w: node.w,
            h: node.h,
            style_role,
            text: task.title.clone(),
            sub_text,
            badges,
        });
    }

    // Hierarchy connectors: elbow from parent's left rail down to the child.
    for node in &layout.nodes {
        let Some(parent_id) = node.parent else {
            continue;
        };
        let Some(parent) = layout.node(parent_id) else {
            continue;
        };
        let rail_x = parent.x + 16.0;
        let start_y = parent.y + parent.h;
        let end_y = node.y + node.h / 2.0;
        graph.edges.push(SceneEdge {
            from: ElementRef::Task { id: parent_id },
            to: ElementRef::Task { id: node.id },
            points: vec![rail_x, start_y, rail_x, end_y, node.x, end_y],
            style_role: StyleRole::HierarchyEdge,
        });
    }

    graph.recompute_bounds();
    graph
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::parse;
    use crate::validate::validate;

    fn scene_of(source: &str) -> SceneGraph {
        let plan = parse(source).plan;
        let issues = validate(&plan);
        scene(&plan, ViewKind::Wbs, &issues)
    }

    #[test]
    fn projects_every_task_as_a_node() {
        let graph = scene_of("- 甲 #t1\n  - 乙 #t2\n- 丙 #t3\n");
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.view, ViewKind::Wbs);
    }

    #[test]
    fn node_text_comes_from_the_model() {
        let graph = scene_of("- 需求阶段 #t1\n");
        assert_eq!(graph.nodes[0].text, "需求阶段");
    }

    #[test]
    fn hierarchy_edges_connect_parents_to_children() {
        let graph = scene_of("- 甲 #t1\n  - 乙 #t2\n");
        assert_eq!(graph.edges.len(), 1);
        let edge = &graph.edges[0];
        assert_eq!(edge.from, ElementRef::Task { id: TaskId(1) });
        assert_eq!(edge.to, ElementRef::Task { id: TaskId(2) });
        assert_eq!(edge.style_role, StyleRole::HierarchyEdge);
        assert!(edge.points.len() >= 4 && edge.points.len() % 2 == 0);
    }

    #[test]
    fn done_tasks_get_the_done_role_and_badge() {
        let graph = scene_of(
            "- 甲 #t1 [2026-09-01..2026-09-02]\n  - [x] 乙 #t2 [2026-09-01..2026-09-02]\n",
        );
        let done = graph
            .nodes
            .iter()
            .find(|n| n.element == ElementRef::Task { id: TaskId(2) })
            .expect("t2");
        assert_eq!(done.style_role, StyleRole::TaskDone);
        assert!(done.badges.contains(&BadgeKind::Done));
    }

    #[test]
    fn tasks_with_errors_are_marked() {
        // Reversed dates raise V-RANGE against t1.
        let graph = scene_of("- 甲 #t1 [2026-09-10..2026-09-01]\n");
        let node = &graph.nodes[0];
        assert_eq!(node.style_role, StyleRole::TaskError);
        assert!(node.badges.contains(&BadgeKind::Error));
    }

    #[test]
    fn warnings_are_marked_but_distinct_from_errors() {
        // An undated leaf raises W-NODATE only.
        let graph = scene_of("- 甲 #t1\n");
        let node = &graph.nodes[0];
        assert_eq!(node.style_role, StyleRole::TaskWarning);
        assert!(node.badges.contains(&BadgeKind::Warning));
    }

    #[test]
    fn sub_text_shows_derived_dates_and_assignee() {
        let graph = scene_of("- 甲 #t1 [2026-09-01..2026-09-05] @王芳\n");
        let sub = graph.nodes[0].sub_text.as_deref().expect("sub text");
        assert!(sub.contains("2026-09-01..2026-09-05"), "{sub}");
        assert!(sub.contains("@王芳"), "{sub}");
    }

    #[test]
    fn bounds_cover_every_node() {
        let graph = scene_of("- 甲 #t1\n  - 乙 #t2\n");
        for node in &graph.nodes {
            assert!(node.x >= graph.bounds.min_x);
            assert!(node.y >= graph.bounds.min_y);
            assert!(node.x + node.w <= graph.bounds.max_x);
            assert!(node.y + node.h <= graph.bounds.max_y);
        }
    }

    #[test]
    fn empty_plan_projects_empty_scene() {
        let graph = scene_of("");
        assert!(graph.nodes.is_empty());
        assert_eq!(graph.bounds, SceneBounds::default());
    }

    #[test]
    fn projection_is_deterministic() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-02]\n  - 乙 #t2 [1d] <-t1\n";
        let plan = parse(source).plan;
        let issues = validate(&plan);
        let baseline = scene(&plan, ViewKind::Wbs, &issues);
        for _ in 0..50 {
            assert_eq!(scene(&plan, ViewKind::Wbs, &issues), baseline);
        }
    }

    fn scene_for(source: &str, view: ViewKind) -> SceneGraph {
        let plan = parse(source).plan;
        let issues = validate(&plan);
        scene(&plan, view, &issues)
    }

    const WITH_ALL: &str = "%mcm 1\n%start 2026-09-01\n- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2026-09-03..2026-09-04] <-t1\n! 冻结 #m1 [2026-09-10] <-t2\n";

    #[test]
    fn dep_graph_projects_nodes_and_edges() {
        let graph = scene_for(WITH_ALL, ViewKind::DepGraph);
        assert_eq!(graph.view, ViewKind::DepGraph);
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].style_role, StyleRole::DependencyEdge);
    }

    #[test]
    fn dep_graph_marks_cycle_edges_as_errors() {
        let graph = scene_for("- 甲 #t1 <-t2\n- 乙 #t2 <-t1\n", ViewKind::DepGraph);
        assert!(
            graph
                .edges
                .iter()
                .any(|e| e.style_role == StyleRole::TaskError)
        );
    }

    #[test]
    fn timeline_projects_bars_with_date_sub_text() {
        let graph = scene_for(WITH_ALL, ViewKind::Timeline);
        assert_eq!(graph.view, ViewKind::Timeline);
        assert_eq!(graph.nodes.len(), 2);
        let sub = graph.nodes[0].sub_text.as_deref().expect("dates");
        assert!(sub.contains("2026-09-01"), "{sub}");
    }

    #[test]
    fn timeline_labels_undated_tasks() {
        let graph = scene_for("- 无日期 #t1\n", ViewKind::Timeline);
        assert_eq!(graph.nodes[0].sub_text.as_deref(), Some("无日期"));
    }

    #[test]
    fn milestones_project_markers_chips_and_links() {
        let graph = scene_for(WITH_ALL, ViewKind::Milestones);
        assert_eq!(graph.view, ViewKind::Milestones);
        let marker = graph
            .nodes
            .iter()
            .find(|n| matches!(n.element, ElementRef::Milestone { .. }))
            .expect("marker");
        assert_eq!(marker.style_role, StyleRole::Milestone);
        assert!(marker.badges.contains(&BadgeKind::Milestone));
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].style_role, StyleRole::MilestoneEdge);
    }

    #[test]
    fn all_views_are_deterministic() {
        let plan = parse(WITH_ALL).plan;
        let issues = validate(&plan);
        for view in [
            ViewKind::Wbs,
            ViewKind::DepGraph,
            ViewKind::Timeline,
            ViewKind::Milestones,
        ] {
            let baseline = scene(&plan, view, &issues);
            for _ in 0..25 {
                assert_eq!(
                    scene(&plan, view, &issues),
                    baseline,
                    "{view:?} not deterministic"
                );
            }
        }
    }

    #[test]
    fn every_view_reports_its_own_kind_and_bounds() {
        for view in [
            ViewKind::Wbs,
            ViewKind::DepGraph,
            ViewKind::Timeline,
            ViewKind::Milestones,
        ] {
            let graph = scene_for(WITH_ALL, view);
            assert_eq!(graph.view, view);
            for node in &graph.nodes {
                assert!(node.x + node.w <= graph.bounds.max_x + f64::EPSILON);
                assert!(node.y + node.h <= graph.bounds.max_y + f64::EPSILON);
            }
        }
    }

    #[test]
    fn empty_plan_projects_empty_scene_in_every_view() {
        for view in [
            ViewKind::Wbs,
            ViewKind::DepGraph,
            ViewKind::Timeline,
            ViewKind::Milestones,
        ] {
            let graph = scene_for("", view);
            assert!(graph.nodes.is_empty(), "{view:?} should be empty");
        }
    }

    #[test]
    fn serializes_with_flat_point_arrays() {
        let graph = scene_of("- 甲 #t1\n  - 乙 #t2\n");
        let json = serde_json::to_string(&graph).expect("serialize");
        assert!(json.contains("\"points\":["), "{json}");
        assert!(json.contains("\"style_role\":\"hierarchy_edge\""), "{json}");
    }
}
