use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::edit::{EditCommand, EditError, Journal, JournalEntry, apply, stale_views};
use crate::model::{IdAllocator, Plan, TaskId, ValidationIssue};
use crate::scene::ViewKind;

/// Errors surfaced to the IPC layer as `E_*` envelopes
/// (contracts/ipc-commands.md §错误码).
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("需要先选择保存路径")]
    NeedPath,
    #[error("文件读写失败：{0}")]
    FileIo(String),
    #[error("文件格式版本 {found} 高于本应用支持的 {supported}，请更新应用后再打开")]
    VersionTooNew { found: u32, supported: u32 },
    #[error("找不到目标元素：{0}")]
    BadTarget(String),
}

impl SessionError {
    /// Stable machine-readable code used by the front end to branch.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            SessionError::NeedPath => "E_NEED_PATH",
            SessionError::FileIo(_) => "E_FILE_IO",
            SessionError::VersionTooNew { .. } => "E_VERSION_TOO_NEW",
            SessionError::BadTarget(_) => "E_BAD_TARGET",
        }
    }
}

/// Snapshot of session status returned by `session_new`/`session_state`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub dirty: bool,
    pub title: String,
    pub revision: u64,
    pub counts: PlanCounts,
    pub issues: Vec<ValidationIssue>,
    pub undo_depth: usize,
    pub redo_depth: usize,
}

/// Reads the `%mcm <n>` header, if the document declares one.
fn declared_version(text: &str) -> Option<u32> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let rest = trimmed.strip_prefix("%mcm")?;
        return rest.trim().parse().ok();
    }
    None
}

/// Temp file + fsync + rename, so the target is either the old file or the new
/// one — never a partial write.
fn write_atomically(target: &Path, contents: &str) -> Result<(), SessionError> {
    use std::io::Write as _;

    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| {
        SessionError::FileIo(format!("无法创建目录 {}：{error}", parent.display()))
    })?;

    // Same directory guarantees the rename stays on one filesystem.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("plan.mcm");
    let temp = parent.join(format!(".{file_name}.tmp-{stamp}"));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temp);
        return Err(SessionError::FileIo(format!("写入临时文件失败：{error}")));
    }

    if let Err(error) = std::fs::rename(&temp, target) {
        let _ = std::fs::remove_file(&temp);
        return Err(SessionError::FileIo(format!(
            "无法保存到 {}：{error}",
            target.display()
        )));
    }
    Ok(())
}

/// One search hit (contracts/ipc-commands.md `search`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMatch {
    #[serde(rename = "ref")]
    pub target: crate::model::ElementRef,
    pub title: String,
    pub snippet: String,
}

/// Extracts a short context window around the first match.
fn snippet_around(text: &str, needle: &str) -> String {
    let lower = text.to_lowercase();
    let Some(byte_index) = lower.find(needle) else {
        return text.chars().take(60).collect();
    };
    // Work in chars so multi-byte text is never split mid-character.
    let char_index = lower[..byte_index].chars().count();
    let start = char_index.saturating_sub(12);
    let snippet: String = text.chars().skip(start).take(60).collect();
    if start > 0 {
        format!("…{snippet}")
    } else {
        snippet
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanCounts {
    pub tasks: usize,
    pub dependencies: usize,
    pub milestones: usize,
}

/// Holds the authoritative plan plus revision/dirty bookkeeping.
///
/// Parsing, validation, editing and file IO are layered on top of this type in
/// later phases; the container itself only guarantees that every mutation bumps
/// the revision and marks the document dirty.
#[derive(Debug)]
pub struct Session {
    plan: Plan,
    path: Option<PathBuf>,
    dirty: bool,
    revision: u64,
    ids: IdAllocator,
    issues: Vec<ValidationIssue>,
    journal: Journal,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    #[must_use]
    pub fn new() -> Self {
        Self {
            plan: Plan::empty(),
            path: None,
            dirty: false,
            revision: 0,
            ids: IdAllocator::new(),
            issues: Vec::new(),
            journal: Journal::new(),
        }
    }

    /// Rebuilds a session around an already-parsed plan (used by open/apply).
    #[must_use]
    pub fn from_plan(plan: Plan) -> Self {
        let mut ids = IdAllocator::new();
        for task in &plan.tasks {
            ids.observe_task(task.id);
        }
        for milestone in &plan.milestones {
            ids.observe_milestone(milestone.id);
        }
        Self {
            plan,
            path: None,
            dirty: false,
            revision: 0,
            ids,
            issues: Vec::new(),
            journal: Journal::new(),
        }
    }

    #[must_use]
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Canonical outline text for the current model.
    #[must_use]
    pub fn outline_text(&self) -> String {
        crate::outline::serialize(&self.plan)
    }

    /// Parses `text`, revalidates and replaces the model. Parse issues (`P-*`)
    /// and semantic issues (`V-*`) are merged into one ordered list.
    pub fn apply_outline_text(&mut self, text: &str) {
        // Record the previous text so the whole reparse is one undo step
        // (data-model.md: ReplaceFromOutline is a single undo boundary).
        let previous = crate::outline::serialize(&self.plan);
        let parsed = crate::outline::parse(text);
        let mut issues = parsed.issues;
        issues.extend(crate::validate::validate(&parsed.plan));
        self.replace_plan(parsed.plan, true);
        self.issues = issues;
        self.journal.record(
            EditCommand::ReplaceFromOutline {
                text: text.to_owned(),
            },
            EditCommand::ReplaceFromOutline { text: previous },
        );
    }

    /// Recomputes validation for the current model without reparsing.
    pub fn revalidate(&mut self) {
        self.issues = crate::validate::validate(&self.plan);
    }

    /// Applies one edit command, revalidates, and records the inverse so the
    /// change can be undone precisely (spec FR-010/FR-012).
    ///
    /// Returns the views whose scene graph is now stale.
    pub fn edit(&mut self, command: &EditCommand) -> Result<Vec<ViewKind>, EditError> {
        let inverse = self.apply_command(command)?;
        self.journal.record(command.clone(), inverse);
        Ok(stale_views(command))
    }

    /// Undoes the most recent command. Returns `None` when the stack is empty.
    pub fn undo(&mut self) -> Option<Vec<ViewKind>> {
        let entry = self.journal.take_undo()?;
        // Applying the inverse yields the command that redoes the change.
        let redo_inverse = self.apply_command(&entry.inverse).ok()?;
        let stale = stale_views(&entry.inverse);
        self.journal.finish_undo(JournalEntry {
            applied: entry.inverse,
            inverse: redo_inverse,
        });
        Some(stale)
    }

    /// Redoes the most recently undone command.
    pub fn redo(&mut self) -> Option<Vec<ViewKind>> {
        let entry = self.journal.take_redo()?;
        let undo_inverse = self.apply_command(&entry.inverse).ok()?;
        let stale = stale_views(&entry.inverse);
        self.journal.finish_redo(JournalEntry {
            applied: entry.inverse,
            inverse: undo_inverse,
        });
        Some(stale)
    }

    /// Shared mutation path: apply, bump revision, mark dirty, revalidate.
    fn apply_command(&mut self, command: &EditCommand) -> Result<EditCommand, EditError> {
        let mut task_cursor = self.ids.clone();
        let mut milestone_cursor = self.ids.clone();
        let mut next_task = || task_cursor.next_task();
        let mut next_milestone = || milestone_cursor.next_milestone();
        let inverse = apply(&mut self.plan, &mut next_task, &mut next_milestone, command)?;

        // Re-seed the allocator from the resulting document so ids never repeat.
        for task in &self.plan.tasks {
            self.ids.observe_task(task.id);
        }
        for milestone in &self.plan.milestones {
            self.ids.observe_milestone(milestone.id);
        }

        self.revision += 1;
        self.dirty = true;
        self.revalidate();
        Ok(inverse)
    }

    /// Opens a `.mcm` file: refuses binary content and files from a newer major
    /// version, and recovers damaged lines instead of failing
    /// (contracts/plan-file-format.md §版本策略 / §恢复语义).
    pub fn open(&mut self, path: &Path) -> Result<(), SessionError> {
        let bytes = std::fs::read(path).map_err(|error| {
            SessionError::FileIo(format!("无法读取 {}：{error}", path.display()))
        })?;
        if crate::outline::is_binary(&bytes) {
            return Err(SessionError::FileIo(format!(
                "{} 不是文本格式的 .mcm 文件",
                path.display()
            )));
        }
        let raw = String::from_utf8_lossy(&bytes).into_owned();
        let text = crate::outline::normalise_input(&raw);

        if let Some(version) = declared_version(&text) {
            if version > crate::FORMAT_VERSION {
                return Err(SessionError::VersionTooNew {
                    found: version,
                    supported: crate::FORMAT_VERSION,
                });
            }
        }

        let parsed = crate::outline::parse(&text);
        let mut issues = parsed.issues;
        issues.extend(crate::validate::validate(&parsed.plan));

        self.plan = parsed.plan;
        self.ids = IdAllocator::new();
        for task in &self.plan.tasks {
            self.ids.observe_task(task.id);
        }
        for milestone in &self.plan.milestones {
            self.ids.observe_milestone(milestone.id);
        }
        self.issues = issues;
        self.journal.clear();
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        self.revision += 1;
        Ok(())
    }

    /// Saves atomically: write a temp file, fsync, then rename over the target
    /// so a crash can never leave a half-written plan
    /// (contracts/plan-file-format.md §原子保存).
    pub fn save(&mut self, path: Option<&Path>) -> Result<PathBuf, SessionError> {
        let target = match path {
            Some(explicit) => explicit.to_path_buf(),
            None => self.path.clone().ok_or(SessionError::NeedPath)?,
        };
        let text = self.outline_text();
        write_atomically(&target, &text)?;
        self.mark_saved(target.clone());
        Ok(target)
    }

    /// Recovery details for the currently loaded document.
    #[must_use]
    pub fn recovery_report(&self) -> crate::outline::RecoveryReport {
        crate::outline::report_for(&self.plan)
    }

    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.journal.undo_depth()
    }

    #[must_use]
    pub fn redo_depth(&self) -> usize {
        self.journal.redo_depth()
    }

    /// Case-insensitive substring search over titles, notes and assignees,
    /// returned in document order (contracts/ipc-commands.md `search`).
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<SearchMatch> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut matches = Vec::new();
        for task in self.plan.tasks_in_document_order() {
            let haystacks = [
                Some(task.title.as_str()),
                task.notes.as_deref(),
                task.assignee.as_deref(),
            ];
            let hit = haystacks
                .iter()
                .flatten()
                .find(|text| text.to_lowercase().contains(&needle));
            if let Some(text) = hit {
                matches.push(SearchMatch {
                    target: crate::model::ElementRef::Task { id: task.id },
                    title: task.title.clone(),
                    snippet: snippet_around(text, &needle),
                });
            }
        }
        for milestone in &self.plan.milestones {
            if milestone.name.to_lowercase().contains(&needle) {
                matches.push(SearchMatch {
                    target: crate::model::ElementRef::Milestone { id: milestone.id },
                    title: milestone.name.clone(),
                    snippet: snippet_around(&milestone.name, &needle),
                });
            }
        }
        matches
    }

    /// Projects the requested view from the current model and issue set.
    #[must_use]
    pub fn scene(&self, view: crate::scene::ViewKind) -> crate::scene::SceneGraph {
        crate::scene::scene(&self.plan, view, &self.issues)
    }

    #[must_use]
    pub fn ids(&self) -> &IdAllocator {
        &self.ids
    }

    pub fn ids_mut(&mut self) -> &mut IdAllocator {
        &mut self.ids
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    #[must_use]
    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }

    pub fn set_issues(&mut self, issues: Vec<ValidationIssue>) {
        self.issues = issues;
    }

    #[must_use]
    pub fn error_count(&self) -> usize {
        self.issues.iter().filter(|issue| issue.is_error()).count()
    }

    /// Mutates the plan, bumping the revision and marking the session dirty.
    pub fn mutate<R>(&mut self, edit: impl FnOnce(&mut Plan) -> R) -> R {
        let result = edit(&mut self.plan);
        self.revision += 1;
        self.dirty = true;
        result
    }

    /// Replaces the whole plan (outline re-parse / open).
    pub fn replace_plan(&mut self, plan: Plan, dirty: bool) {
        for task in &plan.tasks {
            self.ids.observe_task(task.id);
        }
        for milestone in &plan.milestones {
            self.ids.observe_milestone(milestone.id);
        }
        self.plan = plan;
        self.revision += 1;
        self.dirty = dirty;
    }

    pub fn mark_saved(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.dirty = false;
    }

    pub fn require_task(&self, id: TaskId) -> Result<(), SessionError> {
        if self.plan.has_task(id) {
            Ok(())
        } else {
            Err(SessionError::BadTarget(id.as_token()))
        }
    }

    #[must_use]
    pub fn counts(&self) -> PlanCounts {
        PlanCounts {
            tasks: self.plan.tasks.len(),
            dependencies: self.plan.dependencies.len(),
            milestones: self.plan.milestones.len(),
        }
    }

    #[must_use]
    pub fn state(&self, undo_depth: usize, redo_depth: usize) -> SessionState {
        SessionState {
            path: self.path.as_ref().map(|p| p.display().to_string()),
            dirty: self.dirty,
            title: self.plan.title.clone(),
            revision: self.revision,
            counts: self.counts(),
            issues: self.issues.clone(),
            undo_depth,
            redo_depth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Task, TaskId};

    #[test]
    fn new_session_is_clean_at_revision_zero() {
        let session = Session::new();
        assert!(!session.is_dirty());
        assert_eq!(session.revision(), 0);
        assert!(session.path().is_none());
        assert_eq!(session.counts().tasks, 0);
    }

    #[test]
    fn mutation_bumps_revision_monotonically_and_marks_dirty() {
        let mut session = Session::new();
        session.mutate(|plan| plan.tasks.push(Task::new(TaskId(1), "任务")));
        assert_eq!(session.revision(), 1);
        assert!(session.is_dirty());
        session.mutate(|plan| plan.title = "改名".to_owned());
        assert_eq!(session.revision(), 2);
    }

    #[test]
    fn saving_clears_dirty_and_records_path() {
        let mut session = Session::new();
        session.mutate(|plan| plan.title = "x".to_owned());
        session.mark_saved(PathBuf::from("/tmp/plan.mcm"));
        assert!(!session.is_dirty());
        assert_eq!(session.path(), Some(Path::new("/tmp/plan.mcm")));
        // Saving must not rewind the revision counter.
        assert_eq!(session.revision(), 1);
    }

    #[test]
    fn from_plan_seeds_id_allocator_past_existing_ids() {
        let mut plan = Plan::empty();
        plan.tasks.push(Task::new(TaskId(7), "已有任务"));
        let mut session = Session::from_plan(plan);
        assert_eq!(session.ids_mut().next_task(), TaskId(8));
    }

    #[test]
    fn require_task_reports_bad_target() {
        let session = Session::new();
        let err = session.require_task(TaskId(3)).unwrap_err();
        assert_eq!(err.code(), "E_BAD_TARGET");
    }

    #[test]
    fn apply_outline_text_replaces_model_and_merges_issues() {
        let mut session = Session::new();
        session.apply_outline_text("%mcm 1\n%title 测试\n- 甲 #t1 <-t9\n");
        assert_eq!(session.plan().title, "测试");
        assert_eq!(session.plan().tasks.len(), 1);
        // V-REF comes from validation, proving both passes ran.
        assert!(session.issues().iter().any(|i| i.code == "V-REF"));
        assert!(session.is_dirty());
        assert_eq!(session.revision(), 1);
    }

    #[test]
    fn outline_text_round_trips_through_the_session() {
        let mut session = Session::new();
        let source = "%mcm 1\n%title 往返\n\n- 甲 #t1\n";
        session.apply_outline_text(source);
        let text = session.outline_text();
        let mut second = Session::new();
        second.apply_outline_text(&text);
        assert_eq!(second.plan(), session.plan());
    }

    #[test]
    fn scene_projection_reflects_current_issues() {
        let mut session = Session::new();
        session.apply_outline_text("- 甲 #t1 [2026-09-10..2026-09-01]\n");
        let graph = session.scene(crate::scene::ViewKind::Wbs);
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(
            graph.nodes[0].style_role,
            crate::scene::StyleRole::TaskError
        );
    }

    #[test]
    fn error_count_only_counts_errors() {
        let mut session = Session::new();
        // Undated leaf => W-NODATE warning only.
        session.apply_outline_text("- 甲 #t1\n");
        assert_eq!(session.error_count(), 0);
        assert!(!session.issues().is_empty());
    }

    #[test]
    fn search_finds_titles_notes_and_assignees() {
        let mut session = Session::new();
        session.apply_outline_text(
            "- 需求阶段 #t1 @王芳\n  > 关注竞品动向\n- 设计阶段 #t2\n! 冻结 #m1 [2026-09-10]\n",
        );
        assert_eq!(session.search("需求").len(), 1);
        assert_eq!(session.search("王芳").len(), 1);
        assert_eq!(session.search("竞品").len(), 1);
        assert_eq!(session.search("冻结").len(), 1);
    }

    #[test]
    fn search_is_case_insensitive_and_trims() {
        let mut session = Session::new();
        session.apply_outline_text("- Design Review #t1\n");
        assert_eq!(session.search("  design  ").len(), 1);
        assert_eq!(session.search("REVIEW").len(), 1);
    }

    #[test]
    fn search_returns_results_in_document_order() {
        let mut session = Session::new();
        session.apply_outline_text("- 阶段甲 #t1\n  - 阶段乙 #t2\n- 阶段丙 #t3\n");
        let hits = session.search("阶段");
        assert_eq!(hits.len(), 3);
        assert_eq!(
            hits[0].target,
            crate::model::ElementRef::Task { id: TaskId(1) }
        );
        assert_eq!(
            hits[1].target,
            crate::model::ElementRef::Task { id: TaskId(2) }
        );
        assert_eq!(
            hits[2].target,
            crate::model::ElementRef::Task { id: TaskId(3) }
        );
    }

    #[test]
    fn empty_query_matches_nothing() {
        let mut session = Session::new();
        session.apply_outline_text("- 任务 #t1\n");
        assert!(session.search("").is_empty());
        assert!(session.search("   ").is_empty());
    }

    #[test]
    fn snippets_never_split_multibyte_characters() {
        let mut session = Session::new();
        let long = "很长的中文任务标题".repeat(12);
        session.apply_outline_text(&format!("- {long}关键词 #t1\n"));
        let hits = session.search("关键词");
        assert_eq!(hits.len(), 1);
        // Round-tripping through String proves the snippet is valid UTF-8.
        assert!(hits[0].snippet.contains("关键词"));
    }

    #[test]
    fn edit_applies_revalidates_and_records_undo() {
        let mut session = Session::new();
        session.apply_outline_text("- 甲 #t1 [2026-09-01..2026-09-02]\n");
        let revision_before = session.revision();

        let stale = session
            .edit(&EditCommand::RenameTask {
                id: TaskId(1),
                title: "改名".into(),
            })
            .expect("edit applies");
        assert!(!stale.is_empty());
        assert_eq!(session.plan().task(TaskId(1)).unwrap().title, "改名");
        assert_eq!(session.revision(), revision_before + 1);
        assert_eq!(session.undo_depth(), 2, "outline apply + rename");
        assert!(session.is_dirty());
    }

    #[test]
    fn undo_then_redo_restores_exactly() {
        let mut session = Session::new();
        session.apply_outline_text("- 甲 #t1\n");
        session
            .edit(&EditCommand::RenameTask {
                id: TaskId(1),
                title: "改名".into(),
            })
            .expect("edit");
        let after_edit = session.outline_text();

        session.undo().expect("undo available");
        assert_eq!(session.plan().task(TaskId(1)).unwrap().title, "甲");

        session.redo().expect("redo available");
        assert_eq!(session.outline_text(), after_edit);
    }

    #[test]
    fn undo_crosses_the_outline_boundary_in_one_step() {
        let mut session = Session::new();
        session.apply_outline_text("- 原始 #t1\n");
        let original = session.outline_text();
        session.apply_outline_text("- 全新甲 #t1\n- 全新乙 #t2\n");
        assert_eq!(session.plan().tasks.len(), 2);

        session.undo().expect("undo the whole reparse");
        assert_eq!(session.outline_text(), original);
    }

    #[test]
    fn undo_on_empty_stack_returns_none() {
        let mut session = Session::new();
        assert!(session.undo().is_none());
        assert!(session.redo().is_none());
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let mut session = Session::new();
        session.apply_outline_text("- 甲 #t1\n");
        session
            .edit(&EditCommand::RenameTask {
                id: TaskId(1),
                title: "一".into(),
            })
            .expect("edit");
        session.undo().expect("undo");
        assert_eq!(session.redo_depth(), 1);

        session
            .edit(&EditCommand::RenameTask {
                id: TaskId(1),
                title: "二".into(),
            })
            .expect("edit");
        assert_eq!(session.redo_depth(), 0);
    }

    #[test]
    fn edits_revalidate_immediately() {
        let mut session = Session::new();
        session.apply_outline_text(
            "- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2026-09-03..2026-09-04]\n",
        );
        assert_eq!(session.error_count(), 0);

        // Adding a self dependency must surface V-SELF right away.
        session
            .edit(&EditCommand::AddDependency {
                predecessor: TaskId(1),
                successor: TaskId(1),
            })
            .expect("edit");
        assert!(session.issues().iter().any(|i| i.code == "V-SELF"));

        session.undo().expect("undo");
        assert_eq!(session.error_count(), 0, "undo must clear the issue too");
    }

    #[test]
    fn long_undo_chains_replay_precisely() {
        let mut session = Session::new();
        session.apply_outline_text("- 甲 #t1\n");
        let baseline = session.outline_text();

        for index in 0..25u32 {
            session
                .edit(&EditCommand::RenameTask {
                    id: TaskId(1),
                    title: format!("改名 {index}"),
                })
                .expect("edit");
        }
        for _ in 0..25 {
            session.undo().expect("undo");
        }
        assert_eq!(session.outline_text(), baseline);
    }

    #[test]
    fn bad_edit_targets_are_reported() {
        let mut session = Session::new();
        session.apply_outline_text("- 甲 #t1\n");
        let result = session.edit(&EditCommand::RenameTask {
            id: TaskId(99),
            title: "x".into(),
        });
        assert!(result.is_err());
        // A failed command must not enter the journal.
        assert_eq!(
            session.undo_depth(),
            1,
            "only the outline apply is recorded"
        );
    }

    /// Unique scratch directory per test, cleaned up on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!("mcm-{tag}-{stamp}"));
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

    #[test]
    fn save_then_open_round_trips_losslessly() {
        let scratch = Scratch::new("roundtrip");
        let path = scratch.file("plan.mcm");

        let mut session = Session::new();
        session.apply_outline_text(
            "%mcm 1\n%title 往返\n%start 2026-09-01\n\n# 注释\n- 甲 #t1 [2026-09-01..2026-09-02] @王芳\n  > 备注\n  - 乙 #t2 [1d] <-t1\n! 冻结 #m1 [2026-09-30] <-t2\n",
        );
        let before = session.outline_text();
        session.save(Some(&path)).expect("save");
        assert!(!session.is_dirty(), "saving clears the dirty flag");

        let mut reopened = Session::new();
        reopened.open(&path).expect("open");
        assert_eq!(reopened.outline_text(), before);
        assert_eq!(reopened.plan(), session.plan());
        assert!(!reopened.is_dirty());
    }

    #[test]
    fn saving_without_a_path_reports_need_path() {
        let mut session = Session::new();
        let error = session.save(None).expect_err("must require a path");
        assert_eq!(error.code(), "E_NEED_PATH");
    }

    #[test]
    fn saving_remembers_the_path_for_next_time() {
        let scratch = Scratch::new("remember");
        let path = scratch.file("plan.mcm");
        let mut session = Session::new();
        session.apply_outline_text("- 甲 #t1\n");
        session.save(Some(&path)).expect("first save");

        session
            .edit(&EditCommand::RenameTask {
                id: TaskId(1),
                title: "改名".into(),
            })
            .unwrap();
        session.save(None).expect("second save reuses the path");
        assert_eq!(session.path(), Some(path.as_path()));
    }

    #[test]
    fn opening_a_newer_major_version_is_refused() {
        let scratch = Scratch::new("version");
        let path = scratch.file("future.mcm");
        std::fs::write(&path, "%mcm 99\n- 甲 #t1\n").expect("write fixture");

        let mut session = Session::new();
        let error = session.open(&path).expect_err("must refuse");
        assert_eq!(error.code(), "E_VERSION_TOO_NEW");
    }

    #[test]
    fn opening_binary_content_is_refused() {
        let scratch = Scratch::new("binary");
        let path = scratch.file("archive.mcm");
        std::fs::write(&path, [0x50, 0x4b, 0x03, 0x04, 0x00, 0x01]).expect("write fixture");

        let mut session = Session::new();
        let error = session.open(&path).expect_err("must refuse");
        assert_eq!(error.code(), "E_FILE_IO");
    }

    #[test]
    fn opening_a_damaged_file_recovers_the_healthy_part() {
        let scratch = Scratch::new("damaged");
        let path = scratch.file("damaged.mcm");
        std::fs::write(&path, "%mcm 1\n- 正常 #t1\n这一行坏了\n- 也正常 #t2\n")
            .expect("write fixture");

        let mut session = Session::new();
        session.open(&path).expect("damaged files still open");
        assert_eq!(session.plan().tasks.len(), 2);
        assert_eq!(session.recovery_report().count(), 1);
        // The quarantined text is preserved on the next save.
        assert!(session.outline_text().contains("这一行坏了"));
    }

    #[test]
    fn crlf_and_bom_files_open_and_normalise() {
        let scratch = Scratch::new("crlf");
        let path = scratch.file("crlf.mcm");
        std::fs::write(&path, "\u{feff}%mcm 1\r\n- 甲 #t1\r\n").expect("write fixture");

        let mut session = Session::new();
        session.open(&path).expect("open");
        assert_eq!(session.plan().tasks.len(), 1);
        let text = session.outline_text();
        assert!(!text.contains('\r'));
        assert!(!text.starts_with('\u{feff}'));
    }

    #[test]
    fn a_failed_save_leaves_the_original_file_intact() {
        let scratch = Scratch::new("atomic");
        let path = scratch.file("plan.mcm");
        let mut session = Session::new();
        session.apply_outline_text("- 原始 #t1\n");
        session.save(Some(&path)).expect("first save");
        let original = std::fs::read_to_string(&path).expect("read back");

        // A directory cannot be overwritten by the rename, so the save fails.
        let blocked = scratch.file("blocked.mcm");
        std::fs::create_dir_all(&blocked).expect("make dir");
        assert!(session.save(Some(&blocked)).is_err());
        assert_eq!(std::fs::read_to_string(&path).expect("read"), original);
    }

    #[test]
    fn saving_leaves_no_temp_files_behind() {
        let scratch = Scratch::new("tempclean");
        let path = scratch.file("plan.mcm");
        let mut session = Session::new();
        session.apply_outline_text("- 甲 #t1\n");
        session.save(Some(&path)).expect("save");

        let leftovers: Vec<_> = std::fs::read_dir(&scratch.0)
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "temp files must be renamed away");
    }

    #[test]
    fn opening_clears_the_undo_journal() {
        let scratch = Scratch::new("journal");
        let path = scratch.file("plan.mcm");
        let mut session = Session::new();
        session.apply_outline_text("- 甲 #t1\n");
        session.save(Some(&path)).expect("save");

        let mut reopened = Session::new();
        reopened.apply_outline_text("- 别的 #t1\n");
        reopened.open(&path).expect("open");
        assert_eq!(
            reopened.undo_depth(),
            0,
            "a fresh document starts a fresh history"
        );
    }

    #[test]
    fn error_codes_match_contract() {
        assert_eq!(SessionError::NeedPath.code(), "E_NEED_PATH");
        assert_eq!(SessionError::FileIo("x".into()).code(), "E_FILE_IO");
        assert_eq!(
            SessionError::VersionTooNew {
                found: 2,
                supported: 1
            }
            .code(),
            "E_VERSION_TOO_NEW"
        );
    }
}
