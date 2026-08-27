#![forbid(unsafe_code)]

//! Deterministic domain core for MCM project plans.
//!
//! Everything that decides *what the plan is* lives here: the model, the
//! outline grammar, validation, derivation, layout and scene projection.
//! The Tauri shell and the browser front end are thin layers on top.

pub mod edit;
pub mod layout;
pub mod model;
pub mod outline;
pub mod scene;
pub mod session;
pub mod validate;

pub use model::{
    Date, DateRange, Dependency, DependencyKind, ElementRef, IdAllocator, Milestone, MilestoneId,
    Plan, Schedule, Severity, Task, TaskId, ValidationIssue,
};
pub use session::{PlanCounts, Session, SessionError, SessionState};

/// Native file format major version, written as `%mcm <n>`.
pub const FORMAT_VERSION: u32 = 1;
