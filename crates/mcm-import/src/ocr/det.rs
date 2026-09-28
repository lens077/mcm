//! DB text detection (PP-OCR `det` model) with an axis-aligned post-process.
//!
//! Diagrams are rendered, not photographed: text is horizontal, so the
//! rotated-polygon half of PaddleOCR's DB post-process buys nothing here and
//! connected-component bounding boxes are exact enough.

use image::RgbImage;
use image::imageops::FilterType;
use rten::Model;
use rten_tensor::NdTensor;
use rten_tensor::prelude::*;

use crate::ImportError;
use crate::raster::Rect;

/// Short side is brought up to this many pixels so small UI text survives.
const MIN_SHORT_SIDE: f32 = 736.0;
/// Upper bound on the detector input, keeps latency and memory bounded.
const MAX_PIXELS: f32 = 12_000_000.0;
const BIN_THRESHOLD: f32 = 0.3;
const BOX_THRESHOLD: f32 = 0.6;
const UNCLIP_RATIO: f32 = 1.5;

/// ImageNet statistics in the BGR order the Paddle models were trained with.
const MEAN_BGR: [f32; 3] = [0.485, 0.456, 0.406];
const STD_BGR: [f32; 3] = [0.229, 0.224, 0.225];

pub struct Detection {
    pub rect: Rect,
}

pub fn detect(model: &Model, img: &RgbImage) -> Result<Vec<Detection>, ImportError> {
    let (w, h) = img.dimensions();
    let (wf, hf) = (w as f32, h as f32);
    let mut scale = (MIN_SHORT_SIDE / wf.min(hf)).max(1.0);
    if wf * hf * scale * scale > MAX_PIXELS {
        scale = (MAX_PIXELS / (wf * hf)).sqrt();
    }
    let round32 = |v: f32| (((v + 16.0) as u32) / 32 * 32).max(32);
    let (tw, th) = (round32(wf * scale), round32(hf * scale));
    let resized = if (tw, th) == (w, h) {
        img.clone()
    } else {
        image::imageops::resize(img, tw, th, FilterType::Triangle)
    };
    let (sx, sy) = (wf / tw as f32, hf / th as f32);

    let (tw_us, th_us) = (tw as usize, th as usize);
    let plane = tw_us * th_us;
    let mut data = vec![0f32; 3 * plane];
    for (i, p) in resized.pixels().enumerate() {
        // Channel c of the tensor is BGR index c → RGB index 2 - c.
        for c in 0..3 {
            data[c * plane + i] = (f32::from(p[2 - c]) / 255.0 - MEAN_BGR[c]) / STD_BGR[c];
        }
    }
    let input = NdTensor::from_data([1, 3, th_us, tw_us], data);
    let out = model
        .run_one(input.view().into(), None)
        .map_err(|e| ImportError::Ocr(format!("文字检测推理失败：{e}")))?;
    let prob: NdTensor<f32, 4> = out
        .try_into()
        .map_err(|_| ImportError::Ocr("文字检测输出形状异常".into()))?;
    let prob = prob.to_vec();
    Ok(boxes_from_probability(&prob, tw_us, th_us)
        .into_iter()
        .map(|r| Detection {
            rect: Rect::new(
                (r.0 * sx).floor() as i32,
                (r.1 * sy).floor() as i32,
                (r.2 * sx).ceil().min(wf) as i32,
                (r.3 * sy).ceil().min(hf) as i32,
            )
            .clamp_to(w as i32, h as i32),
        })
        .collect())
}

/// Connected components of the binarised probability map → unclipped boxes
/// `(x0, y0, x1, y1)` in detector-input pixels.
fn boxes_from_probability(prob: &[f32], w: usize, h: usize) -> Vec<(f32, f32, f32, f32)> {
    let mut label = vec![u32::MAX; w * h];
    let mut boxes = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if prob[start] <= BIN_THRESHOLD || label[start] != u32::MAX {
            continue;
        }
        let id = boxes.len() as u32;
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        let (mut sum, mut n) = (0f32, 0usize);
        label[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            sum += prob[i];
            n += 1;
            let mut visit = |j: usize| {
                if prob[j] > BIN_THRESHOLD && label[j] == u32::MAX {
                    label[j] = id;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        boxes.push((x0, y0, x1 + 1, y1 + 1, sum / n as f32));
    }
    boxes
        .into_iter()
        .filter(|&(x0, y0, x1, y1, score)| x1 - x0 >= 3 && y1 - y0 >= 3 && score >= BOX_THRESHOLD)
        .map(|(x0, y0, x1, y1, _)| {
            let (bw, bh) = ((x1 - x0) as f32, (y1 - y0) as f32);
            // Same offset PaddleOCR's unclip uses for a rectangle polygon.
            let d = bw * bh * UNCLIP_RATIO / (2.0 * (bw + bh));
            (
                (x0 as f32 - d).max(0.0),
                (y0 as f32 - d).max(0.0),
                (x1 as f32 + d).min(w as f32),
                (y1 as f32 + d).min(h as f32),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probability_blob_becomes_one_unclipped_box() {
        let (w, h) = (40, 20);
        let mut prob = vec![0f32; w * h];
        for y in 8..12 {
            for x in 10..30 {
                prob[y * w + x] = 0.9;
            }
        }
        let boxes = boxes_from_probability(&prob, w, h);
        assert_eq!(boxes.len(), 1);
        let (x0, y0, x1, y1) = boxes[0];
        assert!(x0 < 10.0 && x1 > 30.0 && y0 < 8.0 && y1 > 12.0);
    }

    #[test]
    fn weak_blobs_are_rejected() {
        let (w, h) = (20, 20);
        let mut prob = vec![0f32; w * h];
        for y in 5..10 {
            for x in 5..10 {
                prob[y * w + x] = 0.4;
            }
        }
        assert!(boxes_from_probability(&prob, w, h).is_empty());
    }
}
