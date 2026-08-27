//! Undo/redo journal (data-model.md §撤销栈, spec FR-012).
//!
//! Stores inverse commands rather than whole-model snapshots, so memory stays
//! proportional to the number of edits rather than to plan size (research R10).

use super::commands::EditCommand;

/// One journal entry: the command that was applied plus the command that
/// undoes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub applied: EditCommand,
    pub inverse: EditCommand,
}

/// Unbounded within a session; saving does not truncate it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Journal {
    undo: Vec<JournalEntry>,
    redo: Vec<JournalEntry>,
}

impl Journal {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a freshly applied command. A new edit invalidates the redo path.
    pub fn record(&mut self, applied: EditCommand, inverse: EditCommand) {
        self.undo.push(JournalEntry { applied, inverse });
        self.redo.clear();
    }

    /// Pops the next command to undo. The caller applies `inverse` and reports
    /// the resulting inverse-of-the-inverse through `finish_undo`.
    #[must_use]
    pub fn take_undo(&mut self) -> Option<JournalEntry> {
        self.undo.pop()
    }

    /// Moves an undone entry onto the redo stack.
    pub fn finish_undo(&mut self, entry: JournalEntry) {
        self.redo.push(entry);
    }

    #[must_use]
    pub fn take_redo(&mut self) -> Option<JournalEntry> {
        self.redo.pop()
    }

    /// Moves a redone entry back onto the undo stack.
    pub fn finish_redo(&mut self, entry: JournalEntry) {
        self.undo.push(entry);
    }

    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    #[must_use]
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Clears both stacks (used when a brand-new document is loaded).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TaskId;

    fn rename(id: u32, title: &str) -> EditCommand {
        EditCommand::RenameTask {
            id: TaskId(id),
            title: title.to_owned(),
        }
    }

    #[test]
    fn new_journal_is_empty() {
        let journal = Journal::new();
        assert_eq!(journal.undo_depth(), 0);
        assert_eq!(journal.redo_depth(), 0);
        assert!(!journal.can_undo());
        assert!(!journal.can_redo());
    }

    #[test]
    fn recording_grows_the_undo_stack() {
        let mut journal = Journal::new();
        journal.record(rename(1, "新"), rename(1, "旧"));
        assert_eq!(journal.undo_depth(), 1);
        assert!(journal.can_undo());
    }

    #[test]
    fn undo_moves_entries_onto_the_redo_stack() {
        let mut journal = Journal::new();
        journal.record(rename(1, "新"), rename(1, "旧"));
        let entry = journal.take_undo().expect("entry");
        assert_eq!(entry.inverse, rename(1, "旧"));
        journal.finish_undo(entry);
        assert_eq!(journal.undo_depth(), 0);
        assert_eq!(journal.redo_depth(), 1);
    }

    #[test]
    fn redo_moves_entries_back() {
        let mut journal = Journal::new();
        journal.record(rename(1, "新"), rename(1, "旧"));
        let entry = journal.take_undo().expect("entry");
        journal.finish_undo(entry);
        let entry = journal.take_redo().expect("redo entry");
        journal.finish_redo(entry);
        assert_eq!(journal.undo_depth(), 1);
        assert_eq!(journal.redo_depth(), 0);
    }

    #[test]
    fn a_new_edit_clears_the_redo_stack() {
        let mut journal = Journal::new();
        journal.record(rename(1, "一"), rename(1, "零"));
        let entry = journal.take_undo().expect("entry");
        journal.finish_undo(entry);
        assert_eq!(journal.redo_depth(), 1);

        journal.record(rename(2, "二"), rename(2, "零"));
        assert_eq!(journal.redo_depth(), 0, "new edits must invalidate redo");
    }

    #[test]
    fn undo_on_empty_stack_is_a_no_op() {
        let mut journal = Journal::new();
        assert!(journal.take_undo().is_none());
        assert!(journal.take_redo().is_none());
    }

    #[test]
    fn depth_is_unbounded_within_a_session() {
        let mut journal = Journal::new();
        for index in 0..1000u32 {
            journal.record(rename(index, "x"), rename(index, "y"));
        }
        assert_eq!(journal.undo_depth(), 1000);
    }

    #[test]
    fn clear_resets_both_stacks() {
        let mut journal = Journal::new();
        journal.record(rename(1, "新"), rename(1, "旧"));
        let entry = journal.take_undo().expect("entry");
        journal.finish_undo(entry);
        journal.clear();
        assert_eq!(journal.undo_depth(), 0);
        assert_eq!(journal.redo_depth(), 0);
    }
}
