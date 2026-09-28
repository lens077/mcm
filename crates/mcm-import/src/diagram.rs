//! Format-neutral diagram model shared by every importer (image now, HTML
//! next): nodes with text, nested groups, and directed edges.

use serde::Serialize;

use crate::connectors::Link;
use crate::ocr::TextLine;
use crate::raster::Rect;
use crate::shapes::Shape;

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub title: String,
    /// Secondary lines under the title (subtitle, description).
    pub detail: Vec<String>,
    pub rect: Rect,
}

#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub title: String,
    pub rect: Rect,
    /// Indices into [`Diagram::nodes`].
    pub nodes: Vec<usize>,
    /// Indices into [`Diagram::groups`] nested directly inside.
    pub groups: Vec<usize>,
    pub parent: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub undirected: bool,
    /// Relationship text drawn on the connector, when the source has it.
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Diagram {
    pub nodes: Vec<Node>,
    pub groups: Vec<Group>,
    pub edges: Vec<Edge>,
    /// Text that belongs to no node or group (legends, edge labels, captions).
    pub loose_text: Vec<String>,
}

impl Diagram {
    /// Group that directly contains a node, if any.
    #[must_use]
    pub fn group_of_node(&self, node: usize) -> Option<usize> {
        self.groups.iter().position(|g| g.nodes.contains(&node))
    }
}

/// Split detected rectangles into leaf nodes and group frames: a rectangle
/// that encloses another one is a group.
#[must_use]
pub fn classify(shapes: &[Shape]) -> (Vec<Shape>, Vec<Shape>) {
    let mut leaves = Vec::new();
    let mut frames = Vec::new();
    for (i, s) in shapes.iter().enumerate() {
        let encloses = shapes
            .iter()
            .enumerate()
            .any(|(j, o)| i != j && s.rect.contains(&o.rect) && o.rect.area() < s.rect.area());
        if encloses {
            frames.push(*s);
        } else {
            leaves.push(*s);
        }
    }
    (leaves, frames)
}

/// Put recognised text into the boxes and frames that hold it.
#[must_use]
pub fn assemble(leaves: &[Shape], frames: &[Shape], links: &[Link], text: &[TextLine]) -> Diagram {
    let mut used = vec![false; text.len()];

    let mut nodes: Vec<Node> = leaves
        .iter()
        .map(|leaf| {
            let inside: Vec<usize> = (0..text.len())
                .filter(|&i| {
                    let (cx, cy) = text[i].rect.center();
                    leaf.rect.contains_point(cx, cy)
                })
                .collect();
            for &i in &inside {
                used[i] = true;
            }
            let mut rows = rows_of(inside.iter().map(|&i| &text[i]).collect());
            let title = if rows.is_empty() {
                "（未识别文字）".to_owned()
            } else {
                rows.remove(0)
            };
            Node {
                title,
                detail: rows,
                rect: leaf.rect,
            }
        })
        .collect();

    let frame_rects: Vec<Rect> = frames.iter().map(|f| f.rect).collect();
    let node_rects: Vec<Rect> = nodes.iter().map(|n| n.rect).collect();
    let mut groups = nest(&node_rects, &frame_rects);
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by_key(|&i| groups[i].rect.area());
    // A group's title is the free text hugging its top edge.
    // Innermost first, so an outer frame cannot take a nested frame's label.
    for &gi in &order {
        let g = &mut groups[gi];
        let near_top: Vec<usize> = (0..text.len())
            .filter(|&i| !used[i])
            .filter(|&i| {
                let r = text[i].rect;
                let (cx, cy) = r.center();
                g.rect.contains_point(cx, cy) && cy - g.rect.y0 <= r.height() * 3
            })
            .collect();
        // The label may come back as several boxes; take the whole top row.
        let Some(&first) = near_top
            .iter()
            .min_by_key(|&&i| (text[i].rect.y0, text[i].rect.x0))
        else {
            continue;
        };
        let band = text[first].rect;
        let row: Vec<usize> = near_top
            .into_iter()
            .filter(|&i| {
                let (_, cy) = text[i].rect.center();
                cy >= band.y0 && cy < band.y1
            })
            .collect();
        for &i in &row {
            used[i] = true;
        }
        g.title = rows_of(row.iter().map(|&i| &text[i]).collect()).join(" ");
    }
    name_unnamed(&mut groups);

    let loose_text = rows_of(
        (0..text.len())
            .filter(|&i| !used[i])
            .map(|i| &text[i])
            .collect(),
    );
    for node in &mut nodes {
        node.title = node.title.trim().to_owned();
    }
    let edges = links
        .iter()
        .map(|l| Edge {
            from: l.from,
            to: l.to,
            undirected: l.undirected,
            label: None,
        })
        .collect();
    Diagram {
        nodes,
        groups,
        edges,
        loose_text,
    }
}

/// Groups for `frames` (titles empty): each node and frame belongs to the
/// smallest frame that encloses it.
#[must_use]
pub fn nest(nodes: &[Rect], frames: &[Rect]) -> Vec<Group> {
    let mut order: Vec<usize> = (0..frames.len()).collect();
    order.sort_by_key(|&i| frames[i].area());
    let mut groups: Vec<Group> = frames
        .iter()
        .map(|&rect| Group {
            title: String::new(),
            rect,
            nodes: Vec::new(),
            groups: Vec::new(),
            parent: None,
        })
        .collect();
    let smallest_frame = |rect: &Rect, exclude: Option<usize>| {
        order
            .iter()
            .copied()
            .find(|&g| Some(g) != exclude && frames[g].contains(rect) && frames[g] != *rect)
    };
    for (n, rect) in nodes.iter().enumerate() {
        if let Some(g) = smallest_frame(rect, None) {
            groups[g].nodes.push(n);
        }
    }
    for g in 0..groups.len() {
        if let Some(p) = smallest_frame(&groups[g].rect, Some(g)) {
            groups[g].parent = Some(p);
            groups[p].groups.push(g);
        }
    }
    groups
}

/// Give untitled groups a stable placeholder so the outline stays valid.
pub fn name_unnamed(groups: &mut [Group]) {
    let mut unnamed = 0;
    for g in groups.iter_mut().filter(|g| g.title.trim().is_empty()) {
        unnamed += 1;
        g.title = format!("分组 {unnamed}");
    }
}

/// Join text boxes that sit on the same visual line, top to bottom.
fn rows_of(mut lines: Vec<&TextLine>) -> Vec<String> {
    lines.sort_by_key(|l| (l.rect.y0, l.rect.x0));
    let mut rows: Vec<(Rect, Vec<&TextLine>)> = Vec::new();
    for l in lines {
        let (_, cy) = l.rect.center();
        if let Some(row) = rows.iter_mut().find(|(r, _)| {
            let (_, rcy) = r.center();
            (cy - rcy).abs() * 2 < r.height().min(l.rect.height())
        }) {
            row.0 = Rect::new(
                row.0.x0.min(l.rect.x0),
                row.0.y0.min(l.rect.y0),
                row.0.x1.max(l.rect.x1),
                row.0.y1.max(l.rect.y1),
            );
            row.1.push(l);
        } else {
            rows.push((l.rect, vec![l]));
        }
    }
    rows.sort_by_key(|(r, _)| (r.y0, r.x0));
    let mut out = Vec::new();
    for (_, mut parts) in rows {
        parts.sort_by_key(|p| p.rect.x0);
        // Far-apart pieces on one baseline are separate labels, not one line.
        let mut current: Vec<&str> = Vec::new();
        let mut right = i32::MIN;
        for p in parts {
            if !current.is_empty() && p.rect.x0 - right > p.rect.height() * 6 {
                out.push(current.join(" "));
                current.clear();
            }
            current.push(&p.text);
            right = right.max(p.rect.x1);
        }
        if !current.is_empty() {
            out.push(current.join(" "));
        }
    }
    out
}
