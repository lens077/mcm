//! User Story 1 acceptance scenarios from spec.md, asserted at model level.
//!
//! 1. 完整描述 → 完整规划、零问题
//! 2. 循环依赖 → 报告完整环路径并定位
//! 3. 子任务日期超出父任务 → 定位冲突并给出修复建议
//! 4. 引用不存在的任务 → 引用完整性错误并定位

use mcm_core::model::{ElementRef, Severity, TaskId};
use mcm_core::outline::parse;
use mcm_core::scene::{StyleRole, ViewKind, scene};
use mcm_core::validate::validate;

/// Parses and validates, returning both the plan and the merged issue list.
fn generate(source: &str) -> (mcm_core::model::Plan, Vec<mcm_core::model::ValidationIssue>) {
    let parsed = parse(source);
    let mut issues = parsed.issues;
    issues.extend(validate(&parsed.plan));
    (parsed.plan, issues)
}

const HEALTHY: &str = "%mcm 1
%title 移动端改版
%start 2026-09-01

- 需求阶段 #t1 [2026-09-01..2026-09-30]
  - 用户访谈 #t2 [2026-09-01..2026-09-04]
  - 竞品分析 #t3 [3d] <-t2
- 设计阶段 #t5 [2026-09-10..2026-09-30]
  - 交互稿 #t6 [2026-09-14..2026-09-18] <-t3
! 需求冻结 #m1 [2026-09-30] <-t6
";

#[test]
fn scenario_1_full_description_generates_a_clean_plan() {
    let (plan, issues) = generate(HEALTHY);

    // Structure is complete.
    assert_eq!(plan.tasks.len(), 5);
    assert_eq!(plan.milestones.len(), 1);
    assert_eq!(plan.dependencies.len(), 2);
    assert_eq!(plan.children_of(None).len(), 2);

    // And the issue panel is empty.
    assert!(issues.is_empty(), "expected a clean plan, got: {issues:?}");

    // The WBS view renders every task with no error styling.
    let graph = scene(&plan, ViewKind::Wbs, &issues);
    assert_eq!(graph.nodes.len(), 5);
    assert!(
        graph
            .nodes
            .iter()
            .all(|n| n.style_role != StyleRole::TaskError)
    );
}

#[test]
fn scenario_2_cycle_is_reported_with_the_full_path() {
    let source = "%mcm 1\n- 甲 #t1 <-t3\n- 乙 #t2 <-t1\n- 丙 #t3 <-t2\n";
    let (_, issues) = generate(source);

    let cycle = issues
        .iter()
        .find(|issue| issue.code == "V-CYCLE")
        .expect("V-CYCLE must be reported");

    assert_eq!(cycle.severity, Severity::Error);

    // The path is complete and closed: t1 → t2 → t3 → t1 (in some rotation).
    let path = cycle
        .cycle_path
        .as_ref()
        .expect("cycle path must be present");
    assert_eq!(path.first(), path.last(), "cycle must close: {path:?}");
    assert_eq!(
        path.len(),
        4,
        "expected three nodes plus the closing one: {path:?}"
    );
    for id in [TaskId(1), TaskId(2), TaskId(3)] {
        assert!(path.contains(&id), "cycle path missing {id}: {path:?}");
    }

    // It is located on a concrete dependency and carries a fix hint.
    assert!(matches!(cycle.target, ElementRef::Dependency { .. }));
    assert!(!cycle.fix_hint.is_empty());
}

#[test]
fn scenario_3_child_outside_parent_window_is_located() {
    let source = "%mcm 1
- 父任务 #t1 [2026-09-01..2026-09-05]
  - 子任务 #t2 [2026-09-04..2026-09-20]
";
    let (_, issues) = generate(source);

    let conflict = issues
        .iter()
        .find(|issue| issue.code == "V-PARENT")
        .expect("V-PARENT must be reported");

    // Located on the offending child, not the parent.
    assert!(
        matches!(conflict.target, ElementRef::Task { id } if id == TaskId(2)),
        "unexpected target: {:?}",
        conflict.target
    );
    // The message names both windows so the user can see the conflict.
    assert!(
        conflict.message.contains("2026-09-20"),
        "{}",
        conflict.message
    );
    assert!(
        conflict.message.contains("2026-09-05"),
        "{}",
        conflict.message
    );
    assert!(!conflict.fix_hint.is_empty());
}

#[test]
fn scenario_4_dangling_reference_is_located() {
    let source =
        "%mcm 1\n- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2026-09-03..2026-09-04] <-t99\n";
    let (_, issues) = generate(source);

    let missing = issues
        .iter()
        .find(|issue| issue.code == "V-REF")
        .expect("V-REF must be reported");

    assert_eq!(missing.severity, Severity::Error);
    assert!(missing.message.contains("t99"), "{}", missing.message);
    assert!(matches!(missing.target, ElementRef::Dependency { .. }));
    assert!(!missing.fix_hint.is_empty());
}

#[test]
fn every_reported_issue_is_locatable_and_actionable() {
    // spec FR-004: each issue locates an element, explains why, and suggests a fix.
    let broken = "%mcm 1
- 甲 #t1 [2026-09-10..2026-09-01] <-t99
  - 子 #t2 [2020-01-01..2020-01-02]
! 里程碑 #m1 [2019-01-01] <-t2
";
    let (_, issues) = generate(broken);
    assert!(!issues.is_empty());
    for issue in &issues {
        assert!(!issue.code.is_empty(), "{issue:?}");
        assert!(!issue.message.is_empty(), "{issue:?}");
        assert!(!issue.fix_hint.is_empty(), "{issue:?}");
        assert!(
            !matches!(issue.target, ElementRef::Plan),
            "issue should point at a concrete element: {issue:?}"
        );
    }
}

#[test]
fn generation_is_reproducible() {
    // 同一描述必得同一规划（宪法 IV / FR-001）。
    let baseline = generate(HEALTHY);
    for _ in 0..50 {
        assert_eq!(generate(HEALTHY), baseline);
    }
}

#[test]
fn errors_never_block_the_rest_of_the_plan_from_loading() {
    // Recovery: a malformed line must not cost us the healthy tasks.
    let source = "%mcm 1\n- 正常 #t1 [2026-09-01..2026-09-02]\n这一行是乱码\n- 也正常 #t2 [2026-09-03..2026-09-04] <-t1\n";
    let (plan, issues) = generate(source);
    assert_eq!(plan.tasks.len(), 2);
    assert_eq!(plan.recovered_lines.len(), 1);
    assert!(issues.iter().any(|i| i.code.starts_with("P-")));
}
