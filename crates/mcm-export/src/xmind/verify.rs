//! Pre-write self check (contracts/export-xmind.md §生成规则).
//!
//! The exporter re-reads its own output and validates structure plus reference
//! closure before anything touches the target path, so a corrupt file is never
//! written.

use std::collections::BTreeSet;

use super::model::{Sheet, Topic};
use crate::report::ExportError;

/// Validates the JSON payloads that are about to be zipped.
pub fn check_payload(content: &str, metadata: &str, manifest: &str) -> Result<(), ExportError> {
    let sheets: Vec<Sheet> = serde_json::from_str(content)
        .map_err(|error| ExportError::SelfCheck(format!("content.json 不是合法结构：{error}")))?;

    if sheets.is_empty() {
        return Err(ExportError::SelfCheck(
            "content.json 至少需要一个 sheet".into(),
        ));
    }

    for sheet in &sheets {
        if sheet.class != "sheet" {
            return Err(ExportError::SelfCheck(format!(
                "sheet.class 非法：{}",
                sheet.class
            )));
        }
        let mut ids = BTreeSet::new();
        check_topic(&sheet.root_topic, &mut ids)?;

        for relationship in &sheet.relationships {
            if relationship.class != "relationship" {
                return Err(ExportError::SelfCheck(format!(
                    "relationship.class 非法：{}",
                    relationship.class
                )));
            }
            for (label, end) in [
                ("end1Id", &relationship.end1_id),
                ("end2Id", &relationship.end2_id),
            ] {
                if !ids.contains(end) {
                    return Err(ExportError::SelfCheck(format!(
                        "relationship {} 指向不存在的 topic：{end}",
                        label
                    )));
                }
            }
        }
    }

    // Both companions must be well-formed JSON objects.
    let metadata_value: serde_json::Value = serde_json::from_str(metadata)
        .map_err(|error| ExportError::SelfCheck(format!("metadata.json 非法：{error}")))?;
    if metadata_value.get("dataStructureVersion").is_none() {
        return Err(ExportError::SelfCheck(
            "metadata.json 缺少 dataStructureVersion".into(),
        ));
    }

    let manifest_value: serde_json::Value = serde_json::from_str(manifest)
        .map_err(|error| ExportError::SelfCheck(format!("manifest.json 非法：{error}")))?;
    let entries = manifest_value
        .get("file-entries")
        .and_then(|value| value.as_object())
        .ok_or_else(|| ExportError::SelfCheck("manifest.json 缺少 file-entries".into()))?;
    for required in ["content.json", "metadata.json"] {
        if !entries.contains_key(required) {
            return Err(ExportError::SelfCheck(format!(
                "manifest.json 未列出 {required}"
            )));
        }
    }

    Ok(())
}

fn check_topic(topic: &Topic, ids: &mut BTreeSet<String>) -> Result<(), ExportError> {
    if topic.class != "topic" {
        return Err(ExportError::SelfCheck(format!(
            "topic.class 非法：{}",
            topic.class
        )));
    }
    if topic.id.is_empty() {
        return Err(ExportError::SelfCheck("topic 缺少 id".into()));
    }
    if !ids.insert(topic.id.clone()) {
        return Err(ExportError::SelfCheck(format!(
            "topic id 重复：{}",
            topic.id
        )));
    }

    // At most one marker per group (contract §生成规则).
    let mut groups = BTreeSet::new();
    for marker in &topic.markers {
        let group = marker
            .marker_id
            .split('-')
            .next()
            .unwrap_or_default()
            .to_owned();
        if !groups.insert(group.clone()) {
            return Err(ExportError::SelfCheck(format!(
                "topic {} 的同组 marker 超过一枚：{group}",
                topic.id
            )));
        }
    }

    if let Some(children) = &topic.children {
        for child in &children.attached {
            check_topic(child, ids)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmind::writer::build;
    use mcm_core::outline::parse;

    fn good() -> (String, String, String) {
        let payload = build(
            &parse("%mcm 1\n- 甲 #t1\n- 乙 #t2 <-t1\n! 冻结 #m1 [2026-09-30] <-t2\n").plan,
            "/tmp/a.xmind",
        );
        (payload.content, payload.metadata, payload.manifest)
    }

    #[test]
    fn a_generated_payload_passes() {
        let (content, metadata, manifest) = good();
        assert!(check_payload(&content, &metadata, &manifest).is_ok());
    }

    #[test]
    fn malformed_content_is_rejected() {
        let (_, metadata, manifest) = good();
        let error = check_payload("{not json", &metadata, &manifest).expect_err("must fail");
        assert_eq!(error.code(), "E_INTERNAL");
    }

    #[test]
    fn empty_sheet_list_is_rejected() {
        let (_, metadata, manifest) = good();
        assert!(check_payload("[]", &metadata, &manifest).is_err());
    }

    #[test]
    fn dangling_relationships_are_rejected() {
        let (content, metadata, manifest) = good();
        let mut sheets: Vec<Sheet> = serde_json::from_str(&content).expect("parse");
        sheets[0].relationships[0].end2_id = "does-not-exist".to_owned();
        let broken = serde_json::to_string(&sheets).expect("serialize");
        let error = check_payload(&broken, &metadata, &manifest).expect_err("must fail");
        assert!(error.to_string().contains("不存在"), "{error}");
    }

    #[test]
    fn duplicate_topic_ids_are_rejected() {
        let (content, metadata, manifest) = good();
        let mut sheets: Vec<Sheet> = serde_json::from_str(&content).expect("parse");
        let duplicate = sheets[0].root_topic.id.clone();
        if let Some(children) = sheets[0].root_topic.children.as_mut() {
            children.attached[0].id = duplicate;
        }
        let broken = serde_json::to_string(&sheets).expect("serialize");
        let error = check_payload(&broken, &metadata, &manifest).expect_err("must fail");
        assert!(error.to_string().contains("重复"), "{error}");
    }

    #[test]
    fn manifest_must_list_both_payloads() {
        let (content, metadata, _) = good();
        let error = check_payload(
            &content,
            &metadata,
            "{\"file-entries\":{\"content.json\":{}}}",
        )
        .expect_err("must fail");
        assert!(error.to_string().contains("metadata.json"), "{error}");
    }

    #[test]
    fn metadata_must_declare_the_structure_version() {
        let (content, _, manifest) = good();
        let error = check_payload(&content, "{\"creator\":{\"name\":\"x\"}}", &manifest)
            .expect_err("must fail");
        assert!(
            error.to_string().contains("dataStructureVersion"),
            "{error}"
        );
    }
}
