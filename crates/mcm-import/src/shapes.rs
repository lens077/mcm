//! Box detection: rectangles (solid or dashed, square or rounded corners)
//! assembled from thin straight strokes.
//!
//! Working from strokes rather than filled regions is what lets one detector
//! handle white-on-white boxes, tinted boxes and dashed group frames alike;
//! the fill of a box is a thick uniform band and is rejected as a stroke.

use image::RgbImage;

use crate::raster::{Rect, Rgb, color_dist, mean_color, px};

/// A pixel is "ink" when it differs this much from the paper.
pub const INK_THRESHOLD: u8 = 48;
/// Largest gap bridged inside one stroke — covers dash patterns.
const MAX_GAP: i32 = 6;
/// Pixels whose colour differs more than this from the running stroke colour
/// end the stroke (fill vs border, text vs fill).
const STROKE_COLOR_TOLERANCE: u8 = 70;
const MIN_STROKE_LEN: i32 = 24;
const MIN_STROKE_DENSITY: f32 = 0.45;
const MAX_STROKE_THICKNESS: i32 = 5;
/// Longest stretch of a border that may be hidden under something else.
const MAX_INTERRUPTION: i32 = 40;
/// Largest corner radius (and misalignment) tolerated when pairing strokes.
const CORNER_SLACK: i32 = 26;
const MIN_BOX_W: i32 = 40;
const MIN_BOX_H: i32 = 24;
const SIDE_COLOR_TOLERANCE: u8 = 90;

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Shape {
    /// Outer bounds along the stroke centre lines.
    pub rect: Rect,
    pub stroke: Rgb,
    pub dashed: bool,
}

#[derive(Debug, Clone, Copy)]
struct Stroke {
    /// Position across the stroke (y for horizontal, x for vertical).
    at: i32,
    from: i32,
    to: i32,
    thickness: i32,
    color: Rgb,
    density: f32,
}

/// Ink mask with text areas removed; `true` = ink.
pub struct InkMap {
    pub w: i32,
    pub h: i32,
    pub ink: Vec<bool>,
}

impl InkMap {
    #[must_use]
    pub fn new(img: &RgbImage, paper: Rgb, blanked: &[Rect]) -> Self {
        let (w, h) = (img.width() as i32, img.height() as i32);
        let mut ink: Vec<bool> = img
            .pixels()
            .map(|p| color_dist([p[0], p[1], p[2]], paper) > INK_THRESHOLD)
            .collect();
        for r in blanked {
            let r = r.clamp_to(w, h);
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    ink[(y * w + x) as usize] = false;
                }
            }
        }
        Self { w, h, ink }
    }

    #[inline]
    #[must_use]
    pub fn at(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h && self.ink[(y * self.w + x) as usize]
    }
}

/// Find every box outline in the image.
#[must_use]
pub fn detect_boxes(img: &RgbImage, ink: &InkMap) -> Vec<Shape> {
    let horizontal = strokes(img, ink, true);
    let vertical = strokes(img, ink, false);
    let mut shapes: Vec<Shape> = Vec::new();

    for top in &horizontal {
        for bottom in &horizontal {
            if bottom.at - top.at < MIN_BOX_H
                || (top.from - bottom.from).abs() > CORNER_SLACK / 2
                || (top.to - bottom.to).abs() > CORNER_SLACK / 2
                || color_dist(top.color, bottom.color) > SIDE_COLOR_TOLERANCE
            {
                continue;
            }
            let (from, to) = (top.from.min(bottom.from), top.to.max(bottom.to));
            let side = |near: i32, is_left: bool| {
                vertical.iter().find(|v| {
                    let offset = if is_left { near - v.at } else { v.at - near };
                    // Rounded corners: the side sits outside the horizontal run;
                    // square corners: they share the corner pixel.
                    (-3..=CORNER_SLACK).contains(&offset)
                        && v.from - top.at <= CORNER_SLACK
                        && bottom.at - v.to <= CORNER_SLACK
                        && v.from >= top.at - 3
                        && v.to <= bottom.at + 3
                        && color_dist(v.color, top.color) <= SIDE_COLOR_TOLERANCE
                })
            };
            let (Some(left), Some(right)) = (side(from, true), side(to, false)) else {
                continue;
            };
            let rect = Rect::new(
                left.at.min(from),
                top.at,
                right.at.max(to) + 1,
                bottom.at + 1,
            );
            if rect.width() < MIN_BOX_W {
                continue;
            }
            let dashed = [top, bottom, left, right].iter().any(|s| s.density < 0.85);
            let stroke = mean_color(&[top.color, bottom.color, left.color, right.color])
                .unwrap_or(top.color);
            shapes.push(Shape {
                rect,
                stroke,
                dashed,
            });
        }
    }
    dedupe(shapes)
}

/// Keep the tightest of near-identical detections (a border two pixels thick
/// yields one candidate per pixel row).
fn dedupe(mut shapes: Vec<Shape>) -> Vec<Shape> {
    shapes.sort_by_key(|s| s.rect.area());
    let mut out: Vec<Shape> = Vec::new();
    for s in shapes {
        let duplicate = out.iter().any(|o| {
            (o.rect.x0 - s.rect.x0).abs() <= 6
                && (o.rect.y0 - s.rect.y0).abs() <= 6
                && (o.rect.x1 - s.rect.x1).abs() <= 6
                && (o.rect.y1 - s.rect.y1).abs() <= 6
        });
        if !duplicate {
            out.push(s);
        }
    }
    out.sort_by_key(|s| (s.rect.y0, s.rect.x0));
    out
}

/// Thin straight strokes along rows (`horizontal`) or columns.
fn strokes(img: &RgbImage, ink: &InkMap, horizontal: bool) -> Vec<Stroke> {
    let (lines, len) = if horizontal {
        (ink.h, ink.w)
    } else {
        (ink.w, ink.h)
    };
    let pos = |line: i32, i: i32| if horizontal { (i, line) } else { (line, i) };

    // 1. Runs per scan line.
    let mut runs: Vec<Vec<Stroke>> = Vec::with_capacity(lines as usize);
    for line in 0..lines {
        let mut out = Vec::new();
        let mut run: Option<(i32, i32, [u64; 3], u32)> = None; // start, last ink, colour sum, count
        let close = |run: &mut Option<(i32, i32, [u64; 3], u32)>, out: &mut Vec<Stroke>| {
            if let Some((start, last, sum, n)) = run.take() {
                let length = last - start + 1;
                let density = n as f32 / length as f32;
                if length >= MIN_STROKE_LEN && density >= MIN_STROKE_DENSITY {
                    let color = [0, 1, 2].map(|c| (sum[c] / u64::from(n)) as u8);
                    out.push(Stroke {
                        at: line,
                        from: start,
                        to: last,
                        thickness: 1,
                        color,
                        density,
                    });
                }
            }
        };
        for i in 0..len {
            let (x, y) = pos(line, i);
            if !ink.at(x, y) {
                if run.is_some_and(|r| i - r.1 > MAX_GAP) {
                    close(&mut run, &mut out);
                }
                continue;
            }
            let p = px(img, x, y);
            if let Some(r) = run.as_mut() {
                let current = [0, 1, 2].map(|c| (r.2[c] / u64::from(r.3)) as u8);
                if color_dist(current, p) <= STROKE_COLOR_TOLERANCE {
                    r.1 = i;
                    for (sum, v) in r.2.iter_mut().zip(p) {
                        *sum += u64::from(v);
                    }
                    r.3 += 1;
                    continue;
                }
                // A differently coloured pixel is a gap for this stroke: a
                // connector crossing a frame must not cut the frame in two.
                if i - r.1 <= MAX_GAP {
                    continue;
                }
                close(&mut run, &mut out);
            }
            run = Some((i, i, [u64::from(p[0]), u64::from(p[1]), u64::from(p[2])], 1));
        }
        close(&mut run, &mut out);
        runs.push(out);
    }

    // 2. Merge runs on adjacent scan lines into strokes with a thickness.
    let mut done: Vec<Stroke> = Vec::new();
    let mut open: Vec<Stroke> = Vec::new();
    for (line, line_runs) in runs.into_iter().enumerate() {
        let line = line as i32;
        let mut next_open = Vec::new();
        for r in line_runs {
            if let Some(k) = open.iter().position(|o| {
                o.at + o.thickness == line
                    && (o.from - r.from).abs() <= 3
                    && (o.to - r.to).abs() <= 3
                    && color_dist(o.color, r.color) <= STROKE_COLOR_TOLERANCE
            }) {
                let mut o = open.swap_remove(k);
                o.thickness += 1;
                o.from = o.from.min(r.from);
                o.to = o.to.max(r.to);
                o.density = o.density.max(r.density);
                // Keep the darkest row's colour: it is the stroke core, the
                // others are anti-aliasing.
                if luminance(r.color) < luminance(o.color) {
                    o.color = r.color;
                }
                next_open.push(o);
            } else {
                next_open.push(r);
            }
        }
        done.append(&mut open);
        open = next_open;
    }
    done.append(&mut open);
    let thin: Vec<Stroke> = done
        .into_iter()
        .filter(|s| s.thickness <= MAX_STROKE_THICKNESS)
        .map(|mut s| {
            s.at += s.thickness / 2;
            s
        })
        .collect();
    join_collinear(thin)
}

/// Rejoin pieces of one border that something drawn on top interrupted
/// (arrowheads and labels sitting on a group frame).
fn join_collinear(mut strokes: Vec<Stroke>) -> Vec<Stroke> {
    strokes.sort_by_key(|s| (s.at, s.from));
    let mut out: Vec<Stroke> = Vec::with_capacity(strokes.len());
    for s in strokes {
        if let Some(prev) = out.iter_mut().rev().take(8).find(|p| {
            (p.at - s.at).abs() <= 1
                && s.from > p.to
                && s.from - p.to <= MAX_INTERRUPTION
                && color_dist(p.color, s.color) <= STROKE_COLOR_TOLERANCE
        }) {
            let (a, b) = ((prev.to - prev.from + 1) as f32, (s.to - s.from + 1) as f32);
            let span = (s.to - prev.from + 1) as f32;
            prev.density = (prev.density * a + s.density * b) / span;
            prev.to = s.to;
            continue;
        }
        out.push(s);
    }
    out
}

fn luminance(c: Rgb) -> u32 {
    299 * u32::from(c[0]) + 587 * u32::from(c[1]) + 114 * u32::from(c[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb as Px;

    fn canvas(w: u32, h: u32) -> RgbImage {
        RgbImage::from_pixel(w, h, Px([250, 250, 250]))
    }

    fn stroke_rect(
        img: &mut RgbImage,
        r: Rect,
        color: [u8; 3],
        dash: Option<(i32, i32)>,
        fill: Option<[u8; 3]>,
    ) {
        if let Some(f) = fill {
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    img.put_pixel(x as u32, y as u32, Px(f));
                }
            }
        }
        let on = |i: i32| dash.is_none_or(|(a, b)| i.rem_euclid(a + b) < a);
        for x in r.x0..r.x1 {
            for t in 0..2 {
                if on(x) {
                    img.put_pixel(x as u32, (r.y0 + t) as u32, Px(color));
                    img.put_pixel(x as u32, (r.y1 - 1 - t) as u32, Px(color));
                }
            }
        }
        for y in r.y0..r.y1 {
            for t in 0..2 {
                if on(y) {
                    img.put_pixel((r.x0 + t) as u32, y as u32, Px(color));
                    img.put_pixel((r.x1 - 1 - t) as u32, y as u32, Px(color));
                }
            }
        }
    }

    #[test]
    fn finds_filled_solid_box_and_dashed_frame() {
        let mut img = canvas(400, 300);
        stroke_rect(
            &mut img,
            Rect::new(20, 20, 380, 280),
            [245, 158, 11],
            Some((8, 4)),
            None,
        );
        stroke_rect(
            &mut img,
            Rect::new(60, 60, 200, 120),
            [16, 185, 129],
            None,
            Some([209, 250, 229]),
        );
        let ink = InkMap::new(&img, [250, 250, 250], &[]);
        let boxes = detect_boxes(&img, &ink);
        assert_eq!(boxes.len(), 2, "{boxes:?}");
        let frame = boxes.iter().find(|b| b.rect.width() > 300).unwrap();
        assert!(frame.dashed);
        let node = boxes.iter().find(|b| b.rect.width() < 300).unwrap();
        assert!(!node.dashed);
        assert!((node.rect.x0 - 60).abs() <= 2 && (node.rect.y1 - 120).abs() <= 2);
    }

    #[test]
    fn a_lone_line_is_not_a_box() {
        let mut img = canvas(200, 100);
        for x in 10..190 {
            img.put_pixel(x, 50, Px([100, 100, 100]));
        }
        let ink = InkMap::new(&img, [250, 250, 250], &[]);
        assert!(detect_boxes(&img, &ink).is_empty());
    }
}
