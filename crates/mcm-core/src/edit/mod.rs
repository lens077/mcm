//! The only path that mutates a plan: a closed command set plus an undo journal.

pub mod commands;
pub mod journal;

pub use commands::{EditCommand, EditError, apply, stale_views};
pub use journal::{Journal, JournalEntry};
