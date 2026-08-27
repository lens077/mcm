//! Layered dependency-graph layout (research.md R7: lightweight Sugiyama).
//!
//! 1. Rank by longest path so every edge points strictly forward.
//! 2. Order within each rank by barycentre, with document order as the stable
//!    tie-break, so the result is deterministic for any input.
//! 3. Route edges as polylines between node borders.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Plan, TaskId};

use super::{NODE_HEIGHT, NODE_WIDTH};

/// Horizontal distance between two ranks (layers).
const RANK_GAP: f64 = 120.0;
/// Vertical distance between two nodes inside one rank.
const ROW_GAP: f64 = 28.0;
/// Barycentre refinement sweeps; a small fixed count keeps layout O(n).
const SWEEPS: usize = 4;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DepGraphLayout {
    pub nodes: Vec<DepNode>,
    pub edges: Vec<DepEdge>,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepNode {
    pub id: TaskId,
    pub rank: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DepEdge {
    pub from: TaskId,
    pub to: TaskId,
    /// Flat `[x0, y0, ...]` polyline from the source border to the target border.
    pub points: Vec<f64>,
}

impl DepGraphLayout {
    #[must_use]
    pub fn node(&self, id: TaskId) -> Option<&DepNode> {
        self.nodes.iter().find(|node| node.id == id)
    }
}

/// Tasks shown in the dependency view: leaves plus anything wired up, so a
/// parent that carries no dependency of its own does not clutter the graph.
fn visible_tasks(plan: &Plan) -> Vec<TaskId> {
    let index = plan.index();
    let mut wired: BTreeSet<TaskId> = BTreeSet::new();
    for dep in &plan.dependencies {
        if index.has_task(dep.predecessor) {
            wired.insert(dep.predecessor);
        }
        if index.has_task(dep.successor) {
            wired.insert(dep.successor);
        }
    }
    plan.tasks_in_document_order()
        .into_iter()
        .filter(|task| index.is_leaf(task.id) || wired.contains(&task.id))
        .map(|task| task.id)
        .collect()
}

fn edges_between(plan: &Plan, visible: &[TaskId]) -> Vec<(TaskId, TaskId)> {
    let visible_set: BTreeSet<TaskId> = visible.iter().copied().collect();
    let mut edges: Vec<(TaskId, TaskId)> = plan
        .dependencies
        .iter()
        .filter(|dep| dep.predecessor != dep.successor)
        .filter(|dep| {
            visible_set.contains(&dep.predecessor) && visible_set.contains(&dep.successor)
        })
        .map(|dep| (dep.predecessor, dep.successor))
        .collect();
    edges.sort_unstable();
    edges.dedup();
    edges
}

/// Longest-path ranking. Cyclic input is tolerated: nodes that never settle
/// keep their last rank, so layout still terminates.
fn rank_nodes(visible: &[TaskId], edges: &[(TaskId, TaskId)]) -> BTreeMap<TaskId, usize> {
    let mut ranks: BTreeMap<TaskId, usize> = visible.iter().map(|id| (*id, 0usize)).collect();
    let limit = visible.len() + 1;
    for _ in 0..limit {
        let mut changed = false;
        for (from, to) in edges {
            let candidate = ranks.get(from).copied().unwrap_or(0) + 1;
            let current = ranks.get(to).copied().unwrap_or(0);
            if candidate > current {
                ranks.insert(*to, candidate);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    ranks
}

/// Orders each rank by the mean position of its neighbours in the previous
/// rank, breaking ties with document order for determinism.
fn order_ranks(
    visible: &[TaskId],
    edges: &[(TaskId, TaskId)],
    ranks: &BTreeMap<TaskId, usize>,
) -> BTreeMap<usize, Vec<TaskId>> {
    let doc_index: BTreeMap<TaskId, usize> =
        visible.iter().enumerate().map(|(i, id)| (*id, i)).collect();

    let mut by_rank: BTreeMap<usize, Vec<TaskId>> = BTreeMap::new();
    for id in visible {
        by_rank
            .entry(ranks.get(id).copied().unwrap_or(0))
            .or_default()
            .push(*id);
    }
    for row in by_rank.values_mut() {
        row.sort_by_key(|id| doc_index.get(id).copied().unwrap_or(usize::MAX));
    }

    let mut predecessors: BTreeMap<TaskId, Vec<TaskId>> = BTreeMap::new();
    for (from, to) in edges {
        predecessors.entry(*to).or_default().push(*from);
    }

    for _ in 0..SWEEPS {
        let snapshot = by_rank.clone();
        let positions: BTreeMap<TaskId, usize> = snapshot
            .values()
            .flat_map(|row| row.iter().enumerate().map(|(i, id)| (*id, i)))
            .collect();

        for (rank, row) in by_rank.iter_mut() {
            if *rank == 0 {
                continue;
            }
            row.sort_by(|a, b| {
                let key = |id: &TaskId| -> (u64, usize) {
                    let bary = predecessors
                        .get(id)
                        .map(|preds| {
                            let sum: usize = preds.iter().filter_map(|p| positions.get(p)).sum();
                            let count = preds.iter().filter(|p| positions.contains_key(p)).count();
                            (sum * 1000)
                                .checked_div(count)
                                .map_or(u64::MAX, |v| v as u64)
                        })
                        .unwrap_or(u64::MAX);
                    (bary, doc_index.get(id).copied().unwrap_or(usize::MAX))
                };
                key(a).cmp(&key(b))
            });
        }
    }
    by_rank
}

/// Lays out the dependency network.
#[must_use]
pub fn layout_depgraph(plan: &Plan) -> DepGraphLayout {
    let mut layout = DepGraphLayout::default();
    let visible = visible_tasks(plan);
    if visible.is_empty() {
        return layout;
    }
    let edges = edges_between(plan, &visible);
    let ranks = rank_nodes(&visible, &edges);
    let by_rank = order_ranks(&visible, &edges, &ranks);

    for (rank, row) in &by_rank {
        for (index, id) in row.iter().enumerate() {
            layout.nodes.push(DepNode {
                id: *id,
                rank: *rank,
                x: *rank as f64 * (NODE_WIDTH + RANK_GAP),
                y: index as f64 * (NODE_HEIGHT + ROW_GAP),
                w: NODE_WIDTH,
                h: NODE_HEIGHT,
            });
        }
    }
    // Stable node order simplifies snapshot assertions downstream.
    layout.nodes.sort_by_key(|node| node.id);

    for (from, to) in &edges {
        let (Some(source), Some(target)) = (layout.node(*from), layout.node(*to)) else {
            continue;
        };
        let start_x = source.x + source.w;
        let start_y = source.y + source.h / 2.0;
        let end_x = target.x;
        let end_y = target.y + target.h / 2.0;
        let mid_x = (start_x + end_x) / 2.0;
        layout.edges.push(DepEdge {
            from: *from,
            to: *to,
            points: vec![start_x, start_y, mid_x, start_y, mid_x, end_y, end_x, end_y],
        });
    }

    layout.width = layout
        .nodes
        .iter()
        .map(|n| n.x + n.w)
        .fold(0.0_f64, f64::max);
    layout.height = layout
        .nodes
        .iter()
        .map(|n| n.y + n.h)
        .fold(0.0_f64, f64::max);
    layout
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::parse;

    fn layout_of(source: &str) -> (Plan, DepGraphLayout) {
        let plan = parse(source).plan;
        let layout = layout_depgraph(&plan);
        (plan, layout)
    }

    #[test]
    fn ranks_follow_dependency_direction() {
        let (_, layout) = layout_of("- 甲 #t1\n- 乙 #t2 <-t1\n- 丙 #t3 <-t2\n");
        assert_eq!(layout.node(TaskId(1)).unwrap().rank, 0);
        assert_eq!(layout.node(TaskId(2)).unwrap().rank, 1);
        assert_eq!(layout.node(TaskId(3)).unwrap().rank, 2);
    }

    #[test]
    fn longest_path_wins_when_ranks_conflict() {
        // t3 depends on both t1 (rank 0) and t2 (rank 1) => rank 2.
        let (_, layout) = layout_of("- 甲 #t1\n- 乙 #t2 <-t1\n- 丙 #t3 <-t1 <-t2\n");
        assert_eq!(layout.node(TaskId(3)).unwrap().rank, 2);
    }

    #[test]
    fn rank_drives_horizontal_position() {
        let (_, layout) = layout_of("- 甲 #t1\n- 乙 #t2 <-t1\n");
        let x1 = layout.node(TaskId(1)).unwrap().x;
        let x2 = layout.node(TaskId(2)).unwrap().x;
        assert!(x2 > x1, "{x1} {x2}");
    }

    #[test]
    fn nodes_in_one_rank_do_not_overlap() {
        let (_, layout) = layout_of("- 甲 #t1\n- 乙 #t2\n- 丙 #t3\n- 汇合 #t4 <-t1 <-t2 <-t3\n");
        let mut rank0: Vec<&DepNode> = layout.nodes.iter().filter(|n| n.rank == 0).collect();
        rank0.sort_by(|a, b| a.y.total_cmp(&b.y));
        for pair in rank0.windows(2) {
            assert!(pair[0].y + pair[0].h <= pair[1].y, "overlap: {pair:?}");
        }
    }

    #[test]
    fn every_dependency_gets_a_polyline() {
        let (plan, layout) = layout_of("- 甲 #t1\n- 乙 #t2 <-t1\n");
        assert_eq!(layout.edges.len(), plan.dependencies.len());
        let edge = &layout.edges[0];
        assert!(edge.points.len() >= 4);
        assert_eq!(edge.points.len() % 2, 0);
    }

    #[test]
    fn edges_start_and_end_on_node_borders() {
        let (_, layout) = layout_of("- 甲 #t1\n- 乙 #t2 <-t1\n");
        let source = layout.node(TaskId(1)).unwrap();
        let target = layout.node(TaskId(2)).unwrap();
        let edge = &layout.edges[0];
        assert_eq!(edge.points[0], source.x + source.w);
        assert_eq!(edge.points[edge.points.len() - 2], target.x);
    }

    #[test]
    fn parents_without_dependencies_are_hidden() {
        // t1 is a pure container: only its children carry dependencies.
        let (_, layout) = layout_of("- 父 #t1\n  - 子甲 #t2\n  - 子乙 #t3 <-t2\n");
        assert!(layout.node(TaskId(1)).is_none());
        assert!(layout.node(TaskId(2)).is_some());
    }

    #[test]
    fn parents_that_carry_dependencies_stay_visible() {
        let (_, layout) = layout_of("- 父 #t1 <-t9\n  - 子 #t2\n- 前置 #t9\n");
        assert!(layout.node(TaskId(1)).is_some());
    }

    #[test]
    fn layout_is_deterministic() {
        let source = "- a #t1\n- b #t2 <-t1\n- c #t3 <-t1\n- d #t4 <-t2 <-t3\n";
        let (plan, baseline) = layout_of(source);
        for _ in 0..50 {
            assert_eq!(layout_depgraph(&plan), baseline);
        }
    }

    #[test]
    fn cyclic_input_terminates() {
        // V-CYCLE reports this, but layout must still return promptly.
        let (_, layout) = layout_of("- 甲 #t1 <-t3\n- 乙 #t2 <-t1\n- 丙 #t3 <-t2\n");
        assert_eq!(layout.nodes.len(), 3);
    }

    #[test]
    fn empty_plan_produces_empty_layout() {
        let (_, layout) = layout_of("");
        assert!(layout.nodes.is_empty());
        assert!(layout.edges.is_empty());
    }
}
