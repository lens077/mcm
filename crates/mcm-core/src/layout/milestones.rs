//! Milestone band layout: milestones ordered by date on a shared time axis,
//! with connector geometry back to the tasks they gate.

use crate::model::{Date, DateRange, MilestoneId, Plan, TaskId};
use crate::validate::derive_dates;

use super::timeline::{DAY_WIDTH, RULER_HEIGHT};
use super::{NODE_HEIGHT, NODE_WIDTH};

const MARKER_SIZE: f64 = 56.0;
const LINK_ROW_GAP: f64 = 20.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MilestoneLayout {
    pub markers: Vec<MilestoneMarker>,
    pub links: Vec<MilestoneLink>,
    /// Linked tasks rendered as small chips under the band.
    pub task_chips: Vec<TaskChip>,
    pub span: Option<DateRange>,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MilestoneMarker {
    pub id: MilestoneId,
    pub date: Date,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaskChip {
    pub id: TaskId,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MilestoneLink {
    pub milestone: MilestoneId,
    pub task: TaskId,
    pub points: Vec<f64>,
}

impl MilestoneLayout {
    #[must_use]
    pub fn marker(&self, id: MilestoneId) -> Option<&MilestoneMarker> {
        self.markers.iter().find(|marker| marker.id == id)
    }

    #[must_use]
    pub fn chip(&self, id: TaskId) -> Option<&TaskChip> {
        self.task_chips.iter().find(|chip| chip.id == id)
    }
}

fn days_between(from: Date, to: Date) -> i64 {
    (to - from).num_days()
}

/// Lays out the milestone band and its links.
#[must_use]
pub fn layout_milestones(plan: &Plan) -> MilestoneLayout {
    let mut layout = MilestoneLayout::default();
    if plan.milestones.is_empty() {
        return layout;
    }

    let derived = derive_dates(plan);

    // The axis spans every milestone and every linked task window.
    let mut span: Option<DateRange> = None;
    for milestone in &plan.milestones {
        let point = DateRange::new(milestone.date, milestone.date);
        span = Some(match span {
            Some(current) => current.envelope(point),
            None => point,
        });
        for task in &milestone.linked_tasks {
            if let Some(range) = derived.range(*task) {
                span = span.map(|current| current.envelope(range));
            }
        }
    }
    layout.span = span;
    let Some(span) = span else { return layout };

    // Milestones ordered by date, then id, so the band is stable.
    let mut ordered: Vec<_> = plan.milestones.iter().collect();
    ordered.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));

    for milestone in &ordered {
        let offset = days_between(span.start, milestone.date).max(0) as f64;
        layout.markers.push(MilestoneMarker {
            id: milestone.id,
            date: milestone.date,
            x: offset * DAY_WIDTH - MARKER_SIZE / 2.0,
            y: RULER_HEIGHT,
            w: MARKER_SIZE,
            h: MARKER_SIZE,
        });
    }

    // Linked tasks are chipped below the band in document order.
    let mut chip_row = 0usize;
    for task in plan.tasks_in_document_order() {
        let linked = plan
            .milestones
            .iter()
            .any(|milestone| milestone.linked_tasks.contains(&task.id));
        if !linked {
            continue;
        }
        let x = derived
            .range(task.id)
            .map(|range| days_between(span.start, range.end).max(0) as f64 * DAY_WIDTH)
            .unwrap_or(0.0);
        layout.task_chips.push(TaskChip {
            id: task.id,
            x,
            y: RULER_HEIGHT
                + MARKER_SIZE
                + LINK_ROW_GAP
                + chip_row as f64 * (NODE_HEIGHT + LINK_ROW_GAP),
            w: NODE_WIDTH * 0.75,
            h: NODE_HEIGHT,
        });
        chip_row += 1;
    }

    let mut links = Vec::new();
    for milestone in &ordered {
        let Some(marker) = layout.marker(milestone.id).copied() else {
            continue;
        };
        let mut linked = milestone.linked_tasks.clone();
        linked.sort_unstable();
        linked.dedup();
        for task in linked {
            let Some(chip) = layout.chip(task).copied() else {
                continue;
            };
            links.push(MilestoneLink {
                milestone: milestone.id,
                task,
                points: vec![
                    marker.x + marker.w / 2.0,
                    marker.y + marker.h,
                    marker.x + marker.w / 2.0,
                    chip.y + chip.h / 2.0,
                    chip.x + chip.w,
                    chip.y + chip.h / 2.0,
                ],
            });
        }
    }
    layout.links = links;

    layout.width = layout
        .markers
        .iter()
        .map(|m| m.x + m.w)
        .chain(layout.task_chips.iter().map(|c| c.x + c.w))
        .fold(0.0_f64, f64::max);
    layout.height = layout
        .markers
        .iter()
        .map(|m| m.y + m.h)
        .chain(layout.task_chips.iter().map(|c| c.y + c.h))
        .fold(RULER_HEIGHT, f64::max);
    layout
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::parse;

    fn layout_of(source: &str) -> (Plan, MilestoneLayout) {
        let plan = parse(source).plan;
        let layout = layout_milestones(&plan);
        (plan, layout)
    }

    #[test]
    fn milestones_are_ordered_by_date() {
        let source = "! 乙 #m2 [2026-09-20]\n! 甲 #m1 [2026-09-10]\n";
        let (_, layout) = layout_of(source);
        assert_eq!(layout.markers[0].id, MilestoneId(1));
        assert_eq!(layout.markers[1].id, MilestoneId(2));
        assert!(layout.markers[0].x < layout.markers[1].x);
    }

    #[test]
    fn horizontal_position_reflects_the_date() {
        let source = "! 甲 #m1 [2026-09-01]\n! 乙 #m2 [2026-09-11]\n";
        let (_, layout) = layout_of(source);
        let delta =
            layout.marker(MilestoneId(2)).unwrap().x - layout.marker(MilestoneId(1)).unwrap().x;
        assert_eq!(delta, 10.0 * DAY_WIDTH);
    }

    #[test]
    fn linked_tasks_get_chips_and_links() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-05]\n! 冻结 #m1 [2026-09-10] <-t1\n";
        let (_, layout) = layout_of(source);
        assert!(layout.chip(TaskId(1)).is_some());
        assert_eq!(layout.links.len(), 1);
        let link = &layout.links[0];
        assert_eq!(link.milestone, MilestoneId(1));
        assert_eq!(link.task, TaskId(1));
        assert_eq!(link.points.len() % 2, 0);
    }

    #[test]
    fn unlinked_tasks_are_not_chipped() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-05]\n- 乙 #t2 [2026-09-01..2026-09-05]\n! 冻结 #m1 [2026-09-10] <-t1\n";
        let (_, layout) = layout_of(source);
        assert!(layout.chip(TaskId(1)).is_some());
        assert!(layout.chip(TaskId(2)).is_none());
    }

    #[test]
    fn multiple_links_from_one_milestone() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-05]\n- 乙 #t2 [2026-09-01..2026-09-06]\n! 冻结 #m1 [2026-09-10] <-t1 <-t2\n";
        let (_, layout) = layout_of(source);
        assert_eq!(layout.links.len(), 2);
        assert_eq!(layout.task_chips.len(), 2);
    }

    #[test]
    fn span_covers_milestones_and_linked_tasks() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-05]\n! 冻结 #m1 [2026-09-20] <-t1\n";
        let (_, layout) = layout_of(source);
        let span = layout.span.expect("span");
        assert_eq!(span.start, crate::model::parse_date("2026-09-01").unwrap());
        assert_eq!(span.end, crate::model::parse_date("2026-09-20").unwrap());
    }

    #[test]
    fn chips_never_overlap() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-05]\n- 乙 #t2 [2026-09-01..2026-09-06]\n! 冻结 #m1 [2026-09-10] <-t1 <-t2\n";
        let (_, layout) = layout_of(source);
        let mut chips = layout.task_chips.clone();
        chips.sort_by(|a, b| a.y.total_cmp(&b.y));
        for pair in chips.windows(2) {
            assert!(pair[0].y + pair[0].h <= pair[1].y);
        }
    }

    #[test]
    fn layout_is_deterministic() {
        let source = "- 甲 #t1 [2026-09-01..2026-09-05]\n! 甲程碑 #m1 [2026-09-10] <-t1\n! 乙程碑 #m2 [2026-09-20]\n";
        let (plan, baseline) = layout_of(source);
        for _ in 0..50 {
            assert_eq!(layout_milestones(&plan), baseline);
        }
    }

    #[test]
    fn plan_without_milestones_is_empty() {
        let (_, layout) = layout_of("- 甲 #t1 [2026-09-01..2026-09-02]\n");
        assert!(layout.markers.is_empty());
        assert!(layout.links.is_empty());
    }
}
