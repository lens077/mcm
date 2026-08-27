//! Export reports: what was mapped, what was degraded, what needs attention.
//!
//! Degradation is never silent — every element the target format cannot express
//! natively is listed here so the UI can show it (spec FR-021 / SC-008).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Xmind,
    Vsdx,
}

/// One category of successfully mapped content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappedItem {
    /// e.g. "任务", "依赖", "里程碑"
    pub kind: String,
    pub count: usize,
    /// How it appears in the target format.
    pub representation: String,
}

/// One element that could not be represented natively.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DegradedItem {
    /// Which element (e.g. "任务 t3").
    pub element: String,
    /// What the model expresses (e.g. "日期 2026-09-01..2026-09-05").
    pub original: String,
    /// How it survives in the export (e.g. "标签文本").
    pub fallback: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportReport {
    pub format: ExportFormat,
    pub output_path: String,
    #[serde(default)]
    pub mapped: Vec<MappedItem>,
    #[serde(default)]
    pub degraded: Vec<DegradedItem>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl ExportReport {
    #[must_use]
    pub fn new(format: ExportFormat, output_path: impl Into<String>) -> Self {
        Self {
            format,
            output_path: output_path.into(),
            mapped: Vec::new(),
            degraded: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn map(&mut self, kind: &str, count: usize, representation: &str) {
        if count == 0 {
            return;
        }
        self.mapped.push(MappedItem {
            kind: kind.to_owned(),
            count,
            representation: representation.to_owned(),
        });
    }

    pub fn degrade(&mut self, element: impl Into<String>, original: &str, fallback: &str) {
        self.degraded.push(DegradedItem {
            element: element.into(),
            original: original.to_owned(),
            fallback: fallback.to_owned(),
        });
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    #[must_use]
    pub fn degraded_count(&self) -> usize {
        self.degraded.len()
    }

    #[must_use]
    pub fn mapped_total(&self) -> usize {
        self.mapped.iter().map(|item| item.count).sum()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("导出失败：{0}")]
    Io(String),
    #[error("导出内容自检失败：{0}")]
    SelfCheck(String),
}

impl ExportError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            ExportError::Io(_) => "E_EXPORT_IO",
            ExportError::SelfCheck(_) => "E_INTERNAL",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_records_counts_and_representation() {
        let mut report = ExportReport::new(ExportFormat::Xmind, "/tmp/a.xmind");
        report.map("任务", 5, "topic");
        report.map("依赖", 2, "relationship");
        assert_eq!(report.mapped.len(), 2);
        assert_eq!(report.mapped_total(), 7);
    }

    #[test]
    fn zero_counts_are_not_recorded() {
        let mut report = ExportReport::new(ExportFormat::Xmind, "/tmp/a.xmind");
        report.map("里程碑", 0, "flag topic");
        assert!(report.mapped.is_empty());
    }

    #[test]
    fn degraded_items_name_the_element_and_both_forms() {
        let mut report = ExportReport::new(ExportFormat::Xmind, "/tmp/a.xmind");
        report.degrade("任务 t3", "日期 2026-09-01..2026-09-05", "标签文本");
        assert_eq!(report.degraded_count(), 1);
        let item = &report.degraded[0];
        assert!(!item.element.is_empty());
        assert!(!item.original.is_empty());
        assert!(!item.fallback.is_empty());
    }

    #[test]
    fn warnings_accumulate() {
        let mut report = ExportReport::new(ExportFormat::Vsdx, "/tmp/a.vsdx");
        report.warn("规划仍有 2 个校验错误");
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn report_round_trips_through_json() {
        let mut report = ExportReport::new(ExportFormat::Xmind, "/tmp/a.xmind");
        report.map("任务", 1, "topic");
        report.degrade("任务 t1", "负责人 王芳", "标签");
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(json.contains("\"format\":\"xmind\""), "{json}");
        let restored: ExportReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, report);
    }

    #[test]
    fn error_codes_match_the_ipc_contract() {
        assert_eq!(ExportError::Io("x".into()).code(), "E_EXPORT_IO");
        assert_eq!(ExportError::SelfCheck("x".into()).code(), "E_INTERNAL");
    }
}
