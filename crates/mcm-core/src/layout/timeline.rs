//! Timeline (Gantt-style) layout: a date scale on x, one packed lane per row.
//!
//! Tasks without any date information are placed in a separate undated band so
//! they stay visible (and carry `W-NODATE`) instead of silently disappearing.

use chrono::Datelike;

use crate::model::{Date, DateRange, Plan, TaskId};
use crate::validate::derive_dates;

use super::NODE_HEIGHT;

/// Horizontal pixels per calendar day.
pub const DAY_WIDTH: f64 = 26.0;
const LANE_GAP: f64 = 12.0;
/// Height reserved for the date ruler above the bars.
pub const RULER_HEIGHT: f64 = 40.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimelineLayout {
    pub bars: Vec<TimelineBar>,
    pub ticks: Vec<TimelineTick>,
    /// Overall date span, `None` when nothing is dated.
    pub span: Option<DateRange>,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineBar {
    pub id: TaskId,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// True for tasks with no derivable dates (parked in the undated band).
    pub undated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineTick {
    pub x: f64,
    pub label: String,
    /// Month boundaries are emphasised by the renderer.
    pub major: bool,
}

impl TimelineLayout {
    #[must_use]
    pub fn bar(&self, id: TaskId) -> Option<&TimelineBar> {
        self.bars.iter().find(|bar| bar.id == id)
    }
}

fn days_between(from: Date, to: Date) -> i64 {
    (to - from).num_days()
}

/// Lays out dated tasks as bars plus an undated band underneath.
#[must_use]
pub fn layout_timeline(plan: &Plan) -> TimelineLayout {
    let mut layout = TimelineLayout::default();
    let derived = derive_dates(plan);
    let ordered = plan.tasks_in_document_order();
    if ordered.is_empty() {
        return layout;
    }

    let span = derived.envelope();
    layout.span = span;

    let mut row = 0usize;
    // Dated tasks first, in document order, one lane each.
    for task in &ordered {
        let Some(range) = derived.range(task.id) else {
            continue;
        };
        let Some(span) = span else { continue };
        let offset = days_between(span.start, range.start).max(0) as f64;
        let length = (days_between(range.start, range.end) + 1).max(1) as f64;
        layout.bars.push(TimelineBar {
            id: task.id,
            x: offset * DAY_WIDTH,
            y: RULER_HEIGHT + row as f64 * (NODE_HEIGHT + LANE_GAP),
            w: length * DAY_WIDTH,
            h: NODE_HEIGHT,
            undated: false,
        });
        row += 1;
    }

    // Undated tasks are parked in their own band, left-aligned.
    for task in &ordered {
        if derived.range(task.id).is_some() {
            continue;
        }
        layout.bars.push(TimelineBar {
            id: task.id,
            x: 0.0,
            y: RULER_HEIGHT + row as f64 * (NODE_HEIGHT + LANE_GAP),
            w: DAY_WIDTH * 3.0,
            h: NODE_HEIGHT,
            undated: true,
        });
        row += 1;
    }

    if let Some(span) = span {
        let total_days = days_between(span.start, span.end) + 1;
        // One tick per week plus every month boundary keeps the ruler readable.
        let mut cursor = span.start;
        let mut index = 0i64;
        while index < total_days {
            let is_month_start = cursor.day() == 1;
            if index == 0 || is_month_start || index % 7 == 0 {
                layout.ticks.push(TimelineTick {
                    x: index as f64 * DAY_WIDTH,
                    label: if is_month_start || index == 0 {
                        format!(
                            "{}-{:02}-{:02}",
                            cursor.year(),
                            cursor.month(),
                            cursor.day()
                        )
                    } else {
                        format!("{:02}-{:02}", cursor.month(), cursor.day())
                    },
                    major: is_month_start || index == 0,
                });
            }
            let Some(next) = cursor.succ_opt() else { break };
            cursor = next;
            index += 1;
        }
        layout.width = total_days as f64 * DAY_WIDTH;
    }

    layout.height = layout
        .bars
        .iter()
        .map(|bar| bar.y + bar.h)
        .fold(RULER_HEIGHT, f64::max);
    layout.width = layout
        .bars
        .iter()
        .map(|bar| bar.x + bar.w)
        .fold(layout.width, f64::max);
    layout
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_date;
    use crate::outline::parse;

    fn layout_of(source: &str) -> (Plan, TimelineLayout) {
        let plan = parse(source).plan;
        let layout = layout_timeline(&plan);
        (plan, layout)
    }

    #[test]
    fn bar_width_matches_calendar_length() {
        let (_, layout) = layout_of("- 甲 #t1 [2026-09-01..2026-09-05]\n");
        let bar = layout.bar(TaskId(1)).unwrap();
        // Inclusive range of five days.
        assert_eq!(bar.w, 5.0 * DAY_WIDTH);
    }

    #[test]
    fn earliest_task_starts_at_origin() {
        let (_, layout) =
            layout_of("- 甲 #t1 [2026-09-03..2026-09-04]\n- 乙 #t2 [2026-09-01..2026-09-02]\n");
        assert_eq!(layout.bar(TaskId(2)).unwrap().x, 0.0);
        assert!(layout.bar(TaskId(1)).unwrap().x > 0.0);
    }

    #[test]
    fn horizontal_offset_reflects_start_date() {
        let (_, layout) =
            layout_of("- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2026-09-04..2026-09-05]\n");
        // Three days later on the calendar.
        assert_eq!(layout.bar(TaskId(2)).unwrap().x, 3.0 * DAY_WIDTH);
    }

    #[test]
    fn lanes_never_overlap_vertically() {
        let (_, layout) =
            layout_of("- 甲 #t1 [2026-09-01..2026-09-10]\n- 乙 #t2 [2026-09-02..2026-09-11]\n");
        let mut bars = layout.bars.clone();
        bars.sort_by(|a, b| a.y.total_cmp(&b.y));
        for pair in bars.windows(2) {
            assert!(pair[0].y + pair[0].h <= pair[1].y);
        }
    }

    #[test]
    fn undated_tasks_go_to_their_own_band() {
        let (_, layout) = layout_of("- 甲 #t1 [2026-09-01..2026-09-02]\n- 无日期 #t2\n");
        let dated = layout.bar(TaskId(1)).unwrap();
        let undated = layout.bar(TaskId(2)).unwrap();
        assert!(!dated.undated);
        assert!(undated.undated);
        // The undated band sits below every dated lane.
        assert!(undated.y > dated.y);
    }

    #[test]
    fn span_covers_all_dated_tasks() {
        let (_, layout) =
            layout_of("- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2026-09-10..2026-09-12]\n");
        let span = layout.span.expect("span");
        assert_eq!(span.start, parse_date("2026-09-01").unwrap());
        assert_eq!(span.end, parse_date("2026-09-12").unwrap());
    }

    #[test]
    fn ruler_starts_with_a_major_tick() {
        let (_, layout) = layout_of("- 甲 #t1 [2026-09-01..2026-09-20]\n");
        assert!(!layout.ticks.is_empty());
        assert_eq!(layout.ticks[0].x, 0.0);
        assert!(layout.ticks[0].major);
    }

    #[test]
    fn month_boundaries_are_major_ticks() {
        let (_, layout) = layout_of("- 甲 #t1 [2026-09-25..2026-10-05]\n");
        let october = layout
            .ticks
            .iter()
            .find(|tick| tick.label.contains("10-01"))
            .expect("october tick");
        assert!(october.major);
    }

    #[test]
    fn derived_durations_appear_on_the_timeline() {
        let (_, layout) = layout_of("%start 2026-09-01\n- 甲 #t1 [2d]\n- 乙 #t2 [2d] <-t1\n");
        assert!(layout.bar(TaskId(1)).is_some());
        let second = layout.bar(TaskId(2)).unwrap();
        assert!(second.x > 0.0, "successor must start later");
    }

    #[test]
    fn plan_without_dates_has_no_span_but_keeps_bars() {
        let (_, layout) = layout_of("- 甲 #t1\n- 乙 #t2\n");
        assert!(layout.span.is_none());
        assert_eq!(layout.bars.len(), 2);
        assert!(layout.bars.iter().all(|bar| bar.undated));
    }

    #[test]
    fn layout_is_deterministic() {
        let source = "%start 2026-09-01\n- 甲 #t1 [3d]\n- 乙 #t2 [2d] <-t1\n- 丙 #t3\n";
        let (plan, baseline) = layout_of(source);
        for _ in 0..50 {
            assert_eq!(layout_timeline(&plan), baseline);
        }
    }

    #[test]
    fn empty_plan_produces_empty_layout() {
        let (_, layout) = layout_of("");
        assert!(layout.bars.is_empty());
        assert!(layout.span.is_none());
    }
}
