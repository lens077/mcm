#![forbid(unsafe_code)]

//! Editable XMind and Visio exporters for MCM plans.
//!
//! Both exporters produce structured, re-editable objects — never bitmaps —
//! and report every degraded element so nothing is lost silently (宪法 VI).

pub mod report;
pub mod vsdx;
pub mod xmind;

pub use report::{DegradedItem, ExportError, ExportFormat, ExportReport, MappedItem};
