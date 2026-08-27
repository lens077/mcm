//! File contract tests (contracts/plan-file-format.md §契约测试 1–5, spec SC-006).

use std::path::PathBuf;

use mcm_core::edit::EditCommand;
use mcm_core::model::{Schedule, TaskId};
use mcm_core::session::Session;
use proptest::prelude::*;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("mcm-contract-{tag}-{stamp}"));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        Self(dir)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const RICH: &str = "%mcm 1
%title 契约测试
%start 2026-09-01

# 阶段说明
- 需求 #t1 [2026-09-01..2026-09-10] @王芳
  > 第一行备注
  > 第二行备注
  - 访谈 #t2 [2026-09-01..2026-09-03]
  - 分析 #t3 [2d] <-t2
- 设计 #t4 [2026-09-11..2026-09-20] <-t3
! 冻结 #m1 [2026-09-30] <-t4
";

fn loaded() -> Session {
    let mut session = Session::new();
    session.apply_outline_text(RICH);
    session
}

// ------------------------------------------------------- 1. round trip ---

#[test]
fn save_open_round_trip_is_lossless() {
    let scratch = Scratch::new("roundtrip");
    let path = scratch.file("plan.mcm");

    let mut session = loaded();
    let before = session.outline_text();
    session.save(Some(&path)).expect("save");

    let mut reopened = Session::new();
    reopened.open(&path).expect("open");
    assert_eq!(reopened.outline_text(), before);
    assert_eq!(reopened.plan(), session.plan());
}

#[test]
fn comments_notes_and_assignees_survive_the_file() {
    let scratch = Scratch::new("fidelity");
    let path = scratch.file("plan.mcm");

    let mut session = loaded();
    session.save(Some(&path)).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");

    assert!(text.contains("# 阶段说明"), "comments must persist: {text}");
    assert!(text.contains("> 第一行备注"), "notes must persist");
    assert!(
        text.contains("> 第二行备注"),
        "multi-line notes must persist"
    );
    assert!(text.contains("@王芳"), "assignees must persist");
    assert!(text.contains("! 冻结"), "milestones must persist");
}

#[test]
fn a_hundred_random_edits_still_round_trip() {
    // spec SC-006: 100 consecutive edits, still lossless.
    let scratch = Scratch::new("hundred");
    let path = scratch.file("plan.mcm");
    let mut session = loaded();

    for index in 0..100u32 {
        let command = match index % 5 {
            0 => EditCommand::RenameTask {
                id: TaskId(2),
                title: format!("访谈 {index}"),
            },
            1 => EditCommand::SetDone {
                id: TaskId(3),
                done: index % 10 == 1,
            },
            2 => EditCommand::SetAssignee {
                id: TaskId(4),
                assignee: Some(format!("负责人{index}")),
            },
            3 => EditCommand::SetNotes {
                id: TaskId(1),
                notes: Some(format!("备注 {index}")),
            },
            _ => EditCommand::SetSchedule {
                id: TaskId(3),
                schedule: Schedule::Duration {
                    days: (index % 5) + 1,
                },
            },
        };
        session.edit(&command).expect("edit applies");
    }

    let before = session.outline_text();
    session.save(Some(&path)).expect("save");
    let mut reopened = Session::new();
    reopened.open(&path).expect("open");
    assert_eq!(reopened.outline_text(), before);
}

// ---------------------------------------------------------- 2. atomic ---

#[test]
fn interrupted_save_leaves_the_previous_file_intact() {
    let scratch = Scratch::new("atomic");
    let path = scratch.file("plan.mcm");
    let mut session = loaded();
    session.save(Some(&path)).expect("first save");
    let original = std::fs::read_to_string(&path).expect("read");

    // Renaming onto a directory fails at the last step, standing in for a
    // crash between write and rename.
    let blocked = scratch.file("blocked.mcm");
    std::fs::create_dir_all(&blocked).expect("dir");
    assert!(session.save(Some(&blocked)).is_err());

    assert_eq!(std::fs::read_to_string(&path).expect("read"), original);
}

#[test]
fn saving_never_leaves_temp_files() {
    let scratch = Scratch::new("temps");
    let path = scratch.file("plan.mcm");
    let mut session = loaded();
    for _ in 0..5 {
        session.save(Some(&path)).expect("save");
    }
    let temps: Vec<_> = std::fs::read_dir(&scratch.0)
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
        .collect();
    assert!(temps.is_empty(), "left temp files: {temps:?}");
}

// -------------------------------------------------------- 3. recovery ---

#[test]
fn damaged_files_open_with_a_quarantine_list() {
    let scratch = Scratch::new("recover");
    let path = scratch.file("damaged.mcm");
    std::fs::write(
        &path,
        "%mcm 1\n- 正常甲 #t1\n@@@ 完全不合法 @@@\n- 正常乙 #t2\n???\n",
    )
    .expect("write");

    let mut session = Session::new();
    session.open(&path).expect("damaged files must still open");
    assert_eq!(session.plan().tasks.len(), 2, "healthy tasks survive");
    assert_eq!(
        session.recovery_report().count(),
        2,
        "both bad lines quarantined"
    );

    // Re-saving preserves the quarantined text as a marked comment.
    session.save(Some(&path)).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("[mcm:recovered]"));
    assert!(text.contains("完全不合法"));
}

#[test]
fn binary_files_are_refused_rather_than_recovered() {
    let scratch = Scratch::new("binary");
    let path = scratch.file("image.mcm");
    std::fs::write(&path, [0x89, b'P', b'N', b'G', 0x00, 0x1a]).expect("write");

    let mut session = Session::new();
    let error = session.open(&path).expect_err("binary must be refused");
    assert_eq!(error.code(), "E_FILE_IO");
}

// --------------------------------------------------------- 4. version ---

#[test]
fn newer_major_versions_are_refused_with_a_clear_code() {
    let scratch = Scratch::new("version");
    let path = scratch.file("future.mcm");
    std::fs::write(&path, "%mcm 2\n- 甲 #t1\n").expect("write");

    let mut session = Session::new();
    let error = session.open(&path).expect_err("must refuse");
    assert_eq!(error.code(), "E_VERSION_TOO_NEW");
    assert!(error.to_string().contains('2'), "message names the version");
}

#[test]
fn files_without_a_version_header_open_and_gain_one_on_save() {
    let scratch = Scratch::new("noheader");
    let path = scratch.file("legacy.mcm");
    std::fs::write(&path, "- 甲 #t1\n").expect("write");

    let mut session = Session::new();
    session.open(&path).expect("open");
    assert!(session.issues().iter().any(|issue| issue.code == "P-001"));

    session.save(Some(&path)).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        text.starts_with("%mcm 1\n"),
        "the header is written back: {text}"
    );
}

// ----------------------------------------------- 5. hand-edited input ---

#[test]
fn hand_edited_variants_normalise_on_save() {
    let scratch = Scratch::new("handedit");
    let path = scratch.file("hand.mcm");
    // CRLF, a BOM, and annotations in a non-canonical order.
    std::fs::write(
        &path,
        "\u{feff}%mcm 1\r\n- 手写任务 <-t2 @人 [2d] #t1\r\n- 前置 #t2\r\n",
    )
    .expect("write");

    let mut session = Session::new();
    session.open(&path).expect("open");
    session.save(Some(&path)).expect("save");

    let text = std::fs::read_to_string(&path).expect("read");
    assert!(!text.contains('\r'), "CRLF is normalised away");
    assert!(!text.starts_with('\u{feff}'), "the BOM is stripped");

    // Canonical annotation order: id, schedule, assignee, predecessors.
    let line = text
        .lines()
        .find(|line| line.contains("#t1"))
        .expect("task line");
    let id_at = line.find("#t1").expect("id");
    let schedule_at = line.find("[2d]").expect("schedule");
    let owner_at = line.find("@人").expect("assignee");
    let pred_at = line.find("<-t2").expect("predecessor");
    assert!(
        id_at < schedule_at && schedule_at < owner_at && owner_at < pred_at,
        "{line}"
    );
}

#[test]
fn a_manually_edited_file_reloads_with_the_change_applied() {
    let scratch = Scratch::new("manual");
    let path = scratch.file("plan.mcm");
    let mut session = loaded();
    session.save(Some(&path)).expect("save");

    // Simulate the user editing the file in a text editor.
    let text = std::fs::read_to_string(&path).expect("read");
    let edited = text.replace("需求 #t1", "需求（已改） #t1");
    std::fs::write(&path, edited).expect("write");

    let mut reopened = Session::new();
    reopened.open(&path).expect("open");
    assert_eq!(
        reopened.plan().task(TaskId(1)).expect("t1").title,
        "需求（已改）"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    /// Any sequence of title edits still survives a save/open cycle.
    #[test]
    fn arbitrary_titles_survive_the_file(
        titles in prop::collection::vec("[一-龥A-Za-z0-9 ]{1,20}", 1..6)
    ) {
        let scratch = Scratch::new("prop");
        let path = scratch.file("plan.mcm");
        let mut session = loaded();

        for title in &titles {
            let trimmed = title.trim();
            if trimmed.is_empty() {
                continue;
            }
            session
                .edit(&EditCommand::RenameTask { id: TaskId(1), title: trimmed.to_owned() })
                .expect("rename");
        }

        let before = session.outline_text();
        session.save(Some(&path)).expect("save");
        let mut reopened = Session::new();
        reopened.open(&path).expect("open");
        prop_assert_eq!(reopened.outline_text(), before);
    }
}
