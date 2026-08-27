//! Recovery semantics for damaged files
//! (contracts/plan-file-format.md §恢复语义, spec FR-015).
//!
//! Unparsable lines are quarantined rather than dropped: they surface as `P-*`
//! issues and are written back as `# [mcm:recovered] <原文>` comments on save,
//! so nothing is ever lost silently.

use crate::model::Plan;

/// Marker prefix used when quarantined lines are written back to disk.
pub const RECOVERED_PREFIX: &str = "# [mcm:recovered] ";

/// Outcome of opening a document that needed repair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    /// Lines that could not be parsed, in document order.
    pub quarantined: Vec<String>,
}

impl RecoveryReport {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.quarantined.is_empty()
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.quarantined.len()
    }
}

/// Reads back the quarantine list captured during parsing.
#[must_use]
pub fn report_for(plan: &Plan) -> RecoveryReport {
    RecoveryReport {
        quarantined: plan.recovered_lines.clone(),
    }
}

/// True when the bytes are clearly not a text outline (ZIP, image, ...).
///
/// Such files are refused outright rather than "recovered" into an empty plan
/// (contracts/plan-file-format.md §恢复语义).
#[must_use]
pub fn is_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    // A NUL byte in the first block is the classic binary signal.
    let window = &bytes[..bytes.len().min(8000)];
    if window.contains(&0) {
        return true;
    }
    // Otherwise require valid UTF-8: mojibake is not recoverable either.
    std::str::from_utf8(bytes).is_err()
}

/// Strips a previously written recovery comment back to its original text, so
/// reopening a repaired file does not double-wrap the quarantined line.
#[must_use]
pub fn unwrap_recovered(line: &str) -> Option<&str> {
    line.strip_prefix(RECOVERED_PREFIX)
}

/// Normalises input for parsing: strips a UTF-8 BOM and converts CRLF to LF
/// (contracts/plan-file-format.md §基本约定).
#[must_use]
pub fn normalise_input(text: &str) -> String {
    let without_bom = text.strip_prefix('\u{feff}').unwrap_or(text);
    if without_bom.contains('\r') {
        without_bom.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        without_bom.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::{parse, serialize};

    #[test]
    fn clean_documents_report_nothing() {
        let plan = parse("%mcm 1\n- 甲 #t1\n").plan;
        assert!(report_for(&plan).is_clean());
    }

    #[test]
    fn damaged_lines_are_quarantined_not_dropped() {
        let plan = parse("%mcm 1\n- 正常 #t1\n这一行是乱码\n- 也正常 #t2\n").plan;
        let report = report_for(&plan);
        assert_eq!(report.count(), 1);
        assert_eq!(plan.tasks.len(), 2, "healthy tasks must survive");
    }

    #[test]
    fn quarantined_lines_are_written_back_as_comments() {
        let plan = parse("%mcm 1\n- 正常 #t1\n乱码行\n").plan;
        let text = serialize(&plan);
        assert!(text.contains(RECOVERED_PREFIX), "{text}");
        assert!(
            text.contains("乱码行"),
            "original text must be preserved: {text}"
        );
    }

    #[test]
    fn recovery_comments_can_be_unwrapped() {
        let line = format!("{RECOVERED_PREFIX}原始内容");
        assert_eq!(unwrap_recovered(&line), Some("原始内容"));
        assert_eq!(unwrap_recovered("# 普通注释"), None);
    }

    #[test]
    fn binary_content_is_refused() {
        assert!(is_binary(&[0x50, 0x4b, 0x03, 0x04, 0x00, 0x01]));
        assert!(is_binary(&[0xff, 0xfe, 0x00]));
        assert!(!is_binary("%mcm 1\n- 任务 #t1\n".as_bytes()));
        assert!(!is_binary(b""));
    }

    #[test]
    fn invalid_utf8_is_treated_as_binary() {
        assert!(is_binary(&[0xc3, 0x28]));
    }

    #[test]
    fn bom_and_crlf_are_normalised() {
        let text = "\u{feff}%mcm 1\r\n- 甲 #t1\r\n";
        let normalised = normalise_input(text);
        assert!(!normalised.starts_with('\u{feff}'));
        assert!(!normalised.contains('\r'));
        assert_eq!(parse(&normalised).plan.tasks.len(), 1);
    }

    #[test]
    fn lone_cr_line_endings_are_normalised() {
        let normalised = normalise_input("%mcm 1\r- 甲 #t1\r");
        assert_eq!(normalised.lines().count(), 2);
    }

    #[test]
    fn normalising_clean_text_is_a_no_op() {
        let text = "%mcm 1\n- 甲 #t1\n";
        assert_eq!(normalise_input(text), text);
    }
}
