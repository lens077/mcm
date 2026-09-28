//! HTML import for archify diagrams.
//!
//! archify renders every component and relationship with semantic
//! `data-*` attributes (`data-node-id`, `data-edge-from`, frame labels), so an
//! HTML file can be read exactly — no OCR, no guessing. Other HTML is refused
//! with a clear message rather than half-imported.

use std::collections::{BTreeMap, HashSet};

use dom_query::{Document, NodeRef};

use crate::ImportError;
use crate::diagram::{self, Diagram, Edge, Group, Node};
use crate::raster::Rect;

/// Separator archify uses in `data-node-context` for nested lanes/frames.
const CONTEXT_SEPARATOR: &str = " › ";

/// Parse an archify HTML document into a [`Diagram`] plus its title.
///
/// # Errors
/// [`ImportError::Unsupported`] when the document carries no archify markup.
pub fn parse(html: &str) -> Result<(Diagram, String), ImportError> {
    let doc = Document::from(html);

    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    let mut nodes = Vec::new();
    let mut contexts = Vec::new();
    for g in doc.select("g[data-node-id]").nodes() {
        let Some(id) = attr(g, "data-node-id") else {
            continue;
        };
        if ids.contains_key(&id) {
            continue;
        }
        let title = attr(g, "data-node-label").unwrap_or_else(|| id.clone());
        let detail: Vec<String> = [attr(g, "data-node-sublabel"), attr(g, "data-node-tag")]
            .into_iter()
            .flatten()
            .collect();
        ids.insert(id, nodes.len());
        nodes.push(Node {
            title,
            detail,
            rect: node_rect(g, nodes.len()),
        });
        contexts.push(attr(g, "data-node-context").unwrap_or_default());
    }
    if nodes.is_empty() {
        return Err(ImportError::Unsupported(
            "没有找到 archify 图的节点标注（data-node-id）。目前只支持 archify 生成的 HTML；其他图请截图后用「导入图片」".into(),
        ));
    }

    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for e in doc.select("[data-edge-from][data-edge-to]").nodes() {
        let (Some(from), Some(to)) = (attr(e, "data-edge-from"), attr(e, "data-edge-to")) else {
            continue;
        };
        // The same relationship is drawn several times (path, label, motion).
        let key = attr(e, "data-edge-id")
            .or_else(|| attr(e, "data-edge-key"))
            .unwrap_or_default();
        if !seen.insert((from.clone(), to.clone(), key)) {
            continue;
        }
        let (Some(&from), Some(&to)) = (ids.get(&from), ids.get(&to)) else {
            continue;
        };
        edges.push(Edge {
            from,
            to,
            undirected: false,
            label: attr(e, "data-edge-label"),
        });
    }

    // Only labelled frames are semantic boundaries; unlabelled ones are
    // layout bands (workflow lanes, sequence segments) whose names, if any,
    // live in the node context instead.
    let frames: Vec<(Rect, String)> = doc
        .select(r#"rect[data-graph-role="structural-frame"]"#)
        .nodes()
        .iter()
        .filter_map(|r| Some((rect_of(r)?, attr(r, "data-composition-frame-label")?)))
        .collect();
    let mut groups = if frames.is_empty() {
        groups_from_contexts(&nodes, &contexts)
    } else {
        let node_rects: Vec<Rect> = nodes.iter().map(|n| n.rect).collect();
        let frame_rects: Vec<Rect> = frames.iter().map(|f| f.0).collect();
        let mut groups = diagram::nest(&node_rects, &frame_rects);
        for (g, (_, label)) in groups.iter_mut().zip(&frames) {
            g.title.clone_from(label);
        }
        groups
    };
    diagram::name_unnamed(&mut groups);

    let title = doc
        .select("title")
        .nodes()
        .first()
        .map(|t| t.text().trim().to_owned())
        .unwrap_or_default();
    Ok((
        Diagram {
            nodes,
            groups,
            edges,
            loose_text: Vec::new(),
        },
        title,
    ))
}

fn attr(node: &NodeRef<'_>, name: &str) -> Option<String> {
    node.attr(name)
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn number(node: &NodeRef<'_>, name: &str) -> Option<f64> {
    attr(node, name)?.parse().ok()
}

fn rect_of(node: &NodeRef<'_>) -> Option<Rect> {
    let (x, y) = (number(node, "x")?, number(node, "y")?);
    let (w, h) = (number(node, "width")?, number(node, "height")?);
    #[allow(clippy::cast_possible_truncation)]
    Some(Rect::new(
        x.round() as i32,
        y.round() as i32,
        (x + w).round() as i32,
        (y + h).round() as i32,
    ))
}

/// A node's box: its first `<rect>`, else its first `<text>` anchor, else a
/// slot in document order so reading order still follows the source.
fn node_rect(g: &NodeRef<'_>, index: usize) -> Rect {
    let direct = |sel: &str| g.children().into_iter().find(|c| c.is(sel));
    if let Some(r) = direct("rect").as_ref().and_then(rect_of) {
        return r;
    }
    if let Some(t) = direct("text")
        && let (Some(x), Some(y)) = (number(&t, "x"), number(&t, "y"))
    {
        #[allow(clippy::cast_possible_truncation)]
        let (x, y) = (x.round() as i32, y.round() as i32);
        return Rect::new(x - 40, y - 12, x + 40, y + 12);
    }
    let slot = i32::try_from(index).unwrap_or(i32::MAX / 100) * 100;
    Rect::new(0, slot, 80, slot + 40)
}

/// Workflow lanes and data-flow stages are not drawn as frames; archify
/// records them in `data-node-context` as "Lane › Stage". Build groups from
/// that path — but only when it actually distinguishes nodes, since a single
/// shared context ("Architecture component") carries no structure.
fn groups_from_contexts(nodes: &[Node], contexts: &[String]) -> Vec<Group> {
    let distinct: HashSet<&str> = contexts.iter().map(String::as_str).collect();
    if distinct.len() < 2 {
        return Vec::new();
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut by_path: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    for (n, context) in contexts.iter().enumerate() {
        let path: Vec<String> = context
            .split(CONTEXT_SEPARATOR)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        let mut parent: Option<usize> = None;
        for depth in 1..=path.len() {
            let key = path[..depth].to_vec();
            let g = *by_path.entry(key).or_insert_with(|| {
                groups.push(Group {
                    title: path[depth - 1].clone(),
                    rect: nodes[n].rect,
                    nodes: Vec::new(),
                    groups: Vec::new(),
                    parent,
                });
                let g = groups.len() - 1;
                if let Some(p) = parent {
                    groups[p].groups.push(g);
                }
                g
            });
            let r = nodes[n].rect;
            let gr = &mut groups[g].rect;
            *gr = Rect::new(
                gr.x0.min(r.x0),
                gr.y0.min(r.y0),
                gr.x1.max(r.x1),
                gr.y1.max(r.y1),
            );
            parent = Some(g);
        }
        if let Some(g) = parent {
            groups[g].nodes.push(n);
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINI: &str = r#"<!doctype html><html><head><title>Mini</title>
<script>if (a < b && c > d) { document.write("<g data-node-id='fake'>") }</script></head>
<body><svg viewBox="0 0 400 300">
<rect data-graph-role="structural-frame" data-composition-frame-label="Core" x="100" y="0" width="300" height="200"/>
<g data-node-id="a" data-node-label="Alpha" data-node-sublabel="first &amp; best" data-node-context="Architecture component">
  <rect x="0" y="50" width="80" height="40"/><text x="40" y="70">Alpha</text></g>
<g data-node-id="b" data-node-label="Beta" data-node-tag="v2" data-node-context="Core">
  <rect x="150" y="50" width="80" height="40"/></g>
<path data-edge-from="a" data-edge-to="b" data-edge-label="HTTPS" data-edge-key="0" d="M0 0"/>
<g data-detail="context" data-edge-from="a" data-edge-to="b" data-edge-label="HTTPS" data-edge-key="0"></g>
</svg></body></html>"#;

    #[test]
    fn reads_nodes_frames_and_edges_from_archify_markup() {
        let (d, title) = parse(MINI).unwrap();
        assert_eq!(title, "Mini");
        assert_eq!(d.nodes.len(), 2, "script text must not create nodes");
        assert_eq!(d.nodes[0].detail, vec!["first & best"]);
        assert_eq!(d.nodes[1].detail, vec!["v2"]);
        assert_eq!(d.edges.len(), 1, "duplicate drawings collapse");
        assert_eq!(d.edges[0].label.as_deref(), Some("HTTPS"));
        assert_eq!(d.groups.len(), 1);
        assert_eq!(d.groups[0].title, "Core");
        assert_eq!(d.groups[0].nodes, vec![1]);
    }

    #[test]
    fn lanes_become_groups_when_there_are_no_frames() {
        let html = r#"<svg>
<g data-node-id="a" data-node-label="A" data-node-context="Ops › Intake"><rect x="0" y="0" width="10" height="10"/></g>
<g data-node-id="b" data-node-label="B" data-node-context="Ops › Review"><rect x="20" y="0" width="10" height="10"/></g>
<g data-node-id="c" data-node-label="C" data-node-context="Dev"><rect x="40" y="0" width="10" height="10"/></g>
</svg>"#;
        let (d, _) = parse(html).unwrap();
        let titles: Vec<&str> = d.groups.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, vec!["Ops", "Intake", "Review", "Dev"]);
        assert_eq!(d.groups[1].parent, Some(0));
    }

    #[test]
    fn a_shared_generic_context_is_not_a_group() {
        let html = r#"<svg>
<g data-node-id="a" data-node-label="A" data-node-context="Sequence participant"></g>
<g data-node-id="b" data-node-label="B" data-node-context="Sequence participant"></g>
</svg>"#;
        let (d, _) = parse(html).unwrap();
        assert!(d.groups.is_empty());
        assert!(
            d.nodes[0].rect.y0 < d.nodes[1].rect.y0,
            "document order is kept"
        );
    }

    #[test]
    fn unlabelled_layout_bands_defer_to_node_context() {
        let html = r#"<svg>
<rect data-graph-role="structural-frame" data-composition-frame-kind="lane" x="0" y="0" width="100" height="50"/>
<g data-node-id="a" data-node-label="A" data-node-context="Ops"><rect x="10" y="10" width="10" height="10"/></g>
<g data-node-id="b" data-node-label="B" data-node-context="Dev"><rect x="10" y="60" width="10" height="10"/></g>
</svg>"#;
        let (d, _) = parse(html).unwrap();
        let titles: Vec<&str> = d.groups.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, vec!["Ops", "Dev"]);
    }

    #[test]
    fn plain_html_is_refused_with_guidance() {
        let err = parse("<html><body><p>hi</p></body></html>").unwrap_err();
        assert!(err.to_string().contains("archify"), "{err}");
    }
}
