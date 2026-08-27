//! VSDX export contract tests (contracts/export-vsdx.md §契约测试 1–4).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use mcm_core::model::Plan;
use mcm_core::outline::parse;
use mcm_export::report::ExportReport;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("mcm-vsdx-contract-{tag}-{stamp}"));
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

/// Unpacked package, read the way any OPC consumer would.
struct Package {
    parts: BTreeMap<String, String>,
}

impl Package {
    fn part(&self, name: &str) -> &str {
        self.parts
            .get(name)
            .unwrap_or_else(|| panic!("missing part {name}"))
    }
}

fn read_package(path: &Path) -> Package {
    let file = std::fs::File::open(path).expect("open export");
    let mut zip = zip::ZipArchive::new(file).expect("valid zip");
    let mut parts = BTreeMap::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).expect("entry");
        assert!(
            !entry.name().ends_with('/'),
            "directory entry: {}",
            entry.name()
        );
        let mut data = String::new();
        entry.read_to_string(&mut data).expect("utf-8 part");
        parts.insert(entry.name().to_owned(), data);
    }
    Package { parts }
}

fn export_to(scratch: &Scratch, name: &str, plan: &Plan) -> (PathBuf, ExportReport, Package) {
    let path = scratch.file(name);
    let report = mcm_export::vsdx::export(plan, &path).expect("export succeeds");
    let package = read_package(&path);
    (path, report, package)
}

/// Collects `attribute="value"` occurrences for a given element.
fn attributes_of(xml: &str, element: &str, attribute: &str) -> Vec<String> {
    let needle = format!("<{element} ");
    let mut values = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = xml[cursor..].find(&needle) {
        let start = cursor + found + needle.len();
        let rest = &xml[start..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        let attr_needle = format!("{attribute}=\"");
        if let Some(attr_at) = tag.find(&attr_needle) {
            let value_start = attr_at + attr_needle.len();
            if let Some(value_end) = tag[value_start..].find('"') {
                values.push(tag[value_start..value_start + value_end].to_owned());
            }
        }
        cursor = start;
    }
    values
}

/// Reads every numeric `V` value for a named cell, in document order.
fn numeric_cells(xml: &str, name: &str) -> Vec<f64> {
    let needle = format!("<Cell N=\"{name}\" V=\"");
    let mut values = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = xml[cursor..].find(&needle) {
        let start = cursor + found + needle.len();
        let end = xml[start..].find('"').unwrap_or(0) + start;
        if let Ok(value) = xml[start..end].parse::<f64>() {
            values.push(value);
        }
        cursor = end;
    }
    values
}

const SAMPLE: &str = "%mcm 1
%title Visio 契约 🌍

- 需求阶段 #t1 [2026-09-01..2026-09-05] @王芳
- [x] 用户访谈 #t2 <-t1
- 非常长的任务标题用于验证导出时形状文本不会破坏 XML 结构的边界情况 #t3 [3d] <-t2
! 需求冻结 #m1 [2026-09-30] <-t3
";

// ----------------------------------------------------- 1. structure ---

#[test]
fn package_contains_every_required_part() {
    let scratch = Scratch::new("parts");
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &parse(SAMPLE).plan);
    for required in [
        "[Content_Types].xml",
        "_rels/.rels",
        "visio/document.xml",
        "visio/_rels/document.xml.rels",
        "visio/pages/pages.xml",
        "visio/pages/_rels/pages.xml.rels",
        "visio/pages/page1.xml",
    ] {
        assert!(package.parts.contains_key(required), "missing {required}");
    }
}

#[test]
fn content_types_and_parts_correspond_one_to_one() {
    let scratch = Scratch::new("types");
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &parse(SAMPLE).plan);
    let types = package.part("[Content_Types].xml");

    // Every override names a part that exists.
    for name in attributes_of(types, "Override", "PartName") {
        let trimmed = name.trim_start_matches('/');
        assert!(
            package.parts.contains_key(trimmed),
            "override for missing part: {name}"
        );
    }
    // And every visio/docProps XML part has an override.
    for name in package.parts.keys() {
        if name.ends_with(".rels") || name == "[Content_Types].xml" {
            continue;
        }
        assert!(
            types.contains(&format!("/{name}")),
            "no override for {name}"
        );
    }
}

#[test]
fn the_relationship_chain_is_closed() {
    let scratch = Scratch::new("rels");
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &parse(SAMPLE).plan);

    // package → document
    assert!(package.part("_rels/.rels").contains("visio/document.xml"));
    // document → pages
    let document_rels = package.part("visio/_rels/document.xml.rels");
    assert!(document_rels.contains("pages/pages.xml"));
    // pages → page1, and the page's Rel id matches
    let pages_rels = package.part("visio/pages/_rels/pages.xml.rels");
    assert!(pages_rels.contains("page1.xml"));
    let rel_ids = attributes_of(package.part("visio/pages/pages.xml"), "Rel", "r:id");
    let declared = attributes_of(pages_rels, "Relationship", "Id");
    for id in &rel_ids {
        assert!(declared.contains(id), "page Rel {id} not declared");
    }
    // No masters part: every shape is masterless, so third-party renderers can
    // draw it without resolving a stencil reference.
    assert!(
        !package.parts.keys().any(|name| name.contains("masters")),
        "master parts are dead weight now that no shape references one"
    );
}

#[test]
fn shape_ids_are_unique_within_the_page() {
    let scratch = Scratch::new("ids");
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &parse(SAMPLE).plan);
    let ids = attributes_of(package.part("visio/pages/page1.xml"), "Shape", "ID");
    let unique: BTreeSet<&String> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "duplicate Shape IDs silently drop shapes: {ids:?}"
    );
}

#[test]
fn each_dependency_has_exactly_two_connect_rows_with_valid_endpoints() {
    let scratch = Scratch::new("connects");
    let plan = parse(SAMPLE).plan;
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &plan);
    let page = package.part("visio/pages/page1.xml");

    let shape_ids: BTreeSet<String> = attributes_of(page, "Shape", "ID").into_iter().collect();
    let from_sheets = attributes_of(page, "Connect", "FromSheet");
    let to_sheets = attributes_of(page, "Connect", "ToSheet");

    // Dependencies plus milestone links, two rows each.
    let connector_count = plan.dependencies.len()
        + plan
            .milestones
            .iter()
            .map(|m| m.linked_tasks.len())
            .sum::<usize>();
    assert_eq!(from_sheets.len(), connector_count * 2);

    for id in from_sheets.iter().chain(to_sheets.iter()) {
        assert!(
            shape_ids.contains(id),
            "Connect references unknown shape {id}"
        );
    }
}

#[test]
fn glue_rides_on_connect_rows() {
    let scratch = Scratch::new("glue");
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &parse(SAMPLE).plan);
    let page = package.part("visio/pages/page1.xml");

    // FromPart 9/12 = begin/end point, ToPart 3 = whole shape. This is what
    // Visio uses to re-establish glue for an untrusted file, and it is the
    // whole mechanism now that formulas are gone.
    assert!(page.contains("FromCell=\"BeginX\" FromPart=\"9\""));
    assert!(page.contains("FromCell=\"EndX\" FromPart=\"12\""));
    assert!(page.contains("ToCell=\"PinX\" ToPart=\"3\""));
    // 1-D + walking glue is what makes a shape connectable at all.
    assert!(page.contains("N=\"ObjType\" V=\"2\""));
    assert!(page.contains("N=\"GlueType\" V=\"2\""));
}

#[test]
fn geometry_is_readable_without_evaluating_visio_functions() {
    // Regression from a real rendering failure: `_WALKGLUE`/`GUARD` are Visio
    // internals. A third-party renderer evaluates them to zero, so every
    // connector collapsed to a point and only its text label showed up.
    let scratch = Scratch::new("no-formulas");
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &parse(SAMPLE).plan);
    let page = package.part("visio/pages/page1.xml");

    assert!(
        !page.contains("_WALKGLUE"),
        "internal functions break other tools"
    );
    assert!(
        !page.contains("_XFTRIGGER"),
        "internal functions break other tools"
    );
    assert!(!page.contains("GUARD("), "GUARD hides the numeric value");

    for cell in [
        "BeginX", "BeginY", "EndX", "EndY", "Width", "Height", "PinX", "PinY",
    ] {
        let needle = format!("<Cell N=\"{cell}\" V=\"");
        for fragment in page.split(&needle).skip(1) {
            let tag_end = fragment.find("/>").unwrap_or(fragment.len());
            assert!(
                !fragment[..tag_end].contains("F=\""),
                "{cell} must stay a plain number"
            );
        }
    }
}

#[test]
fn every_connector_spans_a_real_distance() {
    // Guards the exact symptom: a connector whose width does not match its
    // endpoints renders as an invisible zero-length line.
    let scratch = Scratch::new("spans");
    let plan = parse(SAMPLE).plan;
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &plan);
    let page = package.part("visio/pages/page1.xml");

    let begin_x = numeric_cells(page, "BeginX");
    let begin_y = numeric_cells(page, "BeginY");
    let end_x = numeric_cells(page, "EndX");
    let end_y = numeric_cells(page, "EndY");

    let connector_count = plan.dependencies.len()
        + plan
            .milestones
            .iter()
            .map(|m| m.linked_tasks.len())
            .sum::<usize>();
    assert_eq!(begin_x.len(), connector_count);

    for index in 0..begin_x.len() {
        let length = (end_x[index] - begin_x[index]).hypot(end_y[index] - begin_y[index]);
        assert!(
            length > 0.1,
            "connector {index} is only {length} in long — it would be invisible"
        );
    }
}

#[test]
fn shapes_do_not_overlap_each_other() {
    // The milestone diamond used to be pinned at a fixed spot and covered the
    // first task box.
    let scratch = Scratch::new("overlap");
    let plan = parse(SAMPLE).plan;
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &plan);
    let page = package.part("visio/pages/page1.xml");

    let shape_count = plan
        .tasks
        .iter()
        .filter(|t| t.parent.is_some() || true)
        .count();
    let pin_x = numeric_cells(page, "PinX");
    let pin_y = numeric_cells(page, "PinY");
    let widths = numeric_cells(page, "Width");
    let heights = numeric_cells(page, "Height");
    // Only the leading entries are 2-D shapes; connectors follow.
    let count = pin_x.len().min(shape_count + plan.milestones.len());

    for a in 0..count {
        for b in (a + 1)..count {
            let overlap_x = (pin_x[a] - pin_x[b]).abs() < (widths[a] + widths[b]) / 2.0;
            let overlap_y = (pin_y[a] - pin_y[b]).abs() < (heights[a] + heights[b]) / 2.0;
            assert!(!(overlap_x && overlap_y), "shapes {a} and {b} overlap");
        }
    }
}

#[test]
fn geometry_rows_are_sequential_and_start_with_a_move() {
    let scratch = Scratch::new("geometry");
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &parse(SAMPLE).plan);
    let page = package.part("visio/pages/page1.xml");

    // Every geometry section's first row is IX="1" and a (Rel)MoveTo.
    for section in page.split("<Section N=\"Geometry\"").skip(1) {
        let first_row = section.find("<Row ").expect("row");
        let row = &section[first_row..first_row + 40.min(section.len() - first_row)];
        assert!(
            row.contains("MoveTo"),
            "geometry must start with MoveTo: {row}"
        );
        assert!(row.contains("IX=\"1\""), "first row must be IX=1: {row}");
    }
}

#[test]
fn no_part_carries_a_bom_and_all_declare_utf8() {
    let scratch = Scratch::new("encoding");
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &parse(SAMPLE).plan);
    for (name, data) in &package.parts {
        assert!(!data.starts_with('\u{feff}'), "{name} has a BOM");
        assert!(data.starts_with("<?xml"), "{name} lacks an XML declaration");
        assert!(data.contains("utf-8"), "{name} does not declare utf-8");
    }
}

#[test]
fn the_desktop_namespace_is_used_everywhere() {
    let scratch = Scratch::new("namespace");
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &parse(SAMPLE).plan);
    for name in [
        "visio/document.xml",
        "visio/pages/pages.xml",
        "visio/pages/page1.xml",
    ] {
        let data = package.part(name);
        assert!(
            data.contains("http://schemas.microsoft.com/office/visio/2012/main"),
            "{name} uses the wrong namespace"
        );
        assert!(
            !data.contains("visio/2011/1/core"),
            "{name} uses the SharePoint namespace"
        );
    }
}

#[test]
fn unicode_and_long_titles_survive_escaped() {
    let scratch = Scratch::new("unicode");
    let plan = parse(SAMPLE).plan;
    let (_, _, package) = export_to(&scratch, "plan.vsdx", &plan);
    let page = package.part("visio/pages/page1.xml");
    assert!(page.contains("需求阶段"));
    assert!(package.part("docProps/core.xml").contains("🌍"));
    let long = plan
        .task(mcm_core::model::TaskId(3))
        .expect("t3")
        .title
        .clone();
    assert!(page.contains(&long), "long titles must not be truncated");
}

#[test]
fn xml_special_characters_are_escaped() {
    let scratch = Scratch::new("escape");
    let plan = parse("%mcm 1\n%title A & B\n\n- 标题 <含> \"引号\" #t1\n").plan;
    let (_, _, package) = export_to(&scratch, "escape.vsdx", &plan);
    let page = package.part("visio/pages/page1.xml");
    assert!(page.contains("&lt;含&gt;"), "{page}");
    assert!(package.part("docProps/core.xml").contains("A &amp; B"));
}

#[test]
fn a_thousand_task_plan_exports_completely() {
    let scratch = Scratch::new("scale");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../mcm-core/fixtures/perf/plan-1000.mcm");
    let source = std::fs::read_to_string(&fixture).expect("perf fixture");
    let plan = parse(&source).plan;

    let (_, report, package) = export_to(&scratch, "big.vsdx", &plan);
    let ids = attributes_of(package.part("visio/pages/page1.xml"), "Shape", "ID");
    let unique: BTreeSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "ids stay unique at scale");
    assert!(report.mapped_total() > 0);
}

// -------------------------------------------------- 2. golden diff ---

#[test]
fn a_fixed_model_produces_stable_output() {
    // Golden-style check: the same model must serialise identically, so a diff
    // against a checked-in fixture would be meaningful.
    let scratch = Scratch::new("golden");
    let plan = parse("%mcm 1\n%title 固定\n\n- 甲 #t1\n- 乙 #t2 <-t1\n").plan;
    let (_, _, first) = export_to(&scratch, "one.vsdx", &plan);
    let (_, _, second) = export_to(&scratch, "two.vsdx", &plan);

    for name in first.parts.keys() {
        assert_eq!(
            first.part(name),
            second.part(name),
            "{name} is not reproducible"
        );
    }
}

// ------------------------------------------ 3. third-party read-back ---

#[test]
fn libvisio_recognises_and_parses_the_export() {
    let scratch = Scratch::new("libvisio");
    let plan = parse(SAMPLE).plan;
    let (path, _, _) = export_to(&scratch, "plan.vsdx", &plan);
    let path_str = path.display().to_string();

    // The independent reader must accept the file as a Visio document.
    assert!(
        libvisio_rs::is_supported(&path_str),
        "libvisio rejects the export"
    );

    match libvisio_rs::get_page_info(&path_str) {
        Ok(pages) => assert_eq!(pages.len(), 1, "exactly one page expected"),
        Err(error) => panic!("libvisio could not read pages: {error}"),
    }
}

#[test]
fn libvisio_extracts_the_task_titles() {
    let scratch = Scratch::new("libvisio-text");
    let plan = parse(SAMPLE).plan;
    let (path, _, _) = export_to(&scratch, "plan.vsdx", &plan);

    let text =
        libvisio_rs::extract_text(&path.display().to_string()).expect("libvisio extracts text");
    for task in &plan.tasks {
        assert!(
            text.contains(&task.title),
            "missing task text: {}",
            task.title
        );
    }
}

// -------------------------------------------------- 4. degradation ---

#[test]
fn every_degraded_category_is_reported() {
    let scratch = Scratch::new("degrade");
    let plan = parse(SAMPLE).plan;
    let (_, report, _) = export_to(&scratch, "plan.vsdx", &plan);

    // Per-task degradations.
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
    assert_eq!(schedule_items, scheduled);

    let assignees = plan
        .tasks
        .iter()
        .filter(|task| task.assignee.is_some())
        .count();
    assert_eq!(
        report
            .degraded
            .iter()
            .filter(|item| item.original.contains("负责人"))
            .count(),
        assignees
    );

    let done = plan.tasks.iter().filter(|task| task.done).count();
    assert_eq!(
        report
            .degraded
            .iter()
            .filter(|item| item.original == "完成状态")
            .count(),
        done
    );

    // Whole-plan degradations named in the contract.
    assert!(
        report
            .degraded
            .iter()
            .any(|item| item.original.contains("甘特"))
    );
    assert!(
        report
            .degraded
            .iter()
            .any(|item| item.original.contains("WBS"))
    );

    // Every entry is actionable.
    for item in &report.degraded {
        assert!(!item.element.is_empty(), "{item:?}");
        assert!(!item.original.is_empty(), "{item:?}");
        assert!(!item.fallback.is_empty(), "{item:?}");
    }
}

#[test]
fn mapped_categories_cover_shapes_and_connectors() {
    let scratch = Scratch::new("mapped");
    let plan = parse(SAMPLE).plan;
    let (_, report, _) = export_to(&scratch, "plan.vsdx", &plan);
    assert!(report.mapped.iter().any(|item| item.kind == "任务"));
    assert!(report.mapped.iter().any(|item| item.kind == "依赖"));
    assert!(report.mapped.iter().any(|item| item.kind == "里程碑"));
}
