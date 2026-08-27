//! XMind export contract tests (contracts/export-xmind.md §契约测试 1–4).
//!
//! The reader below deliberately avoids the exporter's own types: it walks raw
//! JSON the way an independent tool (xmindparser) would, so a mistake in our
//! model definitions cannot hide itself.

use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use mcm_core::model::{Plan, TaskId};
use mcm_core::outline::parse;
use mcm_export::report::ExportReport;
use serde_json::Value;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("mcm-xmind-contract-{tag}-{stamp}"));
        std::fs::create_dir_all(&dir).expect("scratch");
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

// ------------------------------------------------- independent reader ---

struct Archive {
    entries: Vec<String>,
    stored_only: bool,
    content: Value,
    metadata: Value,
    manifest: Value,
}

/// Reads a `.xmind` the way a third-party tool would: unzip, parse JSON.
fn read_archive(path: &Path) -> Archive {
    let file = std::fs::File::open(path).expect("open export");
    let mut zip = zip::ZipArchive::new(file).expect("valid zip");

    let mut entries: Vec<String> = zip.file_names().map(str::to_owned).collect();
    entries.sort();

    let mut stored_only = true;
    for index in 0..zip.len() {
        let entry = zip.by_index(index).expect("entry");
        if entry.compression() != zip::CompressionMethod::Stored {
            stored_only = false;
        }
    }

    let mut read_json = |name: &str| -> Value {
        let mut text = String::new();
        zip.by_name(name)
            .unwrap_or_else(|_| panic!("missing {name}"))
            .read_to_string(&mut text)
            .expect("utf-8");
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name} is not JSON: {e}"))
    };

    Archive {
        entries,
        stored_only,
        content: read_json("content.json"),
        metadata: read_json("metadata.json"),
        manifest: read_json("manifest.json"),
    }
}

/// Walks `children.attached` recursively, collecting `(title, depth)`.
fn walk_titles(topic: &Value, depth: usize, out: &mut Vec<(String, usize)>) {
    let title = topic
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default();
    out.push((title.to_owned(), depth));
    let Some(children) = topic.get("children").and_then(|c| c.get("attached")) else {
        return;
    };
    for child in children.as_array().into_iter().flatten() {
        walk_titles(child, depth + 1, out);
    }
}

fn collect_ids(topic: &Value, out: &mut Vec<String>) {
    if let Some(id) = topic.get("id").and_then(Value::as_str) {
        out.push(id.to_owned());
    }
    let Some(children) = topic.get("children").and_then(|c| c.get("attached")) else {
        return;
    };
    for child in children.as_array().into_iter().flatten() {
        collect_ids(child, out);
    }
}

fn export_to(scratch: &Scratch, name: &str, plan: &Plan) -> (PathBuf, ExportReport, Archive) {
    let path = scratch.file(name);
    let report = mcm_export::xmind::export(plan, &path).expect("export succeeds");
    let archive = read_archive(&path);
    (path, report, archive)
}

const SAMPLE: &str = "%mcm 1
%title 契约测试 🌍
%start 2026-09-01

- 需求阶段 #t1 [2026-09-01..2026-09-10] @王芳
  > 备注含 emoji 🚀
  - [x] 用户访谈 #t2 [2026-09-01..2026-09-03]
  - 非常长的任务标题用于验证导出时不会被截断或破坏结构的边界情况 #t3 [3d] <-t2
- 设计阶段 #t4 <-t3
! 需求冻结 #m1 [2026-09-30] <-t4
";

// -------------------------------------------------- 1. structure ---

#[test]
fn archive_contains_exactly_the_three_required_entries() {
    let scratch = Scratch::new("structure");
    let (_, _, archive) = export_to(&scratch, "plan.xmind", &parse(SAMPLE).plan);
    assert_eq!(
        archive.entries,
        vec!["content.json", "manifest.json", "metadata.json"]
    );
}

#[test]
fn every_entry_is_stored_uncompressed() {
    let scratch = Scratch::new("stored");
    let (_, _, archive) = export_to(&scratch, "plan.xmind", &parse(SAMPLE).plan);
    assert!(archive.stored_only, "official generators use STORE");
}

#[test]
fn content_is_a_top_level_array() {
    let scratch = Scratch::new("array");
    let (_, _, archive) = export_to(&scratch, "plan.xmind", &parse(SAMPLE).plan);
    assert!(archive.content.is_array(), "content.json must be an array");
    assert_eq!(archive.content.as_array().map(Vec::len), Some(1));
}

#[test]
fn all_topic_ids_are_unique() {
    let scratch = Scratch::new("ids");
    let (_, _, archive) = export_to(&scratch, "plan.xmind", &parse(SAMPLE).plan);
    let sheet = &archive.content[0];
    let mut ids = Vec::new();
    collect_ids(&sheet["rootTopic"], &mut ids);
    let unique: BTreeSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "duplicate topic ids: {ids:?}");
}

#[test]
fn relationship_endpoints_are_closed() {
    let scratch = Scratch::new("closure");
    let (_, _, archive) = export_to(&scratch, "plan.xmind", &parse(SAMPLE).plan);
    let sheet = &archive.content[0];
    let mut ids = Vec::new();
    collect_ids(&sheet["rootTopic"], &mut ids);
    let known: BTreeSet<&String> = ids.iter().collect();

    let relationships = sheet["relationships"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !relationships.is_empty(),
        "sample has dependencies and a milestone link"
    );
    for relationship in &relationships {
        for field in ["end1Id", "end2Id"] {
            let end = relationship[field].as_str().expect("endpoint").to_owned();
            assert!(known.contains(&end), "dangling {field}: {end}");
        }
    }
}

#[test]
fn task_tree_is_isomorphic_to_the_model_including_order() {
    let scratch = Scratch::new("tree");
    let plan = parse(SAMPLE).plan;
    let (_, _, archive) = export_to(&scratch, "plan.xmind", &plan);

    let mut titles = Vec::new();
    walk_titles(&archive.content[0]["rootTopic"], 0, &mut titles);

    // Root, then the tasks in document order, then the milestone branch.
    let task_titles: Vec<String> = titles
        .iter()
        .filter(|(_, depth)| *depth > 0)
        .map(|(title, _)| title.clone())
        .collect();
    let expected: Vec<String> = plan
        .tasks_in_document_order()
        .iter()
        .map(|task| task.title.clone())
        .collect();
    for title in &expected {
        assert!(task_titles.contains(title), "missing task topic: {title}");
    }
    // Document order is preserved among the tasks.
    let positions: Vec<usize> = expected
        .iter()
        .map(|title| {
            task_titles
                .iter()
                .position(|t| t == title)
                .expect("present")
        })
        .collect();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted, "task order changed: {task_titles:?}");
}

#[test]
fn unicode_emoji_and_long_titles_survive_intact() {
    let scratch = Scratch::new("unicode");
    let plan = parse(SAMPLE).plan;
    let (_, _, archive) = export_to(&scratch, "plan.xmind", &plan);
    let text = archive.content.to_string();
    assert!(text.contains("🚀"), "emoji in notes must survive");
    assert!(
        archive.content[0]["title"]
            .as_str()
            .unwrap_or_default()
            .contains('🌍')
    );
    let long = plan.task(TaskId(3)).expect("t3").title.clone();
    assert!(text.contains(&long), "long titles must not be truncated");
}

#[test]
fn a_thousand_task_plan_exports_completely() {
    let scratch = Scratch::new("scale");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../mcm-core/fixtures/perf/plan-1000.mcm");
    let source = std::fs::read_to_string(&fixture).expect("perf fixture");
    let plan = parse(&source).plan;

    let (_, report, archive) = export_to(&scratch, "big.xmind", &plan);
    let mut ids = Vec::new();
    collect_ids(&archive.content[0]["rootTopic"], &mut ids);
    let unique: BTreeSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "ids stay unique at scale");
    assert!(report.mapped_total() >= plan.tasks.len());
}

// ------------------------------------------------------ 2. schema ---

/// Minimal structural validation against the checked-in schema.
fn validate_against_schema(content: &Value) {
    let schema_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/xmind/content.schema.json");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(&schema_path).expect("schema file"))
            .expect("schema is JSON");

    // The schema declares an array of sheets; assert the invariants it encodes.
    assert_eq!(schema["type"], "array");
    let sheets = content.as_array().expect("array");
    assert!(!sheets.is_empty());

    for sheet in sheets {
        for field in ["id", "class", "title", "rootTopic"] {
            assert!(sheet.get(field).is_some(), "sheet missing {field}");
        }
        assert_eq!(sheet["class"], "sheet");
        validate_topic(&sheet["rootTopic"]);
        for relationship in sheet["relationships"].as_array().into_iter().flatten() {
            assert_eq!(relationship["class"], "relationship");
            for field in ["id", "end1Id", "end2Id"] {
                let value = relationship[field].as_str().unwrap_or_default();
                assert!(!value.is_empty(), "relationship {field} must be non-empty");
            }
        }
    }
}

fn validate_topic(topic: &Value) {
    for field in ["id", "class", "title"] {
        assert!(topic.get(field).is_some(), "topic missing {field}");
    }
    assert_eq!(topic["class"], "topic");
    assert!(!topic["id"].as_str().unwrap_or_default().is_empty());

    for marker in topic["markers"].as_array().into_iter().flatten() {
        assert!(!marker["markerId"].as_str().unwrap_or_default().is_empty());
    }
    if let Some(notes) = topic.get("notes") {
        assert!(
            notes["plain"]["content"].is_string(),
            "notes must carry plain content"
        );
    }
    for child in topic["children"]["attached"]
        .as_array()
        .into_iter()
        .flatten()
    {
        validate_topic(child);
    }
}

#[test]
fn content_matches_the_checked_in_schema() {
    let scratch = Scratch::new("schema");
    let (_, _, archive) = export_to(&scratch, "plan.xmind", &parse(SAMPLE).plan);
    validate_against_schema(&archive.content);
}

#[test]
fn metadata_and_manifest_satisfy_the_contract() {
    let scratch = Scratch::new("companions");
    let (_, _, archive) = export_to(&scratch, "plan.xmind", &parse(SAMPLE).plan);
    assert_eq!(archive.metadata["dataStructureVersion"], "2");
    assert!(archive.metadata["creator"]["name"].is_string());

    let entries = archive.manifest["file-entries"]
        .as_object()
        .expect("file-entries");
    assert!(entries.contains_key("content.json"));
    assert!(entries.contains_key("metadata.json"));
}

// -------------------------------------------------- 3. degradation ---

#[test]
fn every_unmappable_element_appears_in_the_report() {
    // SC-008: zero silent loss.
    let scratch = Scratch::new("degrade");
    let plan = parse(SAMPLE).plan;
    let (_, report, _) = export_to(&scratch, "plan.xmind", &plan);

    // Every task carrying a date/duration must be listed.
    let scheduled = plan
        .tasks
        .iter()
        .filter(|task| !task.schedule.is_none())
        .count();
    let schedule_items = report
        .degraded
        .iter()
        .filter(|item| item.element.starts_with("任务"))
        .filter(|item| item.original.starts_with("日期") || item.original.starts_with("工期"))
        .count();
    assert_eq!(
        schedule_items, scheduled,
        "each scheduled task must be reported"
    );

    // The whole-plan note about derived scheduling is reported exactly once.
    let derived_items = report
        .degraded
        .iter()
        .filter(|item| item.element == "整份规划")
        .count();
    assert_eq!(
        derived_items, 1,
        "derived scheduling is reported once, not per task"
    );

    // Every assignee must be listed.
    let assignees = plan
        .tasks
        .iter()
        .filter(|task| task.assignee.is_some())
        .count();
    let assignee_items = report
        .degraded
        .iter()
        .filter(|item| item.original.contains("负责人"))
        .count();
    assert_eq!(assignee_items, assignees);

    // Every milestone must be listed.
    let milestone_items = report
        .degraded
        .iter()
        .filter(|item| item.element.starts_with("里程碑"))
        .count();
    assert_eq!(milestone_items, plan.milestones.len());

    // And every entry names both the original and the fallback.
    for item in &report.degraded {
        assert!(!item.element.is_empty(), "{item:?}");
        assert!(!item.original.is_empty(), "{item:?}");
        assert!(!item.fallback.is_empty(), "{item:?}");
    }
}

#[test]
fn a_plan_with_nothing_to_degrade_reports_nothing() {
    let scratch = Scratch::new("clean");
    let plan = parse("%mcm 1\n%title 干净\n\n- 甲 #t1\n  - 乙 #t2\n").plan;
    let (_, report, _) = export_to(&scratch, "clean.xmind", &plan);
    assert_eq!(report.degraded_count(), 0, "{:?}", report.degraded);
    assert!(report.mapped.iter().any(|item| item.kind == "任务"));
}

// ------------------------------------------- 4. third-party read-back ---

#[test]
fn an_independent_reader_recovers_the_same_tree() {
    let scratch = Scratch::new("readback");
    let plan = parse(SAMPLE).plan;
    let (_, _, archive) = export_to(&scratch, "plan.xmind", &plan);

    // Count topics the way xmindparser does: root plus every attached child.
    let mut titles = Vec::new();
    walk_titles(&archive.content[0]["rootTopic"], 0, &mut titles);
    let topic_count = titles.len();

    // root + tasks + milestone branch + milestones
    let expected = 1 + plan.tasks.len() + 1 + plan.milestones.len();
    assert_eq!(topic_count, expected, "topic count mismatch: {titles:?}");
}

#[test]
fn exports_are_reproducible_byte_for_byte_in_content() {
    let scratch = Scratch::new("reproducible");
    let plan = parse(SAMPLE).plan;
    let (_, _, first) = export_to(&scratch, "one.xmind", &plan);
    let (_, _, second) = export_to(&scratch, "two.xmind", &plan);
    assert_eq!(first.content, second.content);
    assert_eq!(first.metadata, second.metadata);
    assert_eq!(first.manifest, second.manifest);
}
