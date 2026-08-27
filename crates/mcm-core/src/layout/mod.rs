//! Deterministic layout algorithms. Every layout is a pure function of the
//! model with stable sort keys, so the same plan always yields identical
//! geometry (宪法 IV).

pub mod depgraph;
pub mod milestones;
pub mod timeline;
pub mod wbs;

pub use depgraph::{DepGraphLayout, layout_depgraph};
pub use milestones::{MilestoneLayout, layout_milestones};
pub use timeline::{TimelineLayout, layout_timeline};
pub use wbs::{WbsLayout, layout_wbs};

/// Shared node metrics in logical units (design tokens resolve the colors).
pub const NODE_WIDTH: f64 = 240.0;
pub const NODE_HEIGHT: f64 = 48.0;
pub const H_GAP: f64 = 48.0;
pub const V_GAP: f64 = 16.0;
