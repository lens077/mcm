//! `visio/pages/page1.xml`: task rectangles, milestone diamonds and glued
//! connectors (contracts/export-vsdx.md 映射表 + §生成规则).
//!
//! Coordinates are written as **plain numbers**, never as formulas. Visio's own
//! files use `_WALKGLUE`/`GUARD` formulas, but those are internal functions: a
//! third-party renderer evaluates them to zero and the connector collapses to a
//! point. Glue is therefore carried by the `<Connects>` rows alone, which is
//! exactly how Visio re-establishes glue for untrusted files
//! (research-vsdx.md §4).

use mcm_core::layout::layout_depgraph;
use mcm_core::model::{MilestoneId, Plan, Schedule, TaskId, format_date};

use crate::report::ExportReport;

use super::opc::{NS_MAIN, NS_REL, XML_DECL, escape};

/// Body text size in inches (14pt). Visio inherits a smaller default that most
/// renderers draw too small at page scale.
const TEXT_SIZE_IN: f64 = 0.194;
/// Connector labels sit on the line, so they stay smaller.
const LABEL_SIZE_IN: f64 = 0.14;

/// Character + Paragraph sections: explicit size and centring.
fn text_style(size_in: f64) -> String {
    format!(
        "<Section N=\"Character\"><Row IX=\"0\"><Cell N=\"Size\" V=\"{size_in:.4}\"/>\
<Cell N=\"Color\" V=\"#18211d\"/></Row></Section>\
<Section N=\"Paragraph\"><Row IX=\"0\"><Cell N=\"HorzAlign\" V=\"1\"/></Row></Section>"
    )
}

/// Horizontal layout units per inch. Ranks are far apart, so the scale is
/// coarser than the vertical one.
pub const X_UNITS_PER_INCH: f64 = 120.0;
/// Vertical layout units per inch.
pub const Y_UNITS_PER_INCH: f64 = 55.0;
/// Drawn task box size, chosen for two readable text lines.
pub const SHAPE_W_IN: f64 = 2.0;
pub const SHAPE_H_IN: f64 = 0.8;
/// Milestone diamond size.
pub const DIAMOND_W_IN: f64 = 1.9;
pub const DIAMOND_H_IN: f64 = 1.1;
/// Margin around the content, in inches.
pub const MARGIN_IN: f64 = 0.75;
/// Gap between the task graph and the milestone band.
const MILESTONE_GAP_IN: f64 = 0.9;

/// Shortens a date range the way the contract's example does
/// (`2026-09-01..05`), so a task's detail line fits inside its box.
fn compact_range(start: mcm_core::model::Date, end: mcm_core::model::Date) -> String {
    use chrono::Datelike;
    let full_start = format_date(start);
    if start.year() != end.year() {
        return format!("{full_start}..{}", format_date(end));
    }
    if start.month() == end.month() {
        return format!("{full_start}..{:02}", end.day());
    }
    format!("{full_start}..{:02}-{:02}", end.month(), end.day())
}

/// Rounds to a stable number of decimals so exports are byte-reproducible.
fn inches(value: f64) -> String {
    format!("{value:.4}")
}

/// A placed shape in top-down inches; Y is flipped once at the end.
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

impl Placed {
    fn centre(&self) -> (f64, f64) {
        (self.x_in + self.w_in / 2.0, self.y_in + self.h_in / 2.0)
    }
}

/// Where the segment from `from`'s centre toward `to`'s centre leaves `from`'s
/// border. Keeps connectors touching the box edge from any direction.
fn border_point(from: &Placed, to: &Placed) -> (f64, f64) {
    let (cx, cy) = from.centre();
    let (tx, ty) = to.centre();
    let dx = tx - cx;
    let dy = ty - cy;
    if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
        return (cx, cy);
    }
    let scale_x = if dx.abs() > 1e-9 {
        (from.w_in / 2.0) / dx.abs()
    } else {
        f64::INFINITY
    };
    let scale_y = if dy.abs() > 1e-9 {
        (from.h_in / 2.0) / dy.abs()
    } else {
        f64::INFINITY
    };
    let scale = scale_x.min(scale_y);
    (cx + dx * scale, cy + dy * scale)
}

/// Result of laying the plan out on a Visio page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageGeometry {
    pub width_in: f64,
    pub height_in: f64,
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

    let mut next_id = 1u32;
    let mut placed: Vec<Placed> = Vec::new();
    let mut task_shape_ids: Vec<(TaskId, u32)> = Vec::new();

    // --- tasks, in top-down inches -------------------------------------
    for node in &layout.nodes {
        let Some(task) = plan.task(node.id) else {
            continue;
        };
        // The layout supplies topology; the exporter picks the drawn size.
        let x_in = node.x / X_UNITS_PER_INCH;
        let y_in = node.y / Y_UNITS_PER_INCH;

        let mut detail = vec![format!("#{}", node.id.as_token())];
        match task.schedule {
            Schedule::None => {}
            Schedule::Explicit { start, end } => {
                detail.push(compact_range(start, end));
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

        let id = next_id;
        next_id += 1;
        task_shape_ids.push((node.id, id));
        placed.push(Placed {
            id,
            x_in,
            y_in,
            w_in: SHAPE_W_IN,
            h_in: SHAPE_H_IN,
            text: format!("{}\n{}", task.title, detail.join(" · ")),
            diamond: false,
            done: task.done,
        });
    }

    // --- milestones sit below the graph, aligned to what they gate ------
    let tasks_bottom = placed
        .iter()
        .map(|shape| shape.y_in + shape.h_in)
        .fold(0.0_f64, f64::max);
    let milestone_row_y = tasks_bottom + MILESTONE_GAP_IN;

    let mut milestone_shape_ids: Vec<(MilestoneId, u32)> = Vec::new();
    for (index, milestone) in plan.milestones.iter().enumerate() {
        // Centre the diamond under its linked tasks so links stay short.
        let linked_centres: Vec<f64> = milestone
            .linked_tasks
            .iter()
            .filter_map(|task| task_shape_ids.iter().find(|(t, _)| t == task))
            .filter_map(|(_, shape)| placed.iter().find(|candidate| candidate.id == *shape))
            .map(|shape| shape.centre().0)
            .collect();
        let centre_x = if linked_centres.is_empty() {
            index as f64 * (DIAMOND_W_IN + 0.6) + DIAMOND_W_IN / 2.0
        } else {
            linked_centres.iter().sum::<f64>() / linked_centres.len() as f64
        };

        let id = next_id;
        next_id += 1;
        milestone_shape_ids.push((milestone.id, id));
        placed.push(Placed {
            id,
            x_in: centre_x - DIAMOND_W_IN / 2.0,
            y_in: milestone_row_y,
            w_in: DIAMOND_W_IN,
            h_in: DIAMOND_H_IN,
            text: format!("{}\n{}", milestone.name, format_date(milestone.date)),
            diamond: true,
            done: false,
        });
    }

    // --- normalise into page coordinates (Visio's origin is bottom-left) ---
    let min_x = placed.iter().map(|s| s.x_in).fold(f64::INFINITY, f64::min);
    let min_y = placed.iter().map(|s| s.y_in).fold(f64::INFINITY, f64::min);
    let raw_w = placed
        .iter()
        .map(|s| s.x_in + s.w_in)
        .fold(f64::NEG_INFINITY, f64::max)
        - min_x;
    let raw_h = placed
        .iter()
        .map(|s| s.y_in + s.h_in)
        .fold(f64::NEG_INFINITY, f64::max)
        - min_y;
    let (content_w, content_h) = if placed.is_empty() {
        (1.0, 1.0)
    } else {
        (raw_w.max(1.0), raw_h.max(1.0))
    };

    for shape in &mut placed {
        let top_down_y = shape.y_in - min_y;
        shape.x_in = MARGIN_IN + (shape.x_in - min_x);
        shape.y_in = MARGIN_IN + (content_h - top_down_y - shape.h_in);
    }

    let page_w = content_w + MARGIN_IN * 2.0;
    let page_h = content_h + MARGIN_IN * 2.0;

    // --- connectors -----------------------------------------------------
    let mut connectors: Vec<(u32, u32, u32)> = Vec::new();
    let mut labels: Vec<(u32, &'static str)> = Vec::new();
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
        labels.push((id, "依赖"));
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
            labels.push((id, "关联"));
        }
    }

    let geometry = PageGeometry {
        width_in: page_w,
        height_in: page_h,
        task_shape_ids,
        milestone_shape_ids,
        connectors,
    };

    // --- XML ------------------------------------------------------------
    let mut xml = String::from(XML_DECL);
    xml.push_str(&format!(
        "<PageContents xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\" xml:space=\"preserve\"><Shapes>"
    ));
    for shape in &placed {
        write_shape(&mut xml, shape);
    }
    for (id, from, to) in &geometry.connectors {
        let label = labels
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

    // Connects are what actually re-establishes glue when Visio opens a
    // third-party file, so they carry the whole contract on their own.
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
    let (pin_x, pin_y) = shape.centre();

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
    xml.push_str("<Cell N=\"LineWeight\" V=\"0.0139\"/><Cell N=\"LineColor\" V=\"#3d4942\"/>");
    if shape.diamond {
        xml.push_str("<Cell N=\"FillForegnd\" V=\"#fde4dd\"/>");
    } else if shape.done {
        xml.push_str("<Cell N=\"FillForegnd\" V=\"#dff0e6\"/>");
    } else {
        xml.push_str("<Cell N=\"FillForegnd\" V=\"#ffffff\"/>");
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

    xml.push_str(&text_style(TEXT_SIZE_IN));
    // Text runs end with a literal newline, as Visio writes them.
    xml.push_str(&format!("<Text>{}\n</Text>", escape(&shape.text)));
    xml.push_str("</Shape>");
}

/// Smallest bounding-box side for a connector, so a purely horizontal or
/// vertical line still has a non-degenerate box for renderers that clip to it.
const MIN_SPAN_IN: f64 = 0.04;

/// A connector, written to mirror the task rectangles as closely as possible.
///
/// Rectangles render everywhere; connectors did not, so they now share the same
/// recipe: **masterless**, a non-degenerate bounding box, and `RelMoveTo`/
/// `RelLineTo` rows. `BeginX`/`EndX` stay authoritative for Visio and `ObjType`
/// marks the shape one-dimensional, while glue itself rides on `<Connects>`.
fn write_connector(xml: &mut String, id: u32, from: &Placed, to: &Placed, label: &str) {
    let (begin_x, begin_y) = border_point(from, to);
    let (end_x, end_y) = border_point(to, from);

    // Axis-aligned bounding box of the segment, never zero-sized.
    let x0 = begin_x.min(end_x);
    let y0 = begin_y.min(end_y);
    let raw_w = (end_x - begin_x).abs();
    let raw_h = (end_y - begin_y).abs();
    let w = raw_w.max(MIN_SPAN_IN);
    let h = raw_h.max(MIN_SPAN_IN);

    // Endpoints as fractions of that box; a flat axis collapses to the middle.
    let rel = |value: f64, origin: f64, span: f64, raw: f64| -> f64 {
        if raw < MIN_SPAN_IN {
            0.5
        } else {
            (value - origin) / span
        }
    };
    let begin_rx = rel(begin_x, x0, w, raw_w);
    let begin_ry = rel(begin_y, y0, h, raw_h);
    let end_rx = rel(end_x, x0, w, raw_w);
    let end_ry = rel(end_y, y0, h, raw_h);

    xml.push_str(&format!(
        "<Shape ID=\"{id}\" NameU=\"Dynamic connector\" Name=\"Dynamic connector\" Type=\"Shape\" LineStyle=\"0\" FillStyle=\"0\" TextStyle=\"0\">"
    ));
    xml.push_str(&format!(
        "<Cell N=\"PinX\" V=\"{}\"/>",
        inches(x0 + w / 2.0)
    ));
    xml.push_str(&format!(
        "<Cell N=\"PinY\" V=\"{}\"/>",
        inches(y0 + h / 2.0)
    ));
    xml.push_str(&format!("<Cell N=\"Width\" V=\"{}\"/>", inches(w)));
    xml.push_str(&format!("<Cell N=\"Height\" V=\"{}\"/>", inches(h)));
    xml.push_str(&format!("<Cell N=\"LocPinX\" V=\"{}\"/>", inches(w / 2.0)));
    xml.push_str(&format!("<Cell N=\"LocPinY\" V=\"{}\"/>", inches(h / 2.0)));
    xml.push_str(
        "<Cell N=\"Angle\" V=\"0\"/><Cell N=\"FlipX\" V=\"0\"/><Cell N=\"FlipY\" V=\"0\"/>",
    );

    // Endpoints Visio treats as authoritative for a 1-D shape.
    xml.push_str(&format!("<Cell N=\"BeginX\" V=\"{}\"/>", inches(begin_x)));
    xml.push_str(&format!("<Cell N=\"BeginY\" V=\"{}\"/>", inches(begin_y)));
    xml.push_str(&format!("<Cell N=\"EndX\" V=\"{}\"/>", inches(end_x)));
    xml.push_str(&format!("<Cell N=\"EndY\" V=\"{}\"/>", inches(end_y)));

    xml.push_str("<Cell N=\"ObjType\" V=\"2\"/><Cell N=\"GlueType\" V=\"2\"/>");
    xml.push_str("<Cell N=\"ShapeRouteStyle\" V=\"16\"/><Cell N=\"ConFixedCode\" V=\"6\"/>");
    xml.push_str("<Cell N=\"LineWeight\" V=\"0.0208\"/><Cell N=\"LineColor\" V=\"#d84f35\"/>");
    xml.push_str("<Cell N=\"LinePattern\" V=\"1\"/><Cell N=\"EndArrow\" V=\"4\"/>");

    xml.push_str("<Section N=\"Geometry\" IX=\"0\">");
    xml.push_str(
        "<Cell N=\"NoFill\" V=\"1\"/><Cell N=\"NoLine\" V=\"0\"/><Cell N=\"NoShow\" V=\"0\"/>\
<Cell N=\"NoSnap\" V=\"0\"/><Cell N=\"NoQuickDrag\" V=\"0\"/>",
    );
    xml.push_str(&format!(
        "<Row T=\"RelMoveTo\" IX=\"1\"><Cell N=\"X\" V=\"{begin_rx:.4}\"/><Cell N=\"Y\" V=\"{begin_ry:.4}\"/></Row>"
    ));
    xml.push_str(&format!(
        "<Row T=\"RelLineTo\" IX=\"2\"><Cell N=\"X\" V=\"{end_rx:.4}\"/><Cell N=\"Y\" V=\"{end_ry:.4}\"/></Row>"
    ));
    xml.push_str("</Section>");
    xml.push_str(&text_style(LABEL_SIZE_IN));
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

    /// Reads every `V` value for a named cell, in document order.
    fn cell_values(xml: &str, name: &str) -> Vec<f64> {
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

    // ------------------------------------------------- rendering safety ---

    #[test]
    fn no_positional_cell_carries_a_formula() {
        // Regression: `_WALKGLUE`/`GUARD` formulas made third-party renderers
        // evaluate the width to zero, collapsing every connector to a point.
        let (xml, _, _) = build(SAMPLE);
        for cell in [
            "PinX", "PinY", "Width", "Height", "BeginX", "BeginY", "EndX", "EndY",
        ] {
            let needle = format!("<Cell N=\"{cell}\" V=\"");
            for fragment in xml.split(&needle).skip(1) {
                let tag_end = fragment.find("/>").unwrap_or(fragment.len());
                let tag = &fragment[..tag_end];
                assert!(
                    !tag.contains("F=\""),
                    "{cell} must be a plain number for third-party renderers: {tag}"
                );
            }
        }
        assert!(
            !xml.contains("_WALKGLUE"),
            "internal Visio functions break other tools"
        );
        assert!(!xml.contains("GUARD("), "GUARD hides the numeric value");
    }

    #[test]
    fn connector_boxes_match_their_endpoints() {
        // The original failure was a bounding box that did not correspond to
        // Begin/End, so the line had nothing to draw inside.
        let (xml, geometry, _) = build(SAMPLE);
        let begin_x = cell_values(&xml, "BeginX");
        let begin_y = cell_values(&xml, "BeginY");
        let end_x = cell_values(&xml, "EndX");
        let end_y = cell_values(&xml, "EndY");
        assert_eq!(begin_x.len(), geometry.connectors.len());

        let count = geometry.connectors.len();
        let widths = cell_values(&xml, "Width");
        let heights = cell_values(&xml, "Height");
        let pin_x = cell_values(&xml, "PinX");
        let pin_y = cell_values(&xml, "PinY");
        let (widths, heights) = (
            &widths[widths.len() - count..],
            &heights[heights.len() - count..],
        );
        let (pin_x, pin_y) = (&pin_x[pin_x.len() - count..], &pin_y[pin_y.len() - count..]);

        for index in 0..count {
            let span = (end_x[index] - begin_x[index]).hypot(end_y[index] - begin_y[index]);
            assert!(span > 0.1, "connector {index} is degenerate: {span}");

            // The box spans the segment on both axes, never zero-sized.
            let expected_w = (end_x[index] - begin_x[index]).abs().max(MIN_SPAN_IN);
            let expected_h = (end_y[index] - begin_y[index]).abs().max(MIN_SPAN_IN);
            assert!(
                (widths[index] - expected_w).abs() < 0.01,
                "connector {index} width"
            );
            assert!(
                (heights[index] - expected_h).abs() < 0.01,
                "connector {index} height"
            );
            assert!(
                widths[index] > 0.0 && heights[index] > 0.0,
                "degenerate box"
            );

            // And the box is centred on the segment. A flat axis is padded to
            // MIN_SPAN_IN, so allow half of that as slack.
            let slack = MIN_SPAN_IN / 2.0 + 0.001;
            let mid_x = (begin_x[index] + end_x[index]) / 2.0;
            let mid_y = (begin_y[index] + end_y[index]) / 2.0;
            assert!(
                (pin_x[index] - mid_x).abs() <= slack,
                "connector {index} pin x"
            );
            assert!(
                (pin_y[index] - mid_y).abs() <= slack,
                "connector {index} pin y"
            );
        }
    }

    #[test]
    fn connector_midpoints_stay_between_their_shapes() {
        let (xml, geometry, _) = build(SAMPLE);
        let pin_x = cell_values(&xml, "PinX");
        let shape_count = geometry.task_shape_ids.len() + geometry.milestone_shape_ids.len();
        let shape_pins = &pin_x[..shape_count];
        let min = shape_pins.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = shape_pins.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        for pin in &pin_x[shape_count..] {
            assert!(
                *pin >= min - 2.0 && *pin <= max + 2.0,
                "connector pin {pin} is off-page"
            );
        }
    }

    #[test]
    fn shapes_never_overlap() {
        // The milestone diamond used to be pinned to a fixed spot and landed on
        // top of the first task.
        let (xml, geometry, _) = build(SAMPLE);
        let pin_x = cell_values(&xml, "PinX");
        let pin_y = cell_values(&xml, "PinY");
        let widths = cell_values(&xml, "Width");
        let heights = cell_values(&xml, "Height");
        let count = geometry.task_shape_ids.len() + geometry.milestone_shape_ids.len();

        for a in 0..count {
            for b in (a + 1)..count {
                let dx = (pin_x[a] - pin_x[b]).abs();
                let dy = (pin_y[a] - pin_y[b]).abs();
                let overlap_x = dx < (widths[a] + widths[b]) / 2.0;
                let overlap_y = dy < (heights[a] + heights[b]) / 2.0;
                assert!(!(overlap_x && overlap_y), "shapes {a} and {b} overlap");
            }
        }
    }

    // 2.4in x 0.48in rendered as an unreadable sliver, so the box proportions
    // are checked at compile time.
    const _: () = assert!(SHAPE_H_IN >= 0.6, "two text lines need vertical room");
    const _: () = assert!(
        SHAPE_W_IN < SHAPE_H_IN * 3.0,
        "boxes must not be extreme letterboxes"
    );

    #[test]
    fn milestones_sit_below_the_task_graph() {
        let (xml, geometry, _) = build(SAMPLE);
        let pin_y = cell_values(&xml, "PinY");
        let task_count = geometry.task_shape_ids.len();
        let lowest_task = pin_y[..task_count]
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        // Visio's Y grows upward, so "below" means a smaller value.
        for milestone_pin in &pin_y[task_count..task_count + geometry.milestone_shape_ids.len()] {
            assert!(
                *milestone_pin < lowest_task,
                "milestone must sit under the graph"
            );
        }
    }

    // ---------------------------------------------------------- glue ---

    #[test]
    fn each_dependency_yields_exactly_two_connect_rows() {
        let (xml, geometry, _) = build(SAMPLE);
        assert_eq!(
            xml.matches("<Connect ").count(),
            geometry.connectors.len() * 2
        );
    }

    #[test]
    fn connects_use_dynamic_glue_parts() {
        let (xml, _, _) = build(SAMPLE);
        assert!(xml.contains("FromCell=\"BeginX\" FromPart=\"9\""), "{xml}");
        assert!(xml.contains("FromCell=\"EndX\" FromPart=\"12\""), "{xml}");
        assert!(xml.contains("ToCell=\"PinX\" ToPart=\"3\""), "{xml}");
    }

    #[test]
    fn connectors_declare_themselves_one_dimensional() {
        let (xml, _, _) = build(SAMPLE);
        // ObjType 2 + GlueType 2 is what makes Visio treat them as glueable.
        assert!(xml.contains("N=\"ObjType\" V=\"2\""), "{xml}");
        assert!(xml.contains("N=\"GlueType\" V=\"2\""), "{xml}");
    }

    #[test]
    fn connectors_are_masterless_like_the_rectangles() {
        // A `Master` reference made third-party renderers drop the connector
        // geometry entirely; only its text label survived. Rectangles always
        // rendered because they are masterless, so connectors match them now.
        let (xml, _, _) = build(SAMPLE);
        assert!(
            !xml.contains("Master="),
            "no shape may depend on a master: {xml}"
        );
    }

    // -------------------------------------------------------- shapes ---

    #[test]
    fn rectangles_use_inline_geometry_without_a_master() {
        let (xml, _, _) = build(SAMPLE);
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
    fn milestones_become_diamonds() {
        let (xml, geometry, _) = build(SAMPLE);
        assert_eq!(geometry.milestone_shape_ids.len(), 1);
        assert!(xml.contains("<Cell N=\"X\" V=\"0.5\"/>"), "{xml}");
    }

    #[test]
    fn shape_text_carries_title_and_detail_line() {
        let (xml, _, _) = build(SAMPLE);
        assert!(xml.contains("甲\n#t1 · 2026-09-01..02 · @王芳\n"), "{xml}");
    }

    #[test]
    fn date_ranges_are_shortened_so_the_line_fits() {
        use mcm_core::model::parse_date;
        let d = |text: &str| parse_date(text).expect("date");
        // Same month: only the closing day is kept.
        assert_eq!(
            compact_range(d("2026-09-01"), d("2026-09-05")),
            "2026-09-01..05"
        );
        // Same year: month and day.
        assert_eq!(
            compact_range(d("2026-09-01"), d("2026-10-05")),
            "2026-09-01..10-05"
        );
        // Across years: nothing can be dropped.
        assert_eq!(
            compact_range(d("2026-12-20"), d("2027-01-05")),
            "2026-12-20..2027-01-05"
        );
    }

    #[test]
    fn done_tasks_get_a_check_and_fill() {
        let (xml, _, _) = build(SAMPLE);
        assert!(xml.contains('✓'), "{xml}");
        assert!(xml.contains("#dff0e6"), "{xml}");
    }

    #[test]
    fn page_is_large_enough_for_its_content() {
        let (xml, geometry, _) = build(SAMPLE);
        for value in cell_values(&xml, "PinX") {
            assert!(
                value >= 0.0 && value <= geometry.width_in,
                "x {value} off-page"
            );
        }
        for value in cell_values(&xml, "PinY") {
            assert!(
                value >= 0.0 && value <= geometry.height_in,
                "y {value} off-page"
            );
        }
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
