//! Diagram → plan: groups become parent tasks, boxes become tasks, arrows
//! become dependencies. Anything the plan model cannot hold is written as
//! outline comments and listed in the report, never dropped silently (宪法 VI).

use std::collections::BTreeSet;

use mcm_core::model::PositionedComment;
use mcm_core::{Dependency, IdAllocator, Plan, Task, TaskId};
use serde::Serialize;

use crate::diagram::Diagram;
use crate::raster::Rect;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportReport {
    pub nodes: usize,
    pub groups: usize,
    pub dependencies: usize,
    /// Connectors without arrowheads, linked in reading order.
    pub undirected: usize,
    /// Arrows dropped because they closed a cycle, as "A → B".
    pub cycle_breaks: Vec<String>,
    /// Text found outside every box (legends, captions, edge labels).
    pub loose_text: Vec<String>,
    /// Connector text the plan model cannot hold, as "A → B：label".
    pub edge_labels: Vec<String>,
    pub untitled_nodes: usize,
}

#[must_use]
pub fn to_plan(diagram: &Diagram, title: &str) -> (Plan, ImportReport) {
    let mut plan = Plan::empty();
    plan.title = title.trim().to_owned();
    if plan.title.is_empty() {
        plan.title = "导入的图".to_owned();
    }
    let mut ids = IdAllocator::new();
    let mut node_task: Vec<Option<TaskId>> = vec![None; diagram.nodes.len()];

    // Items at one nesting level, in reading order.
    #[derive(Clone, Copy)]
    enum Item {
        Group(usize),
        Node(usize),
    }
    let rect_of = |item: Item| match item {
        Item::Group(g) => diagram.groups[g].rect,
        Item::Node(n) => diagram.nodes[n].rect,
    };
    let top: Vec<Item> = diagram
        .groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.parent.is_none())
        .map(|(i, _)| Item::Group(i))
        .chain(
            (0..diagram.nodes.len())
                .filter(|&n| diagram.group_of_node(n).is_none())
                .map(Item::Node),
        )
        .collect();

    let mut stack: Vec<(Option<TaskId>, Vec<Item>)> = vec![(None, top)];
    while let Some((parent, items)) = stack.pop() {
        let mut items = items;
        let rects: Vec<Rect> = items.iter().map(|&i| rect_of(i)).collect();
        let order = reading_order(&rects);
        items = order.into_iter().map(|k| items[k]).collect();
        for (pos, item) in items.into_iter().enumerate() {
            let id = ids.next_task();
            let mut task = match item {
                Item::Group(g) => {
                    let group = &diagram.groups[g];
                    let children = group
                        .groups
                        .iter()
                        .map(|&c| Item::Group(c))
                        .chain(group.nodes.iter().map(|&n| Item::Node(n)))
                        .collect();
                    stack.push((Some(id), children));
                    Task::new(id, group.title.clone())
                }
                Item::Node(n) => {
                    node_task[n] = Some(id);
                    let node = &diagram.nodes[n];
                    let mut t = Task::new(id, node.title.clone());
                    if !node.detail.is_empty() {
                        t.notes = Some(node.detail.join("\n"));
                    }
                    t
                }
            };
            task.parent = parent;
            task.order = u32::try_from(pos).unwrap_or(u32::MAX);
            plan.tasks.push(task);
        }
    }

    let mut report = ImportReport {
        nodes: diagram.nodes.len(),
        groups: diagram.groups.len(),
        loose_text: diagram.loose_text.clone(),
        untitled_nodes: diagram
            .nodes
            .iter()
            .filter(|n| n.title.starts_with('（'))
            .count(),
        ..ImportReport::default()
    };

    // Dependencies, dropping the arrow that would close a cycle: a plan must
    // be acyclic (V-CYCLE) while an architecture diagram need not be.
    let mut edges: Vec<(TaskId, TaskId, bool, Option<&str>)> = diagram
        .edges
        .iter()
        .filter_map(|e| {
            Some((
                node_task[e.from]?,
                node_task[e.to]?,
                e.undirected,
                e.label.as_deref(),
            ))
        })
        .filter(|(a, b, _, _)| a != b)
        .collect();
    edges.sort_by_key(|&(a, b, _, _)| (a, b));
    let title_of = |id: TaskId| plan.task(id).map_or_else(String::new, |t| t.title.clone());
    let mut kept: Vec<Dependency> = Vec::new();
    for (from, to, undirected, label) in edges {
        let arrow = format!("{} → {}", title_of(from), title_of(to));
        if let Some(label) = label.map(str::trim).filter(|l| !l.is_empty()) {
            // The plan model has no edge text; keep it next to the outline.
            report.edge_labels.push(format!("{arrow}：{label}"));
        }
        if kept
            .iter()
            .any(|d| d.predecessor == from && d.successor == to)
        {
            continue;
        }
        if reaches(&kept, to, from) {
            report.cycle_breaks.push(arrow);
            continue;
        }
        if undirected {
            report.undirected += 1;
        }
        kept.push(Dependency::new(from, to));
    }
    report.dependencies = kept.len();
    plan.dependencies = kept;

    for text in &report.loose_text {
        plan.comments.push(PositionedComment {
            text: format!("图中未归属的文字：{text}"),
            before: None,
        });
    }
    for label in &report.edge_labels {
        plan.comments.push(PositionedComment {
            text: format!("连线说明：{label}"),
            before: None,
        });
    }
    for arrow in &report.cycle_breaks {
        plan.comments.push(PositionedComment {
            text: format!("为避免循环依赖未导入的连线：{arrow}"),
            before: None,
        });
    }
    (plan, report)
}

/// Is `target` reachable from `start` along `deps`?
fn reaches(deps: &[Dependency], start: TaskId, target: TaskId) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(at) = stack.pop() {
        if at == target {
            return true;
        }
        if seen.insert(at) {
            stack.extend(
                deps.iter()
                    .filter(|d| d.predecessor == at)
                    .map(|d| d.successor),
            );
        }
    }
    false
}

/// Row-major order: items whose vertical centres fall inside the band of the
/// row's first item share a row, rows top to bottom, left to right within.
fn reading_order(rects: &[Rect]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..rects.len()).collect();
    idx.sort_by_key(|&i| (rects[i].center().1, rects[i].x0));
    let mut rows: Vec<(Rect, Vec<usize>)> = Vec::new();
    for i in idx {
        let (_, cy) = rects[i].center();
        match rows.last_mut() {
            Some((band, members)) if cy >= band.y0 && cy < band.y1 => members.push(i),
            _ => rows.push((rects[i], vec![i])),
        }
    }
    rows.into_iter()
        .flat_map(|(_, mut members)| {
            members.sort_by_key(|&i| rects[i].x0);
            members
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::{Edge, Group, Node};

    fn node(title: &str, x: i32, y: i32) -> Node {
        Node {
            title: title.into(),
            detail: vec![format!("{title} 说明")],
            rect: Rect::new(x, y, x + 100, y + 40),
        }
    }

    fn sample() -> Diagram {
        Diagram {
            nodes: vec![node("B", 300, 100), node("A", 50, 105), node("C", 50, 300)],
            groups: vec![Group {
                title: "边界".into(),
                rect: Rect::new(20, 80, 450, 200),
                nodes: vec![0, 1],
                groups: vec![],
                parent: None,
            }],
            edges: vec![
                Edge {
                    from: 1,
                    to: 0,
                    undirected: false,
                    label: Some("调用".into()),
                },
                Edge {
                    from: 0,
                    to: 2,
                    undirected: false,
                    label: None,
                },
                Edge {
                    from: 2,
                    to: 1,
                    undirected: false,
                    label: None,
                },
            ],
            loose_text: vec!["Legend".into()],
        }
    }

    #[test]
    fn groups_nest_nodes_in_reading_order() {
        let (plan, _) = to_plan(&sample(), "测试");
        let text = mcm_core::outline::serialize(&plan);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.contains(&"- 边界 #t1"), "{text}");
        assert!(lines.contains(&"  - A #t3 <-t2"), "{text}");
        assert!(lines.contains(&"  - B #t4 <-t3"), "{text}");
        assert!(lines.contains(&"    > A 说明"), "{text}");
    }

    #[test]
    fn cycles_are_broken_and_reported_not_lost() {
        let (plan, report) = to_plan(&sample(), "测试");
        assert_eq!(report.dependencies, 2);
        assert_eq!(report.cycle_breaks.len(), 1);
        let text = mcm_core::outline::serialize(&plan);
        assert!(text.contains("# 为避免循环依赖未导入的连线："), "{text}");
        assert!(text.contains("# 图中未归属的文字：Legend"), "{text}");
        assert!(text.contains("# 连线说明：A → B：调用"), "{text}");
        // The outline is valid input for the regular pipeline.
        let parsed = mcm_core::outline::parse(&text);
        assert!(
            parsed.issues.iter().all(|i| !i.is_error()),
            "{:?}",
            parsed.issues
        );
    }
}
