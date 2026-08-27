//! Deterministic outline → model parsing (contracts/outline-grammar.md).
//!
//! Parsing never aborts: every malformed line becomes a `P-*` issue and is
//! quarantined into `Plan::recovered_lines` so the rest of the document still
//! loads (spec FR-015 / US4-3).

use crate::FORMAT_VERSION;
use crate::model::{
    Dependency, ElementRef, Milestone, MilestoneId, Plan, PositionedComment, Schedule, Task,
    TaskId, ValidationIssue, parse_date,
};

use super::lexer::{IndentError, LineKind, Token, lex, tokenize_body};

/// Result of parsing a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOutput {
    pub plan: Plan,
    /// Parse-level findings (`P-*`). Semantic findings come from `validate`.
    pub issues: Vec<ValidationIssue>,
}

fn issue_line(
    code: &str,
    line: u32,
    message: impl Into<String>,
    hint: impl Into<String>,
) -> ValidationIssue {
    ValidationIssue::error(code, ElementRef::Line { line }, message, hint)
}

fn warn_line(
    code: &str,
    line: u32,
    message: impl Into<String>,
    hint: impl Into<String>,
) -> ValidationIssue {
    ValidationIssue::warning(code, ElementRef::Line { line }, message, hint)
}

#[derive(Default)]
struct Ids {
    used_tasks: Vec<u32>,
    used_milestones: Vec<u32>,
}

impl Ids {
    fn next_task(&mut self) -> TaskId {
        let mut candidate = 1u32;
        while self.used_tasks.contains(&candidate) {
            candidate += 1;
        }
        self.used_tasks.push(candidate);
        TaskId(candidate)
    }

    fn next_milestone(&mut self) -> MilestoneId {
        let mut candidate = 1u32;
        while self.used_milestones.contains(&candidate) {
            candidate += 1;
        }
        self.used_milestones.push(candidate);
        MilestoneId(candidate)
    }
}

/// Pre-scans explicit ids so auto-assignment never collides with a later `#tN`.
fn collect_explicit_ids(source: &str) -> Ids {
    let mut ids = Ids::default();
    for line in lex(source) {
        let body = match &line.kind {
            LineKind::Task { body, .. } | LineKind::Milestone { body, .. } => body,
            _ => continue,
        };
        for token in tokenize_body(body) {
            if let Token::Id(raw) = token {
                if let Some(id) = TaskId::parse(&raw) {
                    ids.used_tasks.push(id.0);
                } else if let Some(id) = MilestoneId::parse(&raw) {
                    ids.used_milestones.push(id.0);
                }
            }
        }
    }
    ids
}

struct BodyParts {
    title: String,
    id: Option<String>,
    brackets: Vec<String>,
    assignee: Option<String>,
    predecessors: Vec<String>,
}

fn split_body(body: &str) -> BodyParts {
    let mut parts = BodyParts {
        title: String::new(),
        id: None,
        brackets: Vec::new(),
        assignee: None,
        predecessors: Vec::new(),
    };
    let mut words: Vec<String> = Vec::new();
    for token in tokenize_body(body) {
        match token {
            Token::Word(word) => words.push(word),
            Token::Id(raw) => {
                if parts.id.is_none() {
                    parts.id = Some(raw);
                }
            }
            Token::Bracket(raw) => parts.brackets.push(raw),
            Token::Assignee(raw) => {
                if parts.assignee.is_none() {
                    parts.assignee = Some(raw);
                }
            }
            Token::Predecessor(raw) => parts.predecessors.push(raw),
        }
    }
    parts.title = words.join(" ");
    parts
}

/// Parses `[YYYY-MM-DD..YYYY-MM-DD]` or `[Nd]`.
fn parse_schedule(raw: &str) -> Option<Schedule> {
    if let Some(days) = raw.strip_suffix('d') {
        let days: u32 = days.trim().parse().ok()?;
        if days == 0 {
            return None;
        }
        return Some(Schedule::Duration { days });
    }
    let (start_raw, end_raw) = raw.split_once("..")?;
    let start = parse_date(start_raw.trim())?;
    let end = parse_date(end_raw.trim())?;
    Some(Schedule::Explicit { start, end })
}

/// Parses a whole document. Always returns a plan (possibly partial).
#[must_use]
pub fn parse(source: &str) -> ParseOutput {
    let mut plan = Plan::empty();
    let mut issues = Vec::new();
    let mut ids = collect_explicit_ids(source);

    // Stack of (indent level, task id) used to resolve parents.
    let mut stack: Vec<(usize, TaskId)> = Vec::new();
    let mut pending_comments: Vec<String> = Vec::new();
    let mut last_task: Option<TaskId> = None;
    let mut order_counters: Vec<u32> = vec![0];
    let mut seen_version = false;
    let mut order_by_parent: Vec<(Option<TaskId>, u32)> = Vec::new();

    let lines = lex(source);
    for line in &lines {
        match &line.kind {
            LineKind::Blank => {}
            LineKind::Version { value, .. } => {
                seen_version = true;
                match value.trim().parse::<u32>() {
                    Ok(version) if version == FORMAT_VERSION => {}
                    Ok(version) if version > FORMAT_VERSION => {
                        issues.push(issue_line(
                            "P-001",
                            line.number,
                            format!("文件格式版本 {version} 高于本应用支持的 {FORMAT_VERSION}"),
                            "请更新应用后再打开该文件",
                        ));
                    }
                    _ => {
                        issues.push(issue_line(
                            "P-001",
                            line.number,
                            format!("无法识别的版本头：{value}"),
                            "版本头应形如 `%mcm 1`",
                        ));
                    }
                }
            }
            LineKind::Directive { name, value } => match name.as_str() {
                "title" => plan.title = value.clone(),
                "desc" => plan.description = Some(value.clone()),
                "start" => match parse_date(value.trim()) {
                    Some(date) => plan.project_start = Some(date),
                    None => issues.push(issue_line(
                        "P-003",
                        line.number,
                        format!("无法解析项目起始日期：{value}"),
                        "日期格式应为 YYYY-MM-DD",
                    )),
                },
                other => {
                    issues.push(warn_line(
                        "P-008",
                        line.number,
                        format!("未知指令：%{other}"),
                        "该行已原样保留；如为笔误请修正指令名",
                    ));
                    plan.recovered_lines.push(line.raw.clone());
                }
            },
            LineKind::Comment { text } => pending_comments.push(text.clone()),
            LineKind::Task { indent, done, body } => {
                if let Some(err) = line.indent_error {
                    issues.push(indent_issue(err, line.number));
                }
                let indent = *indent;
                // Level must not jump more than one step at a time.
                let max_allowed = stack.len();
                if indent > max_allowed {
                    issues.push(issue_line(
                        "P-002",
                        line.number,
                        format!("缩进层级跳变过大（期望至多 {max_allowed} 层）"),
                        "每层缩进 2 空格，且一次只能进入一层",
                    ));
                }
                let indent = indent.min(max_allowed);
                stack.truncate(indent);
                let parent = stack.last().map(|(_, id)| *id);

                let parts = split_body(body);
                let id = resolve_task_id(&parts.id, line.number, &mut issues, &mut ids);
                let mut task = Task::new(id, parts.title.clone());
                task.parent = parent;
                task.done = done.unwrap_or(false);
                task.assignee = parts.assignee.clone();

                let order = next_order(&mut order_by_parent, parent);
                task.order = order;

                for bracket in &parts.brackets {
                    match parse_schedule(bracket) {
                        Some(schedule) => task.schedule = schedule,
                        None => issues.push(issue_line(
                            "P-003",
                            line.number,
                            format!("无法解析日期或工期：[{bracket}]"),
                            "使用 [YYYY-MM-DD..YYYY-MM-DD] 或 [Nd]",
                        )),
                    }
                }

                for predecessor in &parts.predecessors {
                    match TaskId::parse(predecessor) {
                        Some(pred) => plan.dependencies.push(Dependency::new(pred, id)),
                        None => issues.push(issue_line(
                            "P-005",
                            line.number,
                            format!("无法解析前置任务引用：<-{predecessor}"),
                            "前置引用应形如 <-t3",
                        )),
                    }
                }

                attach_comments(&mut plan, &mut pending_comments, ElementRef::Task { id });
                plan.tasks.push(task);
                stack.push((indent, id));
                last_task = Some(id);
                let _ = &mut order_counters;
            }
            LineKind::Note { text, .. } => match last_task {
                Some(id) => {
                    if let Some(task) = plan.task_mut(id) {
                        let note = match task.notes.take() {
                            Some(existing) => format!("{existing}\n{text}"),
                            None => text.clone(),
                        };
                        task.notes = Some(note);
                    }
                }
                None => {
                    issues.push(issue_line(
                        "P-007",
                        line.number,
                        "备注行没有对应的任务",
                        "把备注放到某个任务行下方，并缩进 2 空格",
                    ));
                    plan.recovered_lines.push(line.raw.clone());
                }
            },
            LineKind::Milestone { body, .. } => {
                let parts = split_body(body);
                let id = resolve_milestone_id(&parts.id, &mut ids);
                let date = parts.brackets.iter().find_map(|raw| parse_date(raw.trim()));
                let Some(date) = date else {
                    issues.push(issue_line(
                        "P-006",
                        line.number,
                        "里程碑缺少日期",
                        "里程碑行需要 [YYYY-MM-DD] 日期",
                    ));
                    plan.recovered_lines.push(line.raw.clone());
                    continue;
                };
                let mut linked = Vec::new();
                for predecessor in &parts.predecessors {
                    match TaskId::parse(predecessor) {
                        Some(task_id) => linked.push(task_id),
                        None => issues.push(issue_line(
                            "P-005",
                            line.number,
                            format!("无法解析里程碑关联任务：<-{predecessor}"),
                            "关联引用应形如 <-t3",
                        )),
                    }
                }
                attach_comments(
                    &mut plan,
                    &mut pending_comments,
                    ElementRef::Milestone { id },
                );
                plan.milestones.push(Milestone {
                    id,
                    name: parts.title.clone(),
                    date,
                    linked_tasks: linked,
                });
            }
            LineKind::Unknown { raw } => {
                issues.push(issue_line(
                    "P-002",
                    line.number,
                    format!("无法识别的行：{raw}"),
                    "任务行以 `- ` 开头，备注以 `> ` 开头，里程碑以 `! ` 开头",
                ));
                plan.recovered_lines.push(line.raw.clone());
            }
        }
    }

    // Trailing comments belong to the plan itself.
    for text in pending_comments {
        plan.comments.push(PositionedComment { text, before: None });
    }

    if !seen_version && !source.trim().is_empty() {
        issues.push(warn_line(
            "P-001",
            1,
            "缺少版本头 `%mcm 1`",
            "保存时会自动补写版本头",
        ));
    }

    ParseOutput { plan, issues }
}

fn indent_issue(err: IndentError, line: u32) -> ValidationIssue {
    match err {
        IndentError::NotMultipleOfTwo { spaces } => issue_line(
            "P-002",
            line,
            format!("缩进为 {spaces} 个空格，不是 2 的倍数"),
            "每层缩进使用 2 个空格",
        ),
        IndentError::TabUsed => issue_line("P-002", line, "缩进使用了制表符", "改用 2 个空格缩进"),
    }
}

fn next_order(counters: &mut Vec<(Option<TaskId>, u32)>, parent: Option<TaskId>) -> u32 {
    if let Some(entry) = counters.iter_mut().find(|(key, _)| *key == parent) {
        entry.1 += 1;
        return entry.1;
    }
    counters.push((parent, 0));
    0
}

fn resolve_task_id(
    raw: &Option<String>,
    line: u32,
    issues: &mut Vec<ValidationIssue>,
    ids: &mut Ids,
) -> TaskId {
    match raw {
        Some(token) => match TaskId::parse(token) {
            Some(id) => id,
            None => {
                issues.push(issue_line(
                    "P-005",
                    line,
                    format!("无法解析任务标识：#{token}"),
                    "任务标识应形如 #t3",
                ));
                ids.next_task()
            }
        },
        None => ids.next_task(),
    }
}

fn resolve_milestone_id(raw: &Option<String>, ids: &mut Ids) -> MilestoneId {
    raw.as_deref()
        .and_then(MilestoneId::parse)
        .unwrap_or_else(|| ids.next_milestone())
}

fn attach_comments(plan: &mut Plan, pending: &mut Vec<String>, target: ElementRef) {
    for text in pending.drain(..) {
        plan.comments.push(PositionedComment {
            text,
            before: Some(target.clone()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "%mcm 1\n%title 移动端改版\n%start 2026-09-01\n\n# 第一阶段\n- 需求阶段 #t1 @王芳\n  - [x] 用户访谈 #t2 [2026-09-01..2026-09-05]\n  - 竞品分析 #t3 [3d] <-t2\n    > 重点看 A、B 两家的任务视图\n  - 需求评审 #t4 [1d] <-t2 <-t3\n- 设计阶段 #t5\n  - 交互稿 #t6 [5d] <-t4\n! 需求冻结 #m1 [2026-09-10] <-t4\n";

    #[test]
    fn parses_contract_example_without_errors() {
        let out = parse(SAMPLE);
        assert!(
            out.issues.iter().all(|i| !i.is_error()),
            "unexpected errors: {:?}",
            out.issues
        );
        assert_eq!(out.plan.title, "移动端改版");
        assert_eq!(out.plan.tasks.len(), 6);
        assert_eq!(out.plan.milestones.len(), 1);
        assert_eq!(out.plan.dependencies.len(), 4);
    }

    #[test]
    fn builds_the_expected_hierarchy() {
        let out = parse(SAMPLE);
        let t2 = out.plan.task(TaskId(2)).unwrap();
        assert_eq!(t2.parent, Some(TaskId(1)));
        assert!(t2.done);
        let t6 = out.plan.task(TaskId(6)).unwrap();
        assert_eq!(t6.parent, Some(TaskId(5)));
        assert_eq!(out.plan.children_of(None).len(), 2);
    }

    #[test]
    fn dependency_direction_is_predecessor_to_successor() {
        let out = parse(SAMPLE);
        assert!(
            out.plan
                .dependencies
                .contains(&Dependency::new(TaskId(2), TaskId(3)))
        );
        assert!(
            out.plan
                .dependencies
                .contains(&Dependency::new(TaskId(4), TaskId(6)))
        );
    }

    #[test]
    fn captures_schedules_assignee_and_notes() {
        let out = parse(SAMPLE);
        let t1 = out.plan.task(TaskId(1)).unwrap();
        assert_eq!(t1.assignee.as_deref(), Some("王芳"));
        let t3 = out.plan.task(TaskId(3)).unwrap();
        assert_eq!(t3.schedule, Schedule::Duration { days: 3 });
        assert_eq!(t3.notes.as_deref(), Some("重点看 A、B 两家的任务视图"));
        let t2 = out.plan.task(TaskId(2)).unwrap();
        assert!(matches!(t2.schedule, Schedule::Explicit { .. }));
    }

    #[test]
    fn milestone_links_tasks() {
        let out = parse(SAMPLE);
        let milestone = &out.plan.milestones[0];
        assert_eq!(milestone.name, "需求冻结");
        assert_eq!(milestone.linked_tasks, vec![TaskId(4)]);
    }

    #[test]
    fn auto_assigns_ids_without_colliding_with_explicit_ones() {
        let out = parse("- 无 ID 任务\n- 有 ID #t1\n");
        let ids: Vec<TaskId> = out.plan.tasks.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&TaskId(1)));
        // The auto id must not reuse 1, which the second line claims.
        assert!(ids.iter().any(|id| id.0 != 1));
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            2
        );
    }

    #[test]
    fn parsing_is_deterministic_across_repeats() {
        let first = parse(SAMPLE);
        for _ in 0..100 {
            assert_eq!(parse(SAMPLE), first);
        }
    }

    #[test]
    fn p002_reports_indent_jump_and_recovers() {
        let out = parse("- a\n    - too deep\n");
        assert!(out.issues.iter().any(|i| i.code == "P-002"));
        // Parsing continues: both tasks are present.
        assert_eq!(out.plan.tasks.len(), 2);
    }

    #[test]
    fn p003_reports_bad_dates() {
        let out = parse("- a [2026-9-1..2026-09-05]\n");
        let issue = out
            .issues
            .iter()
            .find(|i| i.code == "P-003")
            .expect("P-003");
        assert!(!issue.fix_hint.is_empty());
        assert!(matches!(issue.target, ElementRef::Line { line: 1 }));
    }

    #[test]
    fn p005_reports_bad_reference() {
        let out = parse("- a <-xyz\n");
        assert!(out.issues.iter().any(|i| i.code == "P-005"));
    }

    #[test]
    fn p006_reports_milestone_without_date() {
        let out = parse("! 冻结 #m1\n");
        assert!(out.issues.iter().any(|i| i.code == "P-006"));
        assert_eq!(out.plan.milestones.len(), 0);
        assert_eq!(out.plan.recovered_lines.len(), 1);
    }

    #[test]
    fn p007_reports_orphan_note() {
        let out = parse("> 孤立备注\n");
        assert!(out.issues.iter().any(|i| i.code == "P-007"));
        assert_eq!(out.plan.recovered_lines.len(), 1);
    }

    #[test]
    fn p008_warns_on_unknown_directive_and_keeps_text() {
        let out = parse("%mcm 1\n%unknown 值\n");
        let issue = out
            .issues
            .iter()
            .find(|i| i.code == "P-008")
            .expect("P-008");
        assert!(!issue.is_error());
        assert_eq!(out.plan.recovered_lines.len(), 1);
    }

    #[test]
    fn p001_rejects_future_major_version() {
        let out = parse("%mcm 2\n- a\n");
        let issue = out
            .issues
            .iter()
            .find(|i| i.code == "P-001")
            .expect("P-001");
        assert!(issue.is_error());
    }

    #[test]
    fn missing_version_header_is_a_warning_only() {
        let out = parse("- a\n");
        let issue = out
            .issues
            .iter()
            .find(|i| i.code == "P-001")
            .expect("P-001");
        assert!(!issue.is_error());
    }

    #[test]
    fn comments_are_attached_to_following_element() {
        let out = parse("# 前置说明\n- 任务 #t1\n");
        assert_eq!(out.plan.comments.len(), 1);
        assert_eq!(
            out.plan.comments[0].before,
            Some(ElementRef::Task { id: TaskId(1) })
        );
    }

    #[test]
    fn garbage_lines_are_quarantined_not_fatal() {
        let out = parse("- 正常任务 #t1\n这是乱码行\n- 另一个任务 #t2\n");
        assert_eq!(out.plan.tasks.len(), 2);
        assert_eq!(out.plan.recovered_lines.len(), 1);
    }
}
