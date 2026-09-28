//! Import of an already-parsed graph: ids, labels, subgraph membership and
//! arrows, with no geometry.
//!
//! The Mermaid importer uses this: the webview parses Mermaid with the mermaid
//! library itself (the same parser that renders the preview), and hands the
//! structure over. Mapping to a plan stays here, so Mermaid obeys exactly the
//! rules the image and archify importers do. Without coordinates, document
//! order is the reading order.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ImportError;
use crate::diagram::{self, Diagram, Edge, End, Group, Node};
use crate::raster::Rect;

/// Upper bound on nodes plus groups; far beyond any hand-written diagram,
/// low enough that a malformed request cannot stall the importer.
const MAX_ITEMS: usize = 10_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphSpec {
    /// Diagram title; empty means "use the file name".
    #[serde(default)]
    pub title: String,
    /// In first-appearance order.
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub groups: Vec<GraphGroup>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    /// Title line; empty falls back to the id.
    #[serde(default)]
    pub label: String,
    /// Further label lines, kept as task notes.
    #[serde(default)]
    pub detail: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphGroup {
    pub id: String,
    #[serde(default)]
    pub label: String,
    /// Ids of member nodes and nested groups. A member listed by several
    /// groups belongs to the innermost one, so flattened lists work too.
    #[serde(default)]
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Node or group id.
    pub from: String,
    pub to: String,
    /// False for plain lines and two-headed arrows.
    #[serde(default = "yes")]
    pub directed: bool,
    #[serde(default)]
    pub label: Option<String>,
}

const fn yes() -> bool {
    true
}

/// Build a [`Diagram`] from `spec`.
///
/// A node whose id is also a group id is dropped: Mermaid creates a vertex
/// for every arrow end, even when the arrow points at a subgraph.
///
/// # Errors
/// [`ImportError::Unsupported`] for an empty or oversized graph, an arrow to
/// an unknown id, or subgraphs that contain each other.
pub fn parse(spec: &GraphSpec) -> Result<Diagram, ImportError> {
    if spec.nodes.len() + spec.groups.len() > MAX_ITEMS {
        return Err(ImportError::Unsupported(format!(
            "图中元素超过 {MAX_ITEMS} 个，无法导入"
        )));
    }

    let mut ends: HashMap<&str, End> = HashMap::new();
    let mut groups: Vec<Group> = Vec::new();
    for g in &spec.groups {
        if ends.contains_key(g.id.as_str()) {
            continue;
        }
        ends.insert(&g.id, End::Group(groups.len()));
        groups.push(Group {
            title: label_or_id(&g.label, &g.id),
            rect: Rect::new(0, 0, 0, 0),
            nodes: Vec::new(),
            groups: Vec::new(),
            parent: None,
        });
    }
    let mut nodes: Vec<Node> = Vec::new();
    for n in &spec.nodes {
        if ends.contains_key(n.id.as_str()) {
            continue;
        }
        ends.insert(&n.id, End::Node(nodes.len()));
        let slot = i32::try_from(nodes.len()).unwrap_or(i32::MAX / 100) * 100;
        nodes.push(Node {
            title: label_or_id(&n.label, &n.id),
            detail: n
                .detail
                .iter()
                .map(|d| d.trim().to_owned())
                .filter(|d| !d.is_empty())
                .collect(),
            rect: Rect::new(0, slot, 100, slot + 40),
        });
    }
    if nodes.is_empty() && groups.is_empty() {
        return Err(ImportError::Unsupported("图中没有任何节点".into()));
    }

    // Which groups list each item (by the item's End).
    let mut listed_by: HashMap<End, Vec<usize>> = HashMap::new();
    let mut seen_groups = std::collections::HashSet::new();
    for g in &spec.groups {
        let Some(&End::Group(gi)) = ends.get(g.id.as_str()) else {
            continue;
        };
        if !seen_groups.insert(gi) {
            continue;
        }
        for m in &g.members {
            match ends.get(m.as_str()) {
                Some(&end) if end != End::Group(gi) => {
                    let by = listed_by.entry(end).or_default();
                    if !by.contains(&gi) {
                        by.push(gi);
                    }
                }
                _ => {}
            }
        }
    }
    let depth = group_depths(groups.len(), &listed_by)?;
    let innermost = |end: End| {
        listed_by.get(&end).and_then(|by| {
            by.iter()
                .copied()
                .max_by_key(|&g| (depth[g], usize::MAX - g))
        })
    };
    for g in 0..groups.len() {
        if let Some(p) = innermost(End::Group(g)) {
            groups[g].parent = Some(p);
            groups[p].groups.push(g);
        }
    }
    for n in 0..nodes.len() {
        if let Some(g) = innermost(End::Node(n)) {
            groups[g].nodes.push(n);
        }
    }
    place_groups(&mut groups, &nodes);
    diagram::name_unnamed(&mut groups);

    let mut edges = Vec::new();
    for e in &spec.edges {
        let end = |id: &str| {
            ends.get(id)
                .copied()
                .ok_or_else(|| ImportError::Unsupported(format!("连线引用了不存在的节点「{id}」")))
        };
        edges.push(Edge {
            from: end(&e.from)?,
            to: end(&e.to)?,
            undirected: !e.directed,
            label: e
                .label
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned),
        });
    }

    Ok(Diagram {
        nodes,
        groups,
        edges,
        loose_text: Vec::new(),
    })
}

fn label_or_id(label: &str, id: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        id.trim().to_owned()
    } else {
        label.to_owned()
    }
}

/// Nesting depth of every group (0 = top level), following the deepest
/// listing group.
fn group_depths(
    count: usize,
    listed_by: &HashMap<End, Vec<usize>>,
) -> Result<Vec<usize>, ImportError> {
    const UNSET: usize = usize::MAX;
    const VISITING: usize = usize::MAX - 1;
    let mut depth = vec![UNSET; count];
    for start in 0..count {
        // Iterative DFS: a pathological chain must not overflow the stack.
        let mut stack = vec![(start, false)];
        while let Some((g, expanded)) = stack.pop() {
            if expanded {
                let parents = listed_by.get(&End::Group(g)).map_or(&[][..], Vec::as_slice);
                depth[g] = parents.iter().map(|&p| depth[p] + 1).max().unwrap_or(0);
                continue;
            }
            match depth[g] {
                UNSET => {}
                VISITING => {
                    return Err(ImportError::Unsupported(
                        "子图之间相互包含，无法确定层级".into(),
                    ));
                }
                _ => continue,
            }
            depth[g] = VISITING;
            stack.push((g, true));
            for &p in listed_by.get(&End::Group(g)).map_or(&[][..], Vec::as_slice) {
                if depth[p] == VISITING {
                    return Err(ImportError::Unsupported(
                        "子图之间相互包含，无法确定层级".into(),
                    ));
                }
                if depth[p] == UNSET {
                    stack.push((p, false));
                }
            }
        }
    }
    Ok(depth)
}

/// Give each group the slot of its first member, so a group is read where it
/// first appears in the source. Empty groups go last, in source order.
fn place_groups(groups: &mut [Group], nodes: &[Node]) {
    fn first_slot(g: usize, groups: &[Group], nodes: &[Node]) -> Option<i32> {
        let own = groups[g].nodes.iter().map(|&n| nodes[n].rect.y0);
        let nested = groups[g]
            .groups
            .iter()
            .filter_map(|&c| first_slot(c, groups, nodes));
        own.chain(nested).min()
    }
    let after = i32::try_from(nodes.len()).unwrap_or(i32::MAX / 200) * 100;
    for g in 0..groups.len() {
        let fallback = after + i32::try_from(g).unwrap_or(0) * 100;
        let y = first_slot(g, groups, nodes).unwrap_or(fallback);
        groups[g].rect = Rect::new(0, y, 100, y + 40);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, label: &str) -> GraphNode {
        GraphNode {
            id: id.into(),
            label: label.into(),
            detail: Vec::new(),
        }
    }

    fn group(id: &str, label: &str, members: &[&str]) -> GraphGroup {
        GraphGroup {
            id: id.into(),
            label: label.into(),
            members: members.iter().map(|&m| m.to_owned()).collect(),
        }
    }

    fn edge(from: &str, to: &str) -> GraphEdge {
        GraphEdge {
            from: from.into(),
            to: to.into(),
            directed: true,
            label: None,
        }
    }

    #[test]
    fn nested_subgraphs_become_nested_groups() {
        // Mermaid lists subgraphs innermost first and does not flatten them.
        let spec = GraphSpec {
            title: String::new(),
            nodes: vec![
                node("A", "网关"),
                node("B", "订单"),
                node("C", "支付"),
                node("core", ""),
            ],
            groups: vec![
                group("inner", "内层", &["C"]),
                group("core", "核心域", &["B", "inner"]),
            ],
            edges: vec![edge("A", "B"), edge("A", "core")],
        };
        let d = parse(&spec).unwrap();
        assert_eq!(
            d.nodes.len(),
            3,
            "the vertex Mermaid made for `core` is the group"
        );
        assert_eq!(d.groups[0].parent, Some(1));
        assert_eq!(d.groups[1].nodes, vec![1]);
        assert_eq!(d.groups[1].groups, vec![0]);
        assert_eq!(d.edges[1].to, End::Group(1));
    }

    #[test]
    fn a_member_listed_at_several_levels_belongs_to_the_innermost() {
        let spec = GraphSpec {
            nodes: vec![node("x", "X")],
            groups: vec![
                group("outer", "外", &["mid", "inner", "x"]),
                group("mid", "中", &["inner", "x"]),
                group("inner", "内", &["x"]),
            ],
            ..GraphSpec::default()
        };
        let d = parse(&spec).unwrap();
        assert_eq!(d.groups[2].nodes, vec![0]);
        assert_eq!(d.groups[2].parent, Some(1));
        assert_eq!(d.groups[1].parent, Some(0));
        assert!(d.groups[0].nodes.is_empty());
    }

    #[test]
    fn source_order_is_reading_order() {
        let spec = GraphSpec {
            title: "流程".into(),
            nodes: vec![node("s", "开始"), node("b", "乙"), node("a", "甲")],
            groups: vec![group("g", "阶段", &["a"])],
            edges: vec![edge("s", "b"), edge("b", "a")],
        };
        let imported = crate::import_graph(&spec, "file").unwrap();
        let lines: Vec<&str> = imported.outline.lines().collect();
        assert_eq!(lines[1], "%title 流程");
        let pos = |needle: &str| lines.iter().position(|l| l.contains(needle)).unwrap();
        assert!(
            pos("开始") < pos("乙") && pos("乙") < pos("阶段"),
            "{lines:?}"
        );
    }

    #[test]
    fn detail_lines_and_edge_labels_are_kept() {
        let spec = GraphSpec {
            nodes: vec![
                GraphNode {
                    id: "a".into(),
                    label: "网关".into(),
                    detail: vec!["Nginx".into(), "  ".into()],
                },
                node("b", ""),
            ],
            edges: vec![GraphEdge {
                from: "a".into(),
                to: "b".into(),
                directed: false,
                label: Some(" HTTPS ".into()),
            }],
            ..GraphSpec::default()
        };
        let imported = crate::import_graph(&spec, "file").unwrap();
        let out = &imported.outline;
        assert!(out.contains("%title file"), "{out}");
        assert!(out.contains("> Nginx"), "{out}");
        assert!(
            out.contains("- b #t2 <-t1"),
            "an empty label falls back to the id: {out}"
        );
        assert_eq!(imported.report.undirected, 1);
        assert_eq!(imported.report.edge_labels, vec!["网关 → b：HTTPS"]);
    }

    #[test]
    fn broken_graphs_are_refused_with_a_reason() {
        let empty = parse(&GraphSpec::default()).unwrap_err();
        assert!(empty.to_string().contains("没有任何节点"), "{empty}");

        let dangling = GraphSpec {
            nodes: vec![node("a", "A")],
            edges: vec![edge("a", "ghost")],
            ..GraphSpec::default()
        };
        assert!(parse(&dangling).unwrap_err().to_string().contains("ghost"));

        let looped = GraphSpec {
            nodes: vec![node("a", "A")],
            groups: vec![group("p", "P", &["q", "a"]), group("q", "Q", &["p"])],
            ..GraphSpec::default()
        };
        assert!(parse(&looped).unwrap_err().to_string().contains("相互包含"));
    }
}
