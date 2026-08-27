use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable task identifier rendered as `t<n>` in the outline grammar.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub u32);

/// Stable milestone identifier rendered as `m<n>` in the outline grammar.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MilestoneId(pub u32);

impl TaskId {
    #[must_use]
    pub fn as_token(self) -> String {
        format!("t{}", self.0)
    }

    /// Parses `t12` style tokens. Returns `None` for any other shape.
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        parse_prefixed(token, 't').map(TaskId)
    }
}

impl MilestoneId {
    #[must_use]
    pub fn as_token(self) -> String {
        format!("m{}", self.0)
    }

    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        parse_prefixed(token, 'm').map(MilestoneId)
    }
}

fn parse_prefixed(token: &str, prefix: char) -> Option<u32> {
    let rest = token.strip_prefix(prefix)?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Reject leading zeroes so every id has exactly one canonical spelling.
    if rest.len() > 1 && rest.starts_with('0') {
        return None;
    }
    rest.parse().ok()
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "t{}", self.0)
    }
}

impl fmt::Debug for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TaskId({self})")
    }
}

impl fmt::Display for MilestoneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "m{}", self.0)
    }
}

impl fmt::Debug for MilestoneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MilestoneId({self})")
    }
}

/// Monotonic id allocator: never reuses a number within one session so that
/// undo/redo and cross-view references stay stable (data-model.md §基础类型).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdAllocator {
    next_task: u32,
    next_milestone: u32,
}

impl IdAllocator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_task: 1,
            next_milestone: 1,
        }
    }

    /// Rebuilds the allocator so that it starts after every id already in use.
    pub fn observe_task(&mut self, id: TaskId) {
        self.next_task = self.next_task.max(id.0 + 1);
    }

    pub fn observe_milestone(&mut self, id: MilestoneId) {
        self.next_milestone = self.next_milestone.max(id.0 + 1);
    }

    pub fn next_task(&mut self) -> TaskId {
        let id = TaskId(self.next_task.max(1));
        self.next_task = id.0 + 1;
        id
    }

    pub fn next_milestone(&mut self) -> MilestoneId {
        let id = MilestoneId(self.next_milestone.max(1));
        self.next_milestone = id.0 + 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_tokens() {
        assert_eq!(TaskId::parse("t7"), Some(TaskId(7)));
        assert_eq!(MilestoneId::parse("m12"), Some(MilestoneId(12)));
        assert_eq!(TaskId::parse("t0"), Some(TaskId(0)));
    }

    #[test]
    fn rejects_non_canonical_tokens() {
        assert_eq!(TaskId::parse("t"), None);
        assert_eq!(TaskId::parse("t01"), None);
        assert_eq!(TaskId::parse("x1"), None);
        assert_eq!(TaskId::parse("t1x"), None);
        assert_eq!(MilestoneId::parse("t1"), None);
    }

    #[test]
    fn allocator_is_monotonic_and_never_reuses() {
        let mut alloc = IdAllocator::new();
        assert_eq!(alloc.next_task(), TaskId(1));
        assert_eq!(alloc.next_task(), TaskId(2));
        alloc.observe_task(TaskId(9));
        assert_eq!(alloc.next_task(), TaskId(10));
        // Observing a lower id must not rewind the sequence.
        alloc.observe_task(TaskId(3));
        assert_eq!(alloc.next_task(), TaskId(11));
    }

    #[test]
    fn round_trips_display_and_parse() {
        let id = TaskId(42);
        assert_eq!(TaskId::parse(&id.as_token()), Some(id));
        let ms = MilestoneId(3);
        assert_eq!(MilestoneId::parse(&ms.as_token()), Some(ms));
    }
}
