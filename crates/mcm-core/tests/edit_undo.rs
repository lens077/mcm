//! Editing contract tests (tasks.md T041):
//! - apply∘undo is the identity for arbitrary command sequences
//! - undo crosses a `ReplaceFromOutline` boundary in one step
//! - `scene_stale` is correct for every command kind

use mcm_core::edit::{EditCommand, stale_views};
use mcm_core::model::{MilestoneId, Schedule, TaskId};
use mcm_core::outline::serialize;
use mcm_core::scene::ViewKind;
use mcm_core::session::Session;
use proptest::prelude::*;

const SAMPLE: &str = "%mcm 1
%title 编辑测试
%start 2026-09-01

- 甲 #t1 [2026-09-01..2026-09-10]
  - 甲一 #t2 [2026-09-01..2026-09-03]
  - 甲二 #t3 [2026-09-04..2026-09-06]
- 乙 #t4 [2026-09-11..2026-09-12] <-t2
! 冻结 #m1 [2026-09-20] <-t4
";

fn session() -> Session {
    let mut session = Session::new();
    session.apply_outline_text(SAMPLE);
    session
}

/// Commands that are always valid against SAMPLE.
fn command_pool() -> Vec<EditCommand> {
    vec![
        EditCommand::RenameTask {
            id: TaskId(1),
            title: "改名甲".into(),
        },
        EditCommand::RenameTask {
            id: TaskId(4),
            title: "改名乙".into(),
        },
        EditCommand::SetDone {
            id: TaskId(2),
            done: true,
        },
        EditCommand::SetAssignee {
            id: TaskId(3),
            assignee: Some("王芳".into()),
        },
        EditCommand::SetNotes {
            id: TaskId(3),
            notes: Some("一些备注".into()),
        },
        EditCommand::SetSchedule {
            id: TaskId(3),
            schedule: Schedule::Duration { days: 4 },
        },
        EditCommand::AddDependency {
            predecessor: TaskId(3),
            successor: TaskId(4),
        },
        EditCommand::RemoveDependency {
            predecessor: TaskId(2),
            successor: TaskId(4),
        },
        EditCommand::AddTask {
            parent: Some(TaskId(1)),
            index: 0,
            title: "新子任务".into(),
            id: None,
        },
        EditCommand::AddTask {
            parent: None,
            index: 1,
            title: "新顶层".into(),
            id: None,
        },
        EditCommand::MoveTask {
            id: TaskId(3),
            new_parent: None,
            index: 0,
        },
        EditCommand::DeleteTask { id: TaskId(3) },
        EditCommand::AddMilestone {
            name: "新里程碑".into(),
            date: "2026-10-01".into(),
            linked_tasks: vec![TaskId(4)],
            id: None,
        },
        EditCommand::UpdateMilestone {
            id: MilestoneId(1),
            name: "改名里程碑".into(),
            date: "2026-09-25".into(),
            linked_tasks: vec![],
        },
        EditCommand::RemoveMilestone { id: MilestoneId(1) },
        EditCommand::SetPlanMeta {
            title: "新标题".into(),
            description: Some("说明".into()),
            project_start: Some("2026-08-01".into()),
        },
    ]
}

#[test]
fn every_command_kind_round_trips_through_undo() {
    for command in command_pool() {
        let mut session = session();
        let before = session.outline_text();

        session
            .edit(&command)
            .unwrap_or_else(|e| panic!("{command:?} failed: {e}"));
        assert_ne!(
            session.outline_text(),
            before,
            "{command:?} had no visible effect"
        );

        session
            .undo()
            .unwrap_or_else(|| panic!("{command:?} produced no undo entry"));
        assert_eq!(
            session.outline_text(),
            before,
            "{command:?} did not undo cleanly"
        );
    }
}

#[test]
fn every_command_kind_redoes_precisely() {
    for command in command_pool() {
        let mut session = session();
        session.edit(&command).expect("apply");
        let after_edit = session.outline_text();

        session.undo().expect("undo");
        session.redo().expect("redo");
        assert_eq!(
            session.outline_text(),
            after_edit,
            "{command:?} did not redo cleanly"
        );
    }
}

#[test]
fn undo_crosses_the_outline_boundary_as_one_step() {
    let mut session = session();
    let original = session.outline_text();

    session.apply_outline_text("%mcm 1\n%title 完全不同\n\n- 只有一个任务 #t1\n");
    assert_eq!(session.plan().tasks.len(), 1);

    // A single undo must restore the entire previous document.
    session.undo().expect("undo the reparse");
    assert_eq!(session.outline_text(), original);
}

#[test]
fn stale_views_are_correct_per_command_kind() {
    // Structural and scheduling changes touch every projection.
    for command in [
        EditCommand::AddTask {
            parent: None,
            index: 0,
            title: "x".into(),
            id: None,
        },
        EditCommand::DeleteTask { id: TaskId(3) },
        EditCommand::MoveTask {
            id: TaskId(3),
            new_parent: None,
            index: 0,
        },
        EditCommand::SetSchedule {
            id: TaskId(3),
            schedule: Schedule::Duration { days: 2 },
        },
        EditCommand::ReplaceFromOutline {
            text: SAMPLE.into(),
        },
    ] {
        assert_eq!(
            stale_views(&command).len(),
            4,
            "{command:?} should invalidate all views"
        );
    }

    // Dependencies never affect the milestone band.
    let dependency = stale_views(&EditCommand::AddDependency {
        predecessor: TaskId(1),
        successor: TaskId(4),
    });
    assert!(dependency.contains(&ViewKind::DepGraph));
    assert!(!dependency.contains(&ViewKind::Milestones));

    // Milestones never affect the dependency graph.
    let milestone = stale_views(&EditCommand::RemoveMilestone { id: MilestoneId(1) });
    assert!(milestone.contains(&ViewKind::Milestones));
    assert!(milestone.contains(&ViewKind::Timeline));
    assert!(!milestone.contains(&ViewKind::DepGraph));
}

#[test]
fn deleting_a_parent_restores_the_whole_subtree_with_references() {
    let mut session = session();
    let before = session.outline_text();

    session
        .edit(&EditCommand::DeleteTask { id: TaskId(1) })
        .expect("delete");
    // Subtree and the dependency that pointed into it are gone.
    assert!(session.plan().task(TaskId(2)).is_none());
    assert!(session.plan().dependencies.is_empty());

    session.undo().expect("undo");
    assert_eq!(
        session.outline_text(),
        before,
        "subtree, deps and links must all return"
    );
}

#[test]
fn deleting_a_sibling_preserves_the_order_of_the_others() {
    // Regression: adding a task renumbers siblings, so undoing a later delete
    // used to resurrect the subtree in the wrong order.
    let mut session = session();
    let before = session.outline_text();

    session
        .edit(&EditCommand::AddTask {
            parent: Some(TaskId(1)),
            index: 0,
            title: "插入".into(),
            id: None,
        })
        .expect("add");
    session
        .edit(&EditCommand::DeleteTask { id: TaskId(2) })
        .expect("delete t2");
    session
        .edit(&EditCommand::DeleteTask { id: TaskId(3) })
        .expect("delete t3");

    session.undo().expect("undo delete t3");
    session.undo().expect("undo delete t2");
    session.undo().expect("undo add");

    assert_eq!(
        session.outline_text(),
        before,
        "sibling order must be restored exactly"
    );
}

#[test]
fn failed_commands_leave_no_trace() {
    let mut session = session();
    let before = session.outline_text();
    let depth_before = session.undo_depth();

    let result = session.edit(&EditCommand::RenameTask {
        id: TaskId(999),
        title: "x".into(),
    });
    assert!(result.is_err());
    assert_eq!(
        session.outline_text(),
        before,
        "a failed command must not mutate the plan"
    );
    assert_eq!(
        session.undo_depth(),
        depth_before,
        "a failed command must not enter the journal"
    );
}

#[test]
fn issues_are_recomputed_after_every_edit_and_undo() {
    let mut session = session();
    assert_eq!(session.error_count(), 0);

    // Introduce a cycle: t4 already depends on t2, so t2 <- t4 closes it.
    session
        .edit(&EditCommand::AddDependency {
            predecessor: TaskId(4),
            successor: TaskId(2),
        })
        .expect("edit");
    assert!(session.issues().iter().any(|issue| issue.code == "V-CYCLE"));

    session.undo().expect("undo");
    assert_eq!(session.error_count(), 0, "undo must clear the issue");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(60))]

    /// Applying N commands then undoing N times restores the original document.
    #[test]
    fn random_command_sequences_undo_to_the_start(
        indices in prop::collection::vec(0usize..16, 1..8)
    ) {
        let pool = command_pool();
        let mut session = session();
        let baseline = session.outline_text();

        let mut applied = 0usize;
        for index in indices {
            let Some(command) = pool.get(index) else { continue };
            // Some sequences invalidate later commands (e.g. delete then rename);
            // only count the ones that actually applied.
            if session.edit(command).is_ok() {
                applied += 1;
            }
        }
        for _ in 0..applied {
            prop_assert!(session.undo().is_some(), "undo stack ran dry too early");
        }
        prop_assert_eq!(session.outline_text(), baseline);
    }

    /// Undo/redo cycling is stable: any number of round trips lands in the
    /// same place.
    #[test]
    fn undo_redo_cycles_are_stable(cycles in 1usize..6) {
        let mut session = session();
        session
            .edit(&EditCommand::RenameTask { id: TaskId(1), title: "改名".into() })
            .expect("edit");
        let after_edit = session.outline_text();

        for _ in 0..cycles {
            session.undo().expect("undo");
            session.redo().expect("redo");
        }
        prop_assert_eq!(session.outline_text(), after_edit);
    }
}

#[test]
fn serialize_stays_canonical_across_edits() {
    // Editing must never produce text that re-serializes differently.
    let mut session = session();
    for command in command_pool() {
        if session.edit(&command).is_err() {
            continue;
        }
        let text = session.outline_text();
        let reparsed = mcm_core::outline::parse(&text).plan;
        assert_eq!(
            serialize(&reparsed),
            text,
            "canonical form broke after {command:?}"
        );
    }
}
