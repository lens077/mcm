//! Model → `.xmind` conversion (contracts/export-xmind.md 映射表).

use std::collections::BTreeMap;

use mcm_core::model::{MilestoneId, Plan, Schedule, TaskId, format_date};
use uuid::Uuid;

use crate::report::{DegradedItem, ExportFormat, ExportReport};

use super::model::{Manifest, Marker, Metadata, Notes, Relationship, Sheet, Topic};

/// Fixed namespace so the same plan always yields the same topic ids, keeping
/// exports byte-stable and diffable (contract §映射表: 确定性派生 UUID).
const NAMESPACE: Uuid = Uuid::from_bytes([
    0x6d, 0x63, 0x6d, 0x2d, 0x78, 0x6d, 0x69, 0x6e, 0x64, 0x2d, 0x6e, 0x73, 0x30, 0x30, 0x30, 0x31,
]);

/// Branch that collects milestones, which mind maps cannot express natively.
const MILESTONE_BRANCH: &str = "里程碑";

fn topic_id(kind: &str, key: &str) -> String {
    Uuid::new_v5(&NAMESPACE, format!("{kind}:{key}").as_bytes()).to_string()
}

fn task_topic_id(id: TaskId) -> String {
    topic_id("task", &id.as_token())
}

fn milestone_topic_id(id: MilestoneId) -> String {
    topic_id("milestone", &id.as_token())
}

/// The three JSON payloads that make up a `.xmind` package.
pub struct XmindPayload {
    pub content: String,
    pub metadata: String,
    pub manifest: String,
    pub report: ExportReport,
}

/// Builds the XMind representation of `plan`.
///
/// `output_path` is recorded in the report; nothing is written here.
#[must_use]
pub fn build(plan: &Plan, output_path: &str) -> XmindPayload {
    let mut report = ExportReport::new(ExportFormat::Xmind, output_path);

    let root_id = topic_id("plan", "root");
    let mut root = Topic::new(root_id.clone(), plan.title.clone());

    // Tasks become the main branch structure, in document order.
    let roots = plan.children_of(None);
    let children: Vec<Topic> = roots
        .iter()
        .map(|task| build_task_topic(plan, task.id, &mut report))
        .collect();
    let task_count = plan.tasks.len();

    let mut relationships = Vec::new();
    let mut attached = children;

    // Milestones get their own branch plus relationships back to their tasks.
    if !plan.milestones.is_empty() {
        let branch_id = topic_id("branch", MILESTONE_BRANCH);
        let mut branch = Topic::new(branch_id, MILESTONE_BRANCH);
        let mut milestone_topics = Vec::new();
        for milestone in &plan.milestones {
            let mut topic = Topic::new(milestone_topic_id(milestone.id), milestone.name.clone());
            topic.markers.push(Marker::new("flag-red"));
            topic.labels.push(format_date(milestone.date));
            topic.labels.push(format!("#{}", milestone.id.as_token()));
            milestone_topics.push(topic);

            report.degrade(
                format!("里程碑 {}", milestone.id),
                &format!(
                    "里程碑 {}（{}）",
                    milestone.name,
                    format_date(milestone.date)
                ),
                "脑图分支节点 + flag-red 标记 + 日期标签",
            );

            for task in &milestone.linked_tasks {
                if !plan.has_task(*task) {
                    continue;
                }
                relationships.push(Relationship::new(
                    topic_id("rel-milestone", &format!("{}-{}", milestone.id, task)),
                    &milestone_topic_id(milestone.id),
                    &task_topic_id(*task),
                    "关联",
                ));
            }
        }
        branch.attach(milestone_topics);
        attached.push(branch);
    }

    // Dependencies map to real, editable relationship lines.
    for dep in &plan.dependencies {
        if !plan.has_task(dep.predecessor) || !plan.has_task(dep.successor) {
            continue;
        }
        relationships.push(Relationship::new(
            topic_id("rel-dep", &format!("{}-{}", dep.predecessor, dep.successor)),
            &task_topic_id(dep.predecessor),
            &task_topic_id(dep.successor),
            "依赖",
        ));
    }

    root.attach(attached);

    report.map("任务", task_count, "脑图 topic（children.attached 层级）");
    report.map(
        "依赖",
        plan.dependencies.len(),
        "sheet 级 relationship 连线（可编辑）",
    );
    report.map(
        "里程碑",
        plan.milestones.len(),
        "里程碑分支下的 flag-red 节点",
    );

    // Derived scheduling cannot be recomputed inside XMind at all.
    if plan.tasks.iter().any(|task| !task.schedule.is_none()) {
        report.degrade(
            "整份规划",
            "时间推导结果（依赖驱动的有效日期）",
            "不导出；XMind 端无日期语义，仅保留标签文本",
        );
    }

    let sheet = Sheet {
        id: topic_id("sheet", "main"),
        class: "sheet".to_owned(),
        title: plan.title.clone(),
        root_topic: root,
        relationships,
    };

    let content = serde_json::to_string(&vec![sheet]).unwrap_or_else(|_| "[]".to_owned());
    let metadata = serde_json::to_string(&Metadata::default()).unwrap_or_else(|_| "{}".to_owned());
    let manifest =
        serde_json::to_string(&Manifest::for_entries(&["content.json", "metadata.json"]))
            .unwrap_or_else(|_| "{}".to_owned());

    XmindPayload {
        content,
        metadata,
        manifest,
        report,
    }
}

/// Recursively converts one task (and its subtree) into a topic.
fn build_task_topic(plan: &Plan, id: TaskId, report: &mut ExportReport) -> Topic {
    let Some(task) = plan.task(id) else {
        return Topic::new(task_topic_id(id), String::new());
    };
    let mut topic = Topic::new(task_topic_id(id), task.title.clone());

    // The stable id travels as a label so round-tripping stays traceable.
    topic.labels.push(format!("#{}", id.as_token()));

    if task.done {
        topic.markers.push(Marker::new("task-done"));
    }
    if let Some(notes) = &task.notes {
        topic.notes = Some(Notes::plain(notes));
    }
    if let Some(assignee) = &task.assignee {
        topic.labels.push(format!("@{assignee}"));
        report.degrade(
            format!("任务 {id}"),
            &format!("负责人 {assignee}"),
            "标签 @负责人（XMind 无负责人字段）",
        );
    }
    match task.schedule {
        Schedule::None => {}
        Schedule::Explicit { start, end } => {
            let text = format!("{}..{}", format_date(start), format_date(end));
            topic.labels.push(text.clone());
            report.degrade(
                format!("任务 {id}"),
                &format!("日期 {text}"),
                "标签文本（XMind 无日期语义）",
            );
        }
        Schedule::Duration { days } => {
            let text = format!("{days}d");
            topic.labels.push(text.clone());
            report.degrade(
                format!("任务 {id}"),
                &format!("工期 {text}"),
                "标签文本（XMind 无工期语义）",
            );
        }
    }

    let children: Vec<Topic> = plan
        .children_of(Some(id))
        .iter()
        .map(|child| build_task_topic(plan, child.id, report))
        .collect();
    topic.attach(children);
    topic
}

/// Collects every topic id in the sheet, for uniqueness and closure checks.
#[must_use]
pub fn collect_topic_ids(sheet: &Sheet) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    fn walk(topic: &Topic, counts: &mut BTreeMap<String, usize>) {
        *counts.entry(topic.id.clone()).or_insert(0) += 1;
        if let Some(children) = &topic.children {
            for child in &children.attached {
                walk(child, counts);
            }
        }
    }
    walk(&sheet.root_topic, &mut counts);
    counts
}

/// Degraded items grouped by their fallback representation, for the UI.
#[must_use]
pub fn degraded_by_fallback(report: &ExportReport) -> BTreeMap<&str, Vec<&DegradedItem>> {
    let mut grouped: BTreeMap<&str, Vec<&DegradedItem>> = BTreeMap::new();
    for item in &report.degraded {
        grouped
            .entry(item.fallback.as_str())
            .or_default()
            .push(item);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcm_core::outline::parse;

    const SAMPLE: &str = "%mcm 1\n%title 导出测试\n\n- 甲 #t1 [2026-09-01..2026-09-05] @王芳\n  > 一些备注\n  - [x] 甲一 #t2 [2d]\n- 乙 #t3 <-t2\n! 冻结 #m1 [2026-09-30] <-t3\n";

    fn payload() -> XmindPayload {
        build(&parse(SAMPLE).plan, "/tmp/out.xmind")
    }

    fn sheets(payload: &XmindPayload) -> Vec<Sheet> {
        serde_json::from_str(&payload.content).expect("content parses")
    }

    #[test]
    fn content_is_a_top_level_array_with_one_sheet() {
        let parsed = sheets(&payload());
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].class, "sheet");
    }

    #[test]
    fn sheet_and_root_carry_the_plan_title() {
        let parsed = sheets(&payload());
        assert_eq!(parsed[0].title, "导出测试");
        assert_eq!(parsed[0].root_topic.title, "导出测试");
    }

    #[test]
    fn task_hierarchy_is_preserved_in_order() {
        let parsed = sheets(&payload());
        let children = parsed[0].root_topic.children.as_ref().expect("children");
        // 甲, 乙, 里程碑 branch
        assert_eq!(children.attached.len(), 3);
        assert_eq!(children.attached[0].title, "甲");
        let grandchildren = children.attached[0]
            .children
            .as_ref()
            .expect("grandchildren");
        assert_eq!(grandchildren.attached[0].title, "甲一");
    }

    #[test]
    fn done_tasks_carry_the_task_done_marker() {
        let parsed = sheets(&payload());
        let children = parsed[0].root_topic.children.as_ref().expect("children");
        let sub = children.attached[0].children.as_ref().expect("sub");
        assert!(
            sub.attached[0]
                .markers
                .iter()
                .any(|m| m.marker_id == "task-done")
        );
    }

    #[test]
    fn notes_become_plain_notes_with_trailing_newline() {
        let parsed = sheets(&payload());
        let children = parsed[0].root_topic.children.as_ref().expect("children");
        let notes = children.attached[0].notes.as_ref().expect("notes");
        assert_eq!(notes.plain.content, "一些备注\n");
    }

    #[test]
    fn dates_and_assignees_become_labels() {
        let parsed = sheets(&payload());
        let children = parsed[0].root_topic.children.as_ref().expect("children");
        let labels = &children.attached[0].labels;
        assert!(
            labels.iter().any(|l| l == "2026-09-01..2026-09-05"),
            "{labels:?}"
        );
        assert!(labels.iter().any(|l| l == "@王芳"), "{labels:?}");
        assert!(labels.iter().any(|l| l == "#t1"), "{labels:?}");
    }

    #[test]
    fn dependencies_become_real_relationships() {
        let parsed = sheets(&payload());
        let dependency = parsed[0]
            .relationships
            .iter()
            .find(|rel| rel.title == "依赖")
            .expect("dependency relationship");
        assert_eq!(dependency.end1_id, task_topic_id(TaskId(2)));
        assert_eq!(dependency.end2_id, task_topic_id(TaskId(3)));
    }

    #[test]
    fn milestones_become_a_flagged_branch_with_links() {
        let parsed = sheets(&payload());
        let children = parsed[0].root_topic.children.as_ref().expect("children");
        let branch = children
            .attached
            .iter()
            .find(|topic| topic.title == MILESTONE_BRANCH)
            .expect("milestone branch");
        let milestone = &branch.children.as_ref().expect("milestones").attached[0];
        assert!(milestone.markers.iter().any(|m| m.marker_id == "flag-red"));
        assert!(milestone.labels.iter().any(|l| l == "2026-09-30"));
        assert!(
            parsed[0]
                .relationships
                .iter()
                .any(|rel| rel.title == "关联")
        );
    }

    #[test]
    fn every_topic_id_is_unique() {
        let parsed = sheets(&payload());
        let counts = collect_topic_ids(&parsed[0]);
        assert!(
            counts.values().all(|count| *count == 1),
            "duplicate ids: {counts:?}"
        );
    }

    #[test]
    fn relationship_endpoints_all_exist() {
        let parsed = sheets(&payload());
        let ids = collect_topic_ids(&parsed[0]);
        for rel in &parsed[0].relationships {
            assert!(ids.contains_key(&rel.end1_id), "dangling end1: {rel:?}");
            assert!(ids.contains_key(&rel.end2_id), "dangling end2: {rel:?}");
        }
    }

    #[test]
    fn export_is_deterministic() {
        let first = payload().content;
        for _ in 0..10 {
            assert_eq!(payload().content, first);
        }
    }

    #[test]
    fn report_lists_mapped_and_degraded_content() {
        let report = payload().report;
        assert!(report.mapped.iter().any(|item| item.kind == "任务"));
        assert!(report.mapped.iter().any(|item| item.kind == "依赖"));
        // Dates, assignee, milestone and derived scheduling all degrade.
        assert!(
            report
                .degraded
                .iter()
                .any(|item| item.original.contains("日期"))
        );
        assert!(
            report
                .degraded
                .iter()
                .any(|item| item.original.contains("负责人"))
        );
        assert!(
            report
                .degraded
                .iter()
                .any(|item| item.element.contains("里程碑"))
        );
        assert!(
            report
                .degraded
                .iter()
                .any(|item| item.original.contains("时间推导"))
        );
    }

    #[test]
    fn metadata_and_manifest_match_the_contract() {
        let payload = payload();
        assert!(payload.metadata.contains("\"dataStructureVersion\":\"2\""));
        assert!(payload.manifest.contains("content.json"));
        assert!(payload.manifest.contains("metadata.json"));
    }

    #[test]
    fn empty_plans_still_produce_a_valid_sheet() {
        let payload = build(&Plan::empty(), "/tmp/empty.xmind");
        let parsed: Vec<Sheet> = serde_json::from_str(&payload.content).expect("parses");
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].root_topic.children.is_none());
    }

    #[test]
    fn degraded_items_group_by_fallback() {
        let payload = payload();
        let grouped = degraded_by_fallback(&payload.report);
        assert!(!grouped.is_empty());
    }
}
