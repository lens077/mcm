//! Canonical serialization (contracts/outline-grammar.md §规范化序列化).
//!
//! Output order is fixed so files diff cleanly line by line, and ids are always
//! written so references stay stable across sessions.

use crate::FORMAT_VERSION;
use crate::model::{ElementRef, Milestone, Plan, PlanIndex, Schedule, Task, TaskId, format_date};

use super::lexer::escape_title;

/// Renders a plan as canonical outline text, ending with exactly one newline.
#[must_use]
pub fn serialize(plan: &Plan) -> String {
    let index = plan.index();
    let mut out = String::new();
    out.push_str(&format!("%mcm {FORMAT_VERSION}\n"));
    out.push_str(&format!("%title {}\n", plan.title));
    if let Some(start) = plan.project_start {
        out.push_str(&format!("%start {}\n", format_date(start)));
    }
    if let Some(description) = &plan.description {
        out.push_str(&format!("%desc {description}\n"));
    }

    let has_body = !plan.tasks.is_empty() || !plan.milestones.is_empty();
    if has_body {
        out.push('\n');
    }

    for task in plan.tasks_in_document_order() {
        write_comments(&mut out, plan, &ElementRef::Task { id: task.id });
        write_task(&mut out, plan, &index, task);
    }

    for milestone in &plan.milestones {
        write_comments(&mut out, plan, &ElementRef::Milestone { id: milestone.id });
        write_milestone(&mut out, milestone);
    }

    // Trailing comments and quarantined lines land at the end of the file.
    for comment in plan.comments.iter().filter(|c| c.before.is_none()) {
        out.push_str(&format!("# {}\n", comment.text));
    }
    for line in &plan.recovered_lines {
        out.push_str(&format!("# [mcm:recovered] {line}\n"));
    }

    out
}

fn write_comments(out: &mut String, plan: &Plan, target: &ElementRef) {
    for comment in plan
        .comments
        .iter()
        .filter(|c| c.before.as_ref() == Some(target))
    {
        out.push_str(&format!("# {}\n", comment.text));
    }
}

fn write_task(out: &mut String, plan: &Plan, index: &PlanIndex<'_>, task: &Task) {
    let indent = "  ".repeat(index.depth_of(task.id));
    out.push_str(&indent);
    out.push_str("- ");
    if task.done {
        out.push_str("[x] ");
    }
    out.push_str(&escape_title(&task.title));
    out.push_str(&format!(" #{}", task.id.as_token()));

    match task.schedule {
        Schedule::None => {}
        Schedule::Explicit { start, end } => {
            out.push_str(&format!(" [{}..{}]", format_date(start), format_date(end)));
        }
        Schedule::Duration { days } => {
            out.push_str(&format!(" [{days}d]"));
        }
    }

    if let Some(assignee) = &task.assignee {
        // Quote so assignees containing spaces still round-trip.
        if assignee.contains(char::is_whitespace) {
            out.push_str(&format!(" @\"{assignee}\""));
        } else {
            out.push_str(&format!(" @{assignee}"));
        }
    }

    let mut predecessors: Vec<TaskId> = plan
        .dependencies
        .iter()
        .filter(|dep| dep.successor == task.id)
        .map(|dep| dep.predecessor)
        .collect();
    predecessors.sort_unstable();
    predecessors.dedup();
    for predecessor in predecessors {
        out.push_str(&format!(" <-{}", predecessor.as_token()));
    }
    out.push('\n');

    if let Some(notes) = &task.notes {
        let note_indent = "  ".repeat(index.depth_of(task.id) + 1);
        for line in notes.split('\n') {
            out.push_str(&format!("{note_indent}> {line}\n"));
        }
    }
}

fn write_milestone(out: &mut String, milestone: &Milestone) {
    out.push_str("! ");
    out.push_str(&escape_title(&milestone.name));
    out.push_str(&format!(" #{}", milestone.id.as_token()));
    out.push_str(&format!(" [{}]", format_date(milestone.date)));
    let mut linked = milestone.linked_tasks.clone();
    linked.sort_unstable();
    linked.dedup();
    for task in linked {
        out.push_str(&format!(" <-{}", task.as_token()));
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::parser::parse;

    const SAMPLE: &str = "%mcm 1\n%title 移动端改版\n%start 2026-09-01\n\n# 第一阶段\n- 需求阶段 #t1 @王芳\n  - [x] 用户访谈 #t2 [2026-09-01..2026-09-05]\n  - 竞品分析 #t3 [3d] <-t2\n    > 重点看 A、B 两家的任务视图\n  - 需求评审 #t4 [1d] <-t2 <-t3\n- 设计阶段 #t5\n  - 交互稿 #t6 [5d] <-t4\n! 需求冻结 #m1 [2026-09-10] <-t4\n";

    #[test]
    fn serialize_then_parse_is_identity() {
        let plan = parse(SAMPLE).plan;
        let text = serialize(&plan);
        let reparsed = parse(&text).plan;
        assert_eq!(reparsed, plan, "round trip mismatch:\n{text}");
    }

    #[test]
    fn canonical_text_is_a_fixed_point() {
        let plan = parse(SAMPLE).plan;
        let once = serialize(&plan);
        let twice = serialize(&parse(&once).plan);
        assert_eq!(once, twice);
    }

    #[test]
    fn output_ends_with_exactly_one_newline() {
        let plan = parse(SAMPLE).plan;
        let text = serialize(&plan);
        assert!(text.ends_with('\n'));
        assert!(!text.ends_with("\n\n"));
    }

    #[test]
    fn ids_are_always_written() {
        let plan = parse("- 无 ID 的任务\n").plan;
        let text = serialize(&plan);
        assert!(text.contains(" #t1"), "missing id in: {text}");
    }

    #[test]
    fn annotation_order_is_canonical() {
        let plan = parse("- 任务 <-t9 @人 [3d] #t1\n- 前置 #t9\n").plan;
        let text = serialize(&plan);
        let line = text.lines().find(|l| l.contains("#t1")).expect("task line");
        let id_at = line.find("#t1").unwrap();
        let bracket_at = line.find("[3d]").unwrap();
        let owner_at = line.find("@人").unwrap();
        let pred_at = line.find("<-t9").unwrap();
        assert!(
            id_at < bracket_at && bracket_at < owner_at && owner_at < pred_at,
            "{line}"
        );
    }

    #[test]
    fn indentation_is_two_spaces_per_level() {
        let plan = parse("- a #t1\n  - b #t2\n    - c #t3\n").plan;
        let text = serialize(&plan);
        assert!(text.contains("\n- a #t1"));
        assert!(text.contains("\n  - b #t2"));
        assert!(text.contains("\n    - c #t3"));
    }

    #[test]
    fn comments_and_notes_survive_the_round_trip() {
        let plan = parse(SAMPLE).plan;
        let text = serialize(&plan);
        assert!(text.contains("# 第一阶段"));
        assert!(text.contains("> 重点看 A、B 两家的任务视图"));
    }

    #[test]
    fn multiline_notes_round_trip() {
        let source = "- 任务 #t1\n  > 第一行\n  > 第二行\n";
        let plan = parse(source).plan;
        assert_eq!(
            plan.task(TaskId(1)).unwrap().notes.as_deref(),
            Some("第一行\n第二行")
        );
        let reparsed = parse(&serialize(&plan)).plan;
        assert_eq!(reparsed, plan);
    }

    #[test]
    fn titles_with_markers_are_escaped_and_restored() {
        let source = "- \\#标签 普通词 #t1\n";
        let plan = parse(source).plan;
        assert_eq!(plan.task(TaskId(1)).unwrap().title, "#标签 普通词");
        let reparsed = parse(&serialize(&plan)).plan;
        assert_eq!(reparsed, plan);
    }

    #[test]
    fn assignee_with_spaces_round_trips_via_quoting() {
        let mut plan = parse("- 任务 #t1\n").plan;
        plan.task_mut(TaskId(1)).unwrap().assignee = Some("团队 A".to_owned());
        let text = serialize(&plan);
        assert!(text.contains("@\"团队 A\""), "{text}");
        let reparsed = parse(&text).plan;
        assert_eq!(
            reparsed.task(TaskId(1)).unwrap().assignee.as_deref(),
            Some("团队 A")
        );
        assert_eq!(reparsed, plan);
    }

    #[test]
    fn empty_plan_serializes_to_header_only() {
        let plan = Plan::empty();
        let text = serialize(&plan);
        assert_eq!(text, "%mcm 1\n%title 未命名规划\n");
    }
}
