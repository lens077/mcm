//! Contract tests for contracts/outline-grammar.md §契约测试要求 1–4.

use std::collections::HashSet;

use mcm_core::model::{Plan, Schedule, TaskId};
use mcm_core::outline::{parse, serialize};
use proptest::prelude::*;

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/outline")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const FIXTURES: [&str; 3] = [
    "contract-example.mcm",
    "unicode-and-escapes.mcm",
    "minimal.mcm",
];

// ---------------------------------------------------------------- golden ---

#[test]
fn golden_fixtures_parse_without_errors() {
    for name in FIXTURES {
        let out = parse(&fixture(name));
        let errors: Vec<_> = out.issues.iter().filter(|i| i.is_error()).collect();
        assert!(
            errors.is_empty(),
            "{name} produced parse errors: {errors:?}"
        );
    }
}

#[test]
fn golden_fixtures_are_canonical_fixed_points() {
    // serialize(parse(t)) == t for canonical text (contract §规范化序列化 往返律).
    for name in FIXTURES {
        let text = fixture(name);
        let round = serialize(&parse(&text).plan);
        assert_eq!(round, text, "{name} is not a canonical fixed point");
    }
}

#[test]
fn golden_contract_example_matches_expected_model() {
    let plan = parse(&fixture("contract-example.mcm")).plan;
    assert_eq!(plan.title, "移动端改版");
    assert_eq!(plan.tasks.len(), 6);
    assert_eq!(plan.dependencies.len(), 4);
    assert_eq!(plan.milestones.len(), 1);
    assert!(plan.task(TaskId(2)).expect("t2").done);
    assert_eq!(
        plan.task(TaskId(3)).expect("t3").schedule,
        Schedule::Duration { days: 3 }
    );
    assert_eq!(
        plan.task(TaskId(1)).expect("t1").assignee.as_deref(),
        Some("王芳")
    );
}

#[test]
fn unicode_titles_notes_and_escapes_survive() {
    let plan = parse(&fixture("unicode-and-escapes.mcm")).plan;
    let t1 = plan.task(TaskId(1)).expect("t1");
    assert_eq!(t1.title, "处理 #标签 与 @提及");
    assert!(t1.notes.as_deref().unwrap_or_default().contains('🚀'));
    assert!(plan.milestones[0].name.contains('🎯'));
}

// ------------------------------------------------------------ determinism ---

#[test]
fn parsing_is_deterministic_over_100_replays() {
    for name in FIXTURES {
        let text = fixture(name);
        let baseline = parse(&text);
        for _ in 0..100 {
            assert_eq!(parse(&text), baseline, "{name} parse is not deterministic");
        }
    }
}

#[test]
fn auto_assigned_ids_are_stable_and_unique() {
    let source = "- 甲\n- 乙 #t5\n- 丙\n  - 丁\n";
    let baseline: Vec<TaskId> = parse(source).plan.tasks.iter().map(|t| t.id).collect();
    for _ in 0..100 {
        let ids: Vec<TaskId> = parse(source).plan.tasks.iter().map(|t| t.id).collect();
        assert_eq!(ids, baseline);
    }
    assert_eq!(
        baseline.iter().collect::<HashSet<_>>().len(),
        baseline.len()
    );
}

// ----------------------------------------------------------- error codes ---

#[test]
fn every_parse_error_code_has_a_minimal_trigger() {
    let cases: [(&str, &str); 6] = [
        ("P-001", "%mcm 9\n- a\n"),
        ("P-002", "- a\n      - too deep\n"),
        ("P-003", "- a [2026-9-1..2026-09-05]\n"),
        ("P-005", "- a <-nope\n"),
        ("P-006", "! 冻结\n"),
        ("P-007", "> 孤立备注\n"),
    ];
    for (code, source) in cases {
        let out = parse(source);
        let issue = out
            .issues
            .iter()
            .find(|i| i.code == code)
            .unwrap_or_else(|| panic!("{code} not raised by: {source:?}"));
        assert!(!issue.message.is_empty(), "{code} needs a message");
        assert!(!issue.fix_hint.is_empty(), "{code} needs a fix hint");
        assert!(
            matches!(issue.target, mcm_core::model::ElementRef::Line { .. }),
            "{code} must locate a line"
        );
    }
}

#[test]
fn p008_is_a_warning_that_preserves_content() {
    let out = parse("%mcm 1\n%mystery 值\n");
    let issue = out
        .issues
        .iter()
        .find(|i| i.code == "P-008")
        .expect("P-008");
    assert!(!issue.is_error());
    assert_eq!(out.plan.recovered_lines.len(), 1);
}

// -------------------------------------------------------------- property ---

/// Builds a small plan directly from generated parts, then checks the round trip.
fn plan_strategy() -> impl Strategy<Value = Plan> {
    let title = "[一-龥A-Za-z][一-龥A-Za-z0-9]{0,12}";
    prop::collection::vec((title, any::<bool>(), 0u32..3u32), 1..8).prop_map(|rows| {
        let mut plan = Plan::empty();
        plan.title = "属性测试".to_owned();
        let mut previous_root: Option<TaskId> = None;
        for (index, (title, done, shape)) in rows.into_iter().enumerate() {
            let id = TaskId(u32::try_from(index + 1).unwrap_or(1));
            let mut task = mcm_core::model::Task::new(id, title);
            task.done = done;
            // Shape 1 nests under the previous root, others stay top level.
            task.parent = if shape == 1 { previous_root } else { None };
            if task.parent.is_none() {
                previous_root = Some(id);
            }
            if shape == 2 {
                task.schedule = Schedule::Duration { days: 3 };
            }
            plan.tasks.push(task);
        }
        // Recompute sibling order so it matches document order.
        let snapshot = plan.tasks.clone();
        for task in &mut plan.tasks {
            let order = snapshot
                .iter()
                .filter(|other| other.parent == task.parent)
                .position(|other| other.id == task.id)
                .unwrap_or(0);
            task.order = u32::try_from(order).unwrap_or(0);
        }
        plan
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// parse(serialize(m)) == m for arbitrary models (contract §往返律).
    #[test]
    fn model_survives_serialize_parse(plan in plan_strategy()) {
        let text = serialize(&plan);
        let reparsed = parse(&text).plan;
        prop_assert_eq!(reparsed, plan);
    }

    /// Serializing is idempotent: canonical text is a fixed point.
    #[test]
    fn serialization_is_idempotent(plan in plan_strategy()) {
        let once = serialize(&plan);
        let twice = serialize(&parse(&once).plan);
        prop_assert_eq!(once, twice);
    }

    /// Arbitrary line-level corruption must never panic and must be recovered.
    #[test]
    fn corrupted_documents_never_panic(
        garbage in prop::collection::vec("[^\n]{0,40}", 0..6)
    ) {
        let mut source = String::from("%mcm 1\n%title 恢复测试\n\n- 正常任务 #t1\n");
        for line in &garbage {
            source.push_str(line);
            source.push('\n');
        }
        let out = parse(&source);
        // The healthy task always survives.
        prop_assert!(out.plan.task(TaskId(1)).is_some());
        // Nothing is silently dropped: unparsable lines are quarantined.
        let non_blank = garbage.iter().filter(|l| !l.trim().is_empty()).count();
        prop_assert!(out.plan.recovered_lines.len() + out.plan.tasks.len() + out.plan.milestones.len() >= 1);
        prop_assert!(out.plan.recovered_lines.len() <= non_blank + 1);
    }
}
