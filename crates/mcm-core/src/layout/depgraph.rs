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
/// 折行后，带与带之间的额外留白，避免上下两带的连线视觉粘连。
const BAND_GAP: f64 = 56.0;
/// 折行阈值：超过这么多层就换带。
/// 4 层 ≈ 1320 单位宽；单节点链每带高 ~104 单位，折行后宽高比约 4:1，
/// 落在画布能舒适容纳的范围内（取 6 时实测仍有 11:1，偏扁）。
const MAX_RANKS_PER_BAND: usize = 4;
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

/// 每带最多放几层。层数不多时不折行，保持最直观的一条横线。
fn wrap_threshold(rank_count: usize) -> usize {
    if rank_count <= MAX_RANKS_PER_BAND {
        rank_count.max(1)
    } else {
        MAX_RANKS_PER_BAND
    }
}

/// 每一带占多少行——取该带内最拥挤那层的行数。
fn band_row_counts(
    by_rank: &std::collections::BTreeMap<usize, Vec<TaskId>>,
    wrap_after: usize,
) -> Vec<usize> {
    let band_count = by_rank
        .keys()
        .map(|r| r / wrap_after + 1)
        .max()
        .unwrap_or(1);
    let mut rows = vec![0usize; band_count];
    for (rank, row) in by_rank {
        let band = rank / wrap_after;
        rows[band] = rows[band].max(row.len());
    }
    rows
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

    // 长依赖链会把图铺成一条极扁的横带（实测 8 层单节点链 = 2760x124，
    // 宽高比 22:1），画布按包围盒适配后内容缩成一条线，几乎无法阅读。
    // 超过阈值就折行：层序不变，只是每 WRAP_AFTER 层换一「带」往下排。
    let rank_count = by_rank.len();
    let wrap_after = wrap_threshold(rank_count);
    // 每一带的高度取该带内最拥挤那层的行数，避免带与带重叠。
    let band_rows = band_row_counts(&by_rank, wrap_after);

    for (rank, row) in &by_rank {
        let band = *rank / wrap_after;
        let column = *rank % wrap_after;
        let band_offset: usize = band_rows[..band].iter().sum();
        for (index, id) in row.iter().enumerate() {
            layout.nodes.push(DepNode {
                id: *id,
                rank: *rank,
                x: column as f64 * (NODE_WIDTH + RANK_GAP),
                y: (band_offset + index) as f64 * (NODE_HEIGHT + ROW_GAP) + band as f64 * BAND_GAP,
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

    /// 回归：长依赖链曾被铺成极扁的横带（8 层单节点链 = 2760x124，
    /// 宽高比 22:1），画布按包围盒适配后内容缩成一条线，无法阅读。
    #[test]
    fn long_chains_wrap_instead_of_stretching_flat() {
        // 一条 10 环的链，最容易触发该问题
        let mut text = String::from("%mcm 1\n- 甲 #t1\n");
        for n in 2..=10 {
            text.push_str(&format!("- 任务{n} #t{n} <-t{}\n", n - 1));
        }
        let plan = crate::outline::parse(&text).plan;
        let layout = layout_depgraph(&plan);

        let min_x = layout
            .nodes
            .iter()
            .map(|n| n.x)
            .fold(f64::INFINITY, f64::min);
        let max_x = layout
            .nodes
            .iter()
            .map(|n| n.x + n.w)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = layout
            .nodes
            .iter()
            .map(|n| n.y)
            .fold(f64::INFINITY, f64::min);
        let max_y = layout
            .nodes
            .iter()
            .map(|n| n.y + n.h)
            .fold(f64::NEG_INFINITY, f64::max);
        let ratio = (max_x - min_x) / (max_y - min_y).max(1.0);

        assert!(
            ratio < 8.0,
            "依赖图宽高比 {ratio:.1} 过扁，适配后会缩成一条线"
        );
    }

    #[test]
    fn short_chains_stay_on_one_band() {
        // 层数不多时不该折行——一条横线最直观
        let plan = crate::outline::parse("%mcm 1\n- 甲 #t1\n- 乙 #t2 <-t1\n- 丙 #t3 <-t2\n").plan;
        let layout = layout_depgraph(&plan);
        let ys: std::collections::BTreeSet<i64> = layout.nodes.iter().map(|n| n.y as i64).collect();
        assert_eq!(ys.len(), 1, "短链应排在同一行");
    }

    #[test]
    fn wrapped_nodes_never_overlap() {
        let mut text = String::from("%mcm 1\n- 甲 #t1\n");
        for n in 2..=15 {
            text.push_str(&format!("- 任务{n} #t{n} <-t{}\n", n - 1));
        }
        let plan = crate::outline::parse(&text).plan;
        let layout = layout_depgraph(&plan);
        for (i, a) in layout.nodes.iter().enumerate() {
            for b in layout.nodes.iter().skip(i + 1) {
                let overlap =
                    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                assert!(!overlap, "节点 {:?} 与 {:?} 重叠", a.id, b.id);
            }
        }
    }
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
