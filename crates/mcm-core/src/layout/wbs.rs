//! WBS tree layout: an indented outline-style tree where depth drives the x
//! offset and document order drives y. Deterministic by construction.

use crate::model::{Plan, TaskId};

use super::{H_GAP, NODE_HEIGHT, NODE_WIDTH, V_GAP};

/// Placed task boxes plus parent/child connector anchors.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WbsLayout {
    pub nodes: Vec<WbsNode>,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WbsNode {
    pub id: TaskId,
    pub parent: Option<TaskId>,
    pub depth: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl WbsLayout {
    #[must_use]
    pub fn node(&self, id: TaskId) -> Option<&WbsNode> {
        self.nodes.iter().find(|node| node.id == id)
    }
}

/// Lays out the task forest in document order.
#[must_use]
pub fn layout_wbs(plan: &Plan) -> WbsLayout {
    let mut layout = WbsLayout::default();
    let index = plan.index();

    for (row, task) in plan.tasks_in_document_order().into_iter().enumerate() {
        let depth = index.depth_of(task.id);
        let x = depth as f64 * (H_GAP + NODE_WIDTH * 0.25);
        let y = row as f64 * (NODE_HEIGHT + V_GAP);
        layout.nodes.push(WbsNode {
            id: task.id,
            parent: task.parent,
            depth,
            x,
            y,
            w: NODE_WIDTH,
            h: NODE_HEIGHT,
        });
    }

    layout.width = layout
        .nodes
        .iter()
        .map(|node| node.x + node.w)
        .fold(0.0_f64, f64::max);
    layout.height = layout
        .nodes
        .iter()
        .map(|node| node.y + node.h)
        .fold(0.0_f64, f64::max);
    layout
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::parse;

    fn layout_of(source: &str) -> (Plan, WbsLayout) {
        let plan = parse(source).plan;
        let layout = layout_wbs(&plan);
        (plan, layout)
    }

    #[test]
    fn places_every_task_once() {
        let (plan, layout) = layout_of("- 甲 #t1\n  - 乙 #t2\n- 丙 #t3\n");
        assert_eq!(layout.nodes.len(), plan.tasks.len());
    }

    #[test]
    fn depth_drives_horizontal_offset() {
        let (_, layout) = layout_of("- 甲 #t1\n  - 乙 #t2\n    - 丙 #t3\n");
        let x0 = layout.node(TaskId(1)).unwrap().x;
        let x1 = layout.node(TaskId(2)).unwrap().x;
        let x2 = layout.node(TaskId(3)).unwrap().x;
        assert!(x0 < x1 && x1 < x2, "{x0} {x1} {x2}");
    }

    #[test]
    fn document_order_drives_vertical_order() {
        let (_, layout) = layout_of("- 甲 #t1\n  - 乙 #t2\n- 丙 #t3\n");
        let y1 = layout.node(TaskId(1)).unwrap().y;
        let y2 = layout.node(TaskId(2)).unwrap().y;
        let y3 = layout.node(TaskId(3)).unwrap().y;
        assert!(y1 < y2 && y2 < y3);
    }

    #[test]
    fn rows_never_overlap() {
        let (_, layout) = layout_of("- a #t1\n- b #t2\n- c #t3\n");
        let mut sorted = layout.nodes.clone();
        sorted.sort_by(|a, b| a.y.total_cmp(&b.y));
        for pair in sorted.windows(2) {
            let (upper, lower) = (pair[0], pair[1]);
            assert!(
                upper.y + upper.h <= lower.y,
                "rows overlap: {upper:?} {lower:?}"
            );
        }
    }

    #[test]
    fn bounds_cover_all_nodes() {
        let (_, layout) = layout_of("- 甲 #t1\n  - 乙 #t2\n");
        for node in &layout.nodes {
            assert!(node.x + node.w <= layout.width + f64::EPSILON);
            assert!(node.y + node.h <= layout.height + f64::EPSILON);
        }
    }

    #[test]
    fn layout_is_deterministic() {
        let source = "- 甲 #t1\n  - 乙 #t2\n  - 丙 #t3\n- 丁 #t4\n";
        let (plan, baseline) = layout_of(source);
        for _ in 0..50 {
            assert_eq!(layout_wbs(&plan), baseline);
        }
    }

    #[test]
    fn empty_plan_produces_empty_layout() {
        let (_, layout) = layout_of("");
        assert!(layout.nodes.is_empty());
        assert_eq!(layout.width, 0.0);
        assert_eq!(layout.height, 0.0);
    }
}
