//! `visio/pages/page1.xml`: task rectangles, milestone diamonds and glued
//! connectors (contracts/export-vsdx.md 映射表 + §生成规则).

use mcm_core::layout::layout_depgraph;
use mcm_core::model::{MilestoneId, Plan, Schedule, TaskId, format_date};

use crate::report::ExportReport;

use super::masters::CONNECTOR_MASTER_ID;
use super::opc::{NS_MAIN, NS_REL, XML_DECL, escape};

/// Logical layout units per inch (contract §模型 → Visio 映射: 100 单位 = 1 英寸).
pub const UNITS_PER_INCH: f64 = 100.0;
/// Margin around the content, in inches.
pub const MARGIN_IN: f64 = 1.0;

/// Rounds to a stable number of decimals so exports are byte-reproducible.
fn inches(value: f64) -> String {
    format!("{:.4}", value)
}

/// A placed shape, ready to be serialised.
#[derive(Debug, Clone, PartialEq)]
struct Placed {
    id: u32,
    x_in: f64,
    y_in: f64,
    w_in: f64,
    h_in: f64,
    text: String,
    diamond: bool,
    done: bool,
}

/// Result of laying the plan out on a Visio page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageGeometry {
    pub width_in: f64,
    pub height_in: f64,
    /// Shape id assigned to each task.
    pub task_shape_ids: Vec<(TaskId, u32)>,
    pub milestone_shape_ids: Vec<(MilestoneId, u32)>,
    /// `(connector id, from shape, to shape)`.
    pub connectors: Vec<(u32, u32, u32)>,
}

impl PageGeometry {
    #[must_use]
    pub fn shape_for_task(&self, id: TaskId) -> Option<u32> {
        self.task_shape_ids
            .iter()
            .find(|(task, _)| *task == id)
            .map(|(_, shape)| *shape)
    }

    #[must_use]
    pub fn shape_for_milestone(&self, id: MilestoneId) -> Option<u32> {
        self.milestone_shape_ids
            .iter()
            .find(|(milestone, _)| *milestone == id)
            .map(|(_, shape)| *shape)
    }
}

/// Builds `page1.xml` plus the geometry summary used by callers and tests.
#[must_use]
pub fn build_page(plan: &Plan, report: &mut ExportReport) -> (String, PageGeometry) {
    let layout = layout_depgraph(plan);

    // Content bounds in logical units, then converted to inches with a margin.
    let content_w = layout.width.max(1.0) / UNITS_PER_INCH;
    let content_h = layout.height.max(1.0) / UNITS_PER_INCH;
    let page_w = content_w + MARGIN_IN * 2.0;
    let page_h = content_h + MARGIN_IN * 2.0;

    let mut next_id = 1u32;
    let mut placed: Vec<Placed> = Vec::new();
    let mut task_shape_ids: Vec<(TaskId, u32)> = Vec::new();

    for node in &layout.nodes {
        let Some(task) = plan.task(node.id) else {
            continue;
        };
        let w_in = node.w / UNITS_PER_INCH;
        let h_in = node.h / UNITS_PER_INCH;
        let x_in = MARGIN_IN + node.x / UNITS_PER_INCH;
        // Visio's origin is bottom-left, so flip Y.
        let y_in = MARGIN_IN + (content_h - node.y / UNITS_PER_INCH - h_in);

        let mut lines = vec![task.title.clone()];
        let mut detail = vec![format!("#{}", node.id.as_token())];
        match task.schedule {
            Schedule::None => {}
            Schedule::Explicit { start, end } => {
                detail.push(format!("{}..{}", format_date(start), format_date(end)));
                report.degrade(
                    format!("任务 {}", node.id),
                    &format!("日期 {}..{}", format_date(start), format_date(end)),
                    "形状文本行（Visio 无日期语义）",
                );
            }
            Schedule::Duration { days } => {
                detail.push(format!("{days}d"));
                report.degrade(
                    format!("任务 {}", node.id),
                    &format!("工期 {days}d"),
                    "形状文本行（Visio 无工期语义）",
                );
            }
        }
        if let Some(assignee) = &task.assignee {
            detail.push(format!("@{assignee}"));
            report.degrade(
                format!("任务 {}", node.id),
                &format!("负责人 {assignee}"),
                "形状文本行（Visio 无负责人字段）",
            );
        }
        if task.done {
            detail.push("✓".to_owned());
            report.degrade(
                format!("任务 {}", node.id),
                "完成状态",
                "形状文本行 ✓ 与完成填充色",
            );
        }
        lines.push(detail.join(" · "));

        let id = next_id;
        next_id += 1;
        task_shape_ids.push((node.id, id));
        placed.push(Placed {
            id,
            x_in,
            y_in,
            w_in,
            h_in,
            text: lines.join("\n"),
            diamond: false,
            done: task.done,
        });
    }

    // Milestones sit in a row beneath the graph as diamonds.
    let mut milestone_shape_ids: Vec<(MilestoneId, u32)> = Vec::new();
    for (index, milestone) in plan.milestones.iter().enumerate() {
        let id = next_id;
        next_id += 1;
        milestone_shape_ids.push((milestone.id, id));
        placed.push(Placed {
            id,
            x_in: MARGIN_IN + index as f64 * 1.6,
            y_in: MARGIN_IN * 0.25,
            w_in: 1.2,
            h_in: 0.8,
            text: format!("{}\n{}", milestone.name, format_date(milestone.date)),
            diamond: true,
            done: false,
        });
    }

    // Connectors: dependencies first, then milestone links.
    let mut connectors: Vec<(u32, u32, u32)> = Vec::new();
    let mut connector_labels: Vec<(u32, &'static str)> = Vec::new();
    for dep in &plan.dependencies {
        let (Some(from), Some(to)) = (
            task_shape_ids
                .iter()
                .find(|(t, _)| *t == dep.predecessor)
                .map(|(_, s)| *s),
            task_shape_ids
                .iter()
                .find(|(t, _)| *t == dep.successor)
                .map(|(_, s)| *s),
        ) else {
            continue;
        };
        let id = next_id;
        next_id += 1;
        connectors.push((id, from, to));
        connector_labels.push((id, "依赖"));
    }
    for milestone in &plan.milestones {
        let Some(from) = milestone_shape_ids
            .iter()
            .find(|(m, _)| *m == milestone.id)
            .map(|(_, s)| *s)
        else {
            continue;
        };
        for task in &milestone.linked_tasks {
            let Some(to) = task_shape_ids
                .iter()
                .find(|(t, _)| t == task)
                .map(|(_, s)| *s)
            else {
                continue;
            };
            let id = next_id;
            next_id += 1;
            connectors.push((id, from, to));
            connector_labels.push((id, "关联"));
        }
    }

    let geometry = PageGeometry {
        width_in: page_w,
        height_in: page_h,
        task_shape_ids,
        milestone_shape_ids,
        connectors,
    };

    // ------------------------------------------------------------- XML ---
    let mut xml = String::from(XML_DECL);
    xml.push_str(&format!(
        "<PageContents xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\" xml:space=\"preserve\"><Shapes>"
    ));

    for shape in &placed {
        write_shape(&mut xml, shape);
    }
    for (id, from, to) in &geometry.connectors {
        let label = connector_labels
            .iter()
            .find(|(candidate, _)| candidate == id)
            .map(|(_, label)| *label)
            .unwrap_or("依赖");
        let from_shape = placed.iter().find(|shape| shape.id == *from);
        let to_shape = placed.iter().find(|shape| shape.id == *to);
        if let (Some(from_shape), Some(to_shape)) = (from_shape, to_shape) {
            write_connector(&mut xml, *id, from_shape, to_shape, label);
        }
    }
    xml.push_str("</Shapes>");

    // Connects re-establish glue when Visio opens a third-party file.
    if !geometry.connectors.is_empty() {
        xml.push_str("<Connects>");
        for (id, from, to) in &geometry.connectors {
            // FromPart 9 = begin point, 12 = end point; ToPart 3 = whole shape.
            xml.push_str(&format!(
                "<Connect FromSheet=\"{id}\" FromCell=\"BeginX\" FromPart=\"9\" ToSheet=\"{from}\" ToCell=\"PinX\" ToPart=\"3\"/>"
            ));
            xml.push_str(&format!(
                "<Connect FromSheet=\"{id}\" FromCell=\"EndX\" FromPart=\"12\" ToSheet=\"{to}\" ToCell=\"PinX\" ToPart=\"3\"/>"
            ));
        }
        xml.push_str("</Connects>");
    }

    xml.push_str("</PageContents>");

    report.map("任务", geometry.task_shape_ids.len(), "矩形形状（可编辑）");
    report.map(
        "依赖",
        plan.dependencies.len(),
        "动态粘连连接线（拖动跟随）",
    );
    report.map("里程碑", geometry.milestone_shape_ids.len(), "菱形形状");
    report.degrade(
        "整份规划",
        "时间线甘特几何",
        "不导出为时间轴页；日期保留在形状文本行",
    );
    report.degrade(
        "整份规划",
        "WBS 层级结构",
        "依赖网络页平铺分层；层级信息保留在任务标识中",
    );

    (xml, geometry)
}

/// A masterless rectangle (or diamond), exactly how Visio saves one.
fn write_shape(xml: &mut String, shape: &Placed) {
    let pin_x = shape.x_in + shape.w_in / 2.0;
    let pin_y = shape.y_in + shape.h_in / 2.0;

    xml.push_str(&format!(
        "<Shape ID=\"{}\" Type=\"Shape\" LineStyle=\"0\" FillStyle=\"0\" TextStyle=\"0\">",
        shape.id
    ));
    xml.push_str(&format!("<Cell N=\"PinX\" V=\"{}\"/>", inches(pin_x)));
    xml.push_str(&format!("<Cell N=\"PinY\" V=\"{}\"/>", inches(pin_y)));
    xml.push_str(&format!("<Cell N=\"Width\" V=\"{}\"/>", inches(shape.w_in)));
    xml.push_str(&format!(
        "<Cell N=\"Height\" V=\"{}\"/>",
        inches(shape.h_in)
    ));
    xml.push_str(&format!(
        "<Cell N=\"LocPinX\" V=\"{}\" F=\"Width*0.5\"/>",
        inches(shape.w_in / 2.0)
    ));
    xml.push_str(&format!(
        "<Cell N=\"LocPinY\" V=\"{}\" F=\"Height*0.5\"/>",
        inches(shape.h_in / 2.0)
    ));
    xml.push_str(
        "<Cell N=\"Angle\" V=\"0\"/><Cell N=\"FlipX\" V=\"0\"/><Cell N=\"FlipY\" V=\"0\"/>",
    );
    xml.push_str("<Cell N=\"ResizeMode\" V=\"0\"/>");
    if shape.done {
        // Completed tasks get a distinct fill so the state survives visually.
        xml.push_str("<Cell N=\"FillForegnd\" V=\"#DFF0E6\"/>");
    }

    xml.push_str("<Section N=\"Geometry\" IX=\"0\">");
    xml.push_str(
        "<Cell N=\"NoFill\" V=\"0\"/><Cell N=\"NoLine\" V=\"0\"/><Cell N=\"NoShow\" V=\"0\"/>\
<Cell N=\"NoSnap\" V=\"0\"/><Cell N=\"NoQuickDrag\" V=\"0\"/>",
    );
    // Row IX starts at 1 and must be sequential; the first row is a MoveTo.
    let points: [(f64, f64); 5] = if shape.diamond {
        [(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5), (0.5, 0.0)]
    } else {
        [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.0, 0.0)]
    };
    for (index, (x, y)) in points.iter().enumerate() {
        let row_type = if index == 0 { "RelMoveTo" } else { "RelLineTo" };
        xml.push_str(&format!(
            "<Row T=\"{row_type}\" IX=\"{}\"><Cell N=\"X\" V=\"{x}\"/><Cell N=\"Y\" V=\"{y}\"/></Row>",
            index + 1
        ));
    }
    xml.push_str("</Section>");

    // Text runs end with a literal newline, as Visio writes them.
    xml.push_str(&format!("<Text>{}\n</Text>", escape(&shape.text)));
    xml.push_str("</Shape>");
}

/// A Dynamic connector instance with both glue mechanisms in place.
fn write_connector(xml: &mut String, id: u32, from: &Placed, to: &Placed, label: &str) {
    let begin_x = from.x_in + from.w_in;
    let begin_y = from.y_in + from.h_in / 2.0;
    let end_x = to.x_in;
    let end_y = to.y_in + to.h_in / 2.0;
    let width = end_x - begin_x;
    let height = end_y - begin_y;

    xml.push_str(&format!(
        "<Shape ID=\"{id}\" NameU=\"Dynamic connector\" Name=\"Dynamic connector\" Type=\"Shape\" Master=\"{CONNECTOR_MASTER_ID}\">"
    ));
    xml.push_str(&format!(
        "<Cell N=\"PinX\" V=\"{}\" F=\"Inh\"/><Cell N=\"PinY\" V=\"{}\" F=\"Inh\"/>",
        inches(begin_x),
        inches(begin_y)
    ));
    // GUARD keeps width/height derived from the glued endpoints.
    xml.push_str(&format!(
        "<Cell N=\"Width\" V=\"{}\" F=\"GUARD(EndX-BeginX)\"/>",
        inches(width)
    ));
    xml.push_str(&format!(
        "<Cell N=\"Height\" V=\"{}\" F=\"GUARD(EndY-BeginY)\"/>",
        inches(height)
    ));
    // _WALKGLUE makes the endpoints follow the shapes as they move.
    xml.push_str(&format!(
        "<Cell N=\"BeginX\" V=\"{}\" F=\"_WALKGLUE(BegTrigger,EndTrigger,WalkPreference)\"/>",
        inches(begin_x)
    ));
    xml.push_str(&format!(
        "<Cell N=\"BeginY\" V=\"{}\" F=\"_WALKGLUE(BegTrigger,EndTrigger,WalkPreference)\"/>",
        inches(begin_y)
    ));
    xml.push_str(&format!(
        "<Cell N=\"EndX\" V=\"{}\" F=\"_WALKGLUE(EndTrigger,BegTrigger,WalkPreference)\"/>",
        inches(end_x)
    ));
    xml.push_str(&format!(
        "<Cell N=\"EndY\" V=\"{}\" F=\"_WALKGLUE(EndTrigger,BegTrigger,WalkPreference)\"/>",
        inches(end_y)
    ));
    // _XFTRIGGER wires the triggers to the two shapes we are glued to.
    xml.push_str(&format!(
        "<Cell N=\"BegTrigger\" V=\"2\" F=\"_XFTRIGGER(Sheet.{}!EventXFMod)\"/>",
        from.id
    ));
    xml.push_str(&format!(
        "<Cell N=\"EndTrigger\" V=\"2\" F=\"_XFTRIGGER(Sheet.{}!EventXFMod)\"/>",
        to.id
    ));
    xml.push_str("<Cell N=\"ObjType\" V=\"2\"/><Cell N=\"ShapeRouteStyle\" V=\"16\"/>");
    xml.push_str("<Cell N=\"ConFixedCode\" V=\"6\"/>");

    xml.push_str("<Section N=\"Geometry\" IX=\"0\">");
    xml.push_str("<Row T=\"MoveTo\" IX=\"1\"><Cell N=\"X\" V=\"0\"/><Cell N=\"Y\" V=\"0\"/></Row>");
    xml.push_str(&format!(
        "<Row T=\"LineTo\" IX=\"2\"><Cell N=\"X\" V=\"{}\" F=\"Width*1\"/><Cell N=\"Y\" V=\"{}\" F=\"Height*1\"/></Row>",
        inches(width),
        inches(height)
    ));
    xml.push_str("</Section>");
    xml.push_str(&format!("<Text>{}\n</Text>", escape(label)));
    xml.push_str("</Shape>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{ExportFormat, ExportReport};
    use mcm_core::outline::parse;

    const SAMPLE: &str = "%mcm 1\n%title Visio 测试\n\n- 甲 #t1 [2026-09-01..2026-09-02] @王芳\n- [x] 乙 #t2 <-t1\n- 丙 #t3 [3d] <-t2\n! 冻结 #m1 [2026-09-30] <-t3\n";

    fn build(source: &str) -> (String, PageGeometry, ExportReport) {
        let plan = parse(source).plan;
        let mut report = ExportReport::new(ExportFormat::Vsdx, "/tmp/a.vsdx");
        let (xml, geometry) = build_page(&plan, &mut report);
        (xml, geometry, report)
    }

    #[test]
    fn every_task_becomes_a_shape() {
        let (_, geometry, _) = build(SAMPLE);
        assert_eq!(geometry.task_shape_ids.len(), 3);
    }

    #[test]
    fn shape_ids_are_unique_across_tasks_milestones_and_connectors() {
        let (_, geometry, _) = build(SAMPLE);
        let mut ids: Vec<u32> = geometry.task_shape_ids.iter().map(|(_, id)| *id).collect();
        ids.extend(geometry.milestone_shape_ids.iter().map(|(_, id)| *id));
        ids.extend(geometry.connectors.iter().map(|(id, _, _)| *id));
        let unique: std::collections::BTreeSet<u32> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "duplicate shape ids: {ids:?}");
    }

    #[test]
    fn each_dependency_yields_exactly_two_connect_rows() {
        let (xml, geometry, _) = build(SAMPLE);
        let connect_count = xml.matches("<Connect ").count();
        assert_eq!(connect_count, geometry.connectors.len() * 2);
    }

    #[test]
    fn connects_use_dynamic_glue_parts() {
        let (xml, _, _) = build(SAMPLE);
        assert!(xml.contains("FromCell=\"BeginX\" FromPart=\"9\""), "{xml}");
        assert!(xml.contains("FromCell=\"EndX\" FromPart=\"12\""), "{xml}");
        assert!(xml.contains("ToCell=\"PinX\" ToPart=\"3\""), "{xml}");
    }

    #[test]
    fn connectors_carry_walkglue_and_xftrigger_formulas() {
        let (xml, _, _) = build(SAMPLE);
        assert!(
            xml.contains("_WALKGLUE(BegTrigger,EndTrigger,WalkPreference)"),
            "{xml}"
        );
        assert!(xml.contains("_XFTRIGGER(Sheet."), "{xml}");
        assert!(xml.contains("GUARD(EndX-BeginX)"), "{xml}");
    }

    #[test]
    fn connector_triggers_reference_real_shape_ids() {
        let (xml, geometry, _) = build(SAMPLE);
        for (_, from, to) in &geometry.connectors {
            assert!(
                xml.contains(&format!("_XFTRIGGER(Sheet.{from}!EventXFMod)")),
                "{from}"
            );
            assert!(
                xml.contains(&format!("_XFTRIGGER(Sheet.{to}!EventXFMod)")),
                "{to}"
            );
        }
    }

    #[test]
    fn rectangles_use_inline_geometry_without_a_master() {
        let (xml, _, _) = build(SAMPLE);
        // The first task shape has no Master attribute.
        let shape_start = xml.find("<Shape ID=\"1\"").expect("shape 1");
        let shape_end = xml[shape_start..].find("</Shape>").expect("close") + shape_start;
        let shape = &xml[shape_start..shape_end];
        assert!(
            !shape.contains("Master="),
            "task rectangles are masterless: {shape}"
        );
        assert!(shape.contains("T=\"RelMoveTo\" IX=\"1\""), "{shape}");
        assert_eq!(shape.matches("T=\"RelLineTo\"").count(), 4);
    }

    #[test]
    fn geometry_rows_are_sequential_from_one() {
        let (xml, _, _) = build(SAMPLE);
        for index in 1..=5 {
            assert!(
                xml.contains(&format!("IX=\"{index}\"")),
                "missing row {index}"
            );
        }
    }

    #[test]
    fn connectors_instance_the_dynamic_connector_master() {
        let (xml, _, _) = build(SAMPLE);
        assert!(
            xml.contains(&format!("Master=\"{CONNECTOR_MASTER_ID}\"")),
            "{xml}"
        );
    }

    #[test]
    fn milestones_become_diamonds() {
        let (xml, geometry, _) = build(SAMPLE);
        assert_eq!(geometry.milestone_shape_ids.len(), 1);
        // The diamond starts at the mid-point of the top edge.
        assert!(xml.contains("<Cell N=\"X\" V=\"0.5\"/>"), "{xml}");
    }

    #[test]
    fn shape_text_carries_title_and_detail_line() {
        let (xml, _, _) = build(SAMPLE);
        assert!(
            xml.contains("甲\n#t1 · 2026-09-01..2026-09-02 · @王芳\n"),
            "{xml}"
        );
    }

    #[test]
    fn done_tasks_get_a_check_and_fill() {
        let (xml, _, _) = build(SAMPLE);
        assert!(xml.contains('✓'), "{xml}");
        assert!(xml.contains("FillForegnd"), "{xml}");
    }

    #[test]
    fn coordinates_are_in_inches_with_a_margin() {
        let (_, geometry, _) = build(SAMPLE);
        assert!(geometry.width_in > MARGIN_IN * 2.0);
        assert!(geometry.height_in > MARGIN_IN * 2.0);
    }

    #[test]
    fn report_lists_mapped_and_degraded_content() {
        let (_, _, report) = build(SAMPLE);
        assert!(report.mapped.iter().any(|item| item.kind == "任务"));
        assert!(report.mapped.iter().any(|item| item.kind == "依赖"));
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
                .any(|item| item.original.contains("WBS"))
        );
        assert!(
            report
                .degraded
                .iter()
                .any(|item| item.original.contains("甘特"))
        );
    }

    #[test]
    fn output_is_deterministic() {
        let first = build(SAMPLE).0;
        for _ in 0..10 {
            assert_eq!(build(SAMPLE).0, first);
        }
    }

    #[test]
    fn empty_plans_produce_a_valid_empty_page() {
        let (xml, geometry, _) = build("%mcm 1\n%title 空\n");
        assert!(xml.contains("<Shapes></Shapes>"), "{xml}");
        assert!(geometry.connectors.is_empty());
        assert!(!xml.contains("<Connects>"), "no empty Connects block");
    }
}
