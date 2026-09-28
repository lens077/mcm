//! Connector detection: the ink left between boxes, grouped into strokes,
//! attached to the boxes it touches, and oriented by its arrowheads.

use image::RgbImage;

use crate::raster::{Rect, Rgb, color_dist, median_color, px};
use crate::shapes::INK_THRESHOLD;

/// Ink pixels closer than this (Chebyshev) belong to the same connector —
/// bridges dash gaps and anti-aliasing holes.
const LINK_RADIUS: i32 = 6;
/// How close a connector end must come to a box to count as attached.
const ATTACH_DISTANCE: i32 = 8;
/// Band outside the box in which an end's ink is weighed for an arrowhead.
const HEAD_BAND: i32 = 14;
/// An end is an arrowhead when it carries this much more ink than the
/// thinnest end of the same connector.
const HEAD_RATIO: f32 = 1.6;
const MIN_PIXELS: usize = 12;
const COLOR_MATCH: u8 = 60;

/// A detected connection between two boxes (indices into the box list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    /// No arrowhead was found on either end; direction follows reading order.
    pub undirected: bool,
}

/// Regions whose own pixels must not be mistaken for connector ink.
pub struct Masks<'a> {
    /// Leaf boxes: everything inside (text, icons, border) is removed.
    pub boxes: &'a [Rect],
    /// Group frames: only pixels matching the frame colour are removed, so
    /// connectors crossing the frame stay continuous.
    pub frames: &'a [(Rect, Rgb)],
    /// Text lines: only pixels matching the text colour are removed, so a
    /// connector running behind a label survives.
    pub texts: &'a [Rect],
}

#[must_use]
pub fn detect_links(img: &RgbImage, paper: Rgb, masks: &Masks<'_>) -> Vec<Link> {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut ink: Vec<bool> = img
        .pixels()
        .map(|p| color_dist([p[0], p[1], p[2]], paper) > INK_THRESHOLD)
        .collect();
    let idx = |x: i32, y: i32| (y * w + x) as usize;

    for r in masks.boxes {
        let r = r.inflate(2).clamp_to(w, h);
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                ink[idx(x, y)] = false;
            }
        }
    }
    for (frame, color) in masks.frames {
        let outer = frame.inflate(4).clamp_to(w, h);
        let inner = frame.inflate(-4);
        for y in outer.y0..outer.y1 {
            for x in outer.x0..outer.x1 {
                if !inner.contains_point(x, y) && is_shade_of(px(img, x, y), *color, paper) {
                    ink[idx(x, y)] = false;
                }
            }
        }
    }
    for t in masks.texts {
        let t = t.clamp_to(w, h);
        let samples: Vec<Rgb> = (t.y0..t.y1)
            .flat_map(|y| (t.x0..t.x1).map(move |x| (x, y)))
            .filter(|&(x, y)| ink[idx(x, y)])
            .map(|(x, y)| px(img, x, y))
            .collect();
        let Some(text_color) = median_color(&samples) else {
            continue;
        };
        for y in t.y0..t.y1 {
            for x in t.x0..t.x1 {
                if is_shade_of(px(img, x, y), text_color, paper) {
                    ink[idx(x, y)] = false;
                }
            }
        }
    }

    let components = components(&ink, w, h);
    let mut links = Vec::new();
    for comp in components.iter().filter(|c| c.len() >= MIN_PIXELS) {
        links.extend(links_of(comp, masks.boxes));
    }
    links.sort_by_key(|l| (l.from, l.to));
    links.dedup_by_key(|l| (l.from, l.to));
    links
}

/// `p` is `color` blended with the paper to some degree — the anti-aliased
/// fringe of a stroke or glyph drawn in `color`.
fn is_shade_of(p: Rgb, color: Rgb, paper: Rgb) -> bool {
    let d: [f32; 3] = [0, 1, 2].map(|c| f32::from(color[c]) - f32::from(paper[c]));
    let v: [f32; 3] = [0, 1, 2].map(|c| f32::from(p[c]) - f32::from(paper[c]));
    let dd: f32 = d.iter().map(|x| x * x).sum();
    if dd < 1.0 {
        return false;
    }
    // Project onto the paper→colour axis; allow a little overshoot for
    // renderers that darken the stroke core.
    let t = (d.iter().zip(&v).map(|(a, b)| a * b).sum::<f32>() / dd).clamp(0.0, 1.3);
    let residual = (0..3).map(|c| (v[c] - t * d[c]).abs()).fold(0f32, f32::max);
    residual <= f32::from(COLOR_MATCH) / 2.0
}

/// Group ink pixels that lie within [`LINK_RADIUS`] of each other.
fn components(ink: &[bool], w: i32, h: i32) -> Vec<Vec<(i32, i32)>> {
    let mut seen = vec![false; ink.len()];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..ink.len() {
        if !ink[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut comp = Vec::new();
        while let Some(i) = stack.pop() {
            let (x, y) = (i as i32 % w, i as i32 / w);
            comp.push((x, y));
            for ny in (y - LINK_RADIUS).max(0)..=(y + LINK_RADIUS).min(h - 1) {
                for nx in (x - LINK_RADIUS).max(0)..=(x + LINK_RADIUS).min(w - 1) {
                    let j = (ny * w + nx) as usize;
                    if ink[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        out.push(comp);
    }
    out
}

/// Which boxes one connector joins, and in which direction.
fn links_of(comp: &[(i32, i32)], boxes: &[Rect]) -> Vec<Link> {
    // (box index, ink weight near that box)
    let mut ends: Vec<(usize, usize)> = Vec::new();
    for (i, b) in boxes.iter().enumerate() {
        let attached = comp
            .iter()
            .any(|&(x, y)| b.distance_to(x, y) <= ATTACH_DISTANCE);
        if attached {
            let weight = comp
                .iter()
                .filter(|&&(x, y)| b.distance_to(x, y) <= HEAD_BAND)
                .count();
            ends.push((i, weight));
        }
    }
    if ends.len() < 2 {
        return Vec::new();
    }
    let thinnest = ends.iter().map(|e| e.1).min().unwrap_or(1).max(1) as f32;
    let (heads, tails): (Vec<&(usize, usize)>, Vec<_>) = ends
        .iter()
        .partition(|e| e.1 as f32 >= thinnest * HEAD_RATIO);
    if heads.is_empty() || tails.is_empty() {
        // No arrowhead (or arrowheads everywhere): connect in reading order.
        let mut order: Vec<usize> = ends.iter().map(|e| e.0).collect();
        order.sort_by_key(|&i| (boxes[i].y0, boxes[i].x0));
        return order
            .windows(2)
            .map(|p| Link {
                from: p[0],
                to: p[1],
                undirected: true,
            })
            .collect();
    }
    let mut out = Vec::new();
    for t in &tails {
        for hd in &heads {
            out.push(Link {
                from: t.0,
                to: hd.0,
                undirected: false,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb as Px;

    fn arrow_right(img: &mut RgbImage, x0: u32, x1: u32, y: u32) {
        for x in x0..x1 {
            for t in 0..2 {
                img.put_pixel(x, y + t, Px([120, 120, 120]));
            }
        }
        // Filled triangle whose tip touches x1.
        for d in 0..10u32 {
            for t in 0..=(10 - d) {
                img.put_pixel(x1 - 10 + d, y - t / 2 + 1, Px([120, 120, 120]));
                img.put_pixel(x1 - 10 + d, y + t / 2, Px([120, 120, 120]));
            }
        }
    }

    #[test]
    fn arrowhead_sets_direction() {
        let mut img = RgbImage::from_pixel(300, 100, Px([250, 250, 250]));
        let a = Rect::new(10, 30, 80, 70);
        let b = Rect::new(200, 30, 280, 70);
        arrow_right(&mut img, 80, 200, 50);
        let links = detect_links(
            &img,
            [250, 250, 250],
            &Masks {
                boxes: &[a, b],
                frames: &[],
                texts: &[],
            },
        );
        assert_eq!(
            links,
            vec![Link {
                from: 0,
                to: 1,
                undirected: false
            }]
        );
    }

    #[test]
    fn plain_line_links_in_reading_order() {
        let mut img = RgbImage::from_pixel(300, 100, Px([250, 250, 250]));
        let a = Rect::new(10, 30, 80, 70);
        let b = Rect::new(200, 30, 280, 70);
        for x in 80..200 {
            img.put_pixel(x, 50, Px([120, 120, 120]));
        }
        let links = detect_links(
            &img,
            [250, 250, 250],
            &Masks {
                boxes: &[b, a],
                frames: &[],
                texts: &[],
            },
        );
        assert_eq!(
            links,
            vec![Link {
                from: 1,
                to: 0,
                undirected: true
            }]
        );
    }
}
