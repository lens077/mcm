//! CTC text recognition (PP-OCR `rec` model).

use image::RgbImage;
use image::imageops::FilterType;
use rten::Model;
use rten_tensor::NdTensor;
use rten_tensor::prelude::*;

use crate::ImportError;

const INPUT_H: usize = 48;
/// Widest crop fed to the model; longer lines are squeezed rather than cut.
const MAX_W: usize = 48 * 40;
/// Crops of similar aspect ratio are batched so padding stays small.
const BATCH: usize = 8;

pub struct Recognized {
    pub text: String,
    pub confidence: f32,
}

/// Character table: index 0 is the CTC blank, then the dictionary, then space.
pub struct Charset {
    chars: Vec<String>,
}

impl Charset {
    #[must_use]
    pub fn from_dict(dict: &str) -> Self {
        let mut chars = vec![String::new()];
        chars.extend(dict.lines().map(str::to_owned));
        chars.push(" ".to_owned());
        Self { chars }
    }

    fn get(&self, index: usize) -> &str {
        self.chars.get(index).map_or("", String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.chars.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }
}

pub fn recognize(
    model: &Model,
    charset: &Charset,
    crops: &[RgbImage],
) -> Result<Vec<Recognized>, ImportError> {
    let mut order: Vec<usize> = (0..crops.len()).collect();
    let ratio = |i: usize| crops[i].width() as f32 / crops[i].height().max(1) as f32;
    order.sort_by(|&a, &b| ratio(a).total_cmp(&ratio(b)));

    let mut results: Vec<Option<Recognized>> = (0..crops.len()).map(|_| None).collect();
    for chunk in order.chunks(BATCH) {
        let widths: Vec<usize> = chunk
            .iter()
            .map(|&i| ((INPUT_H as f32 * ratio(i)).ceil() as usize).clamp(8, MAX_W))
            .collect();
        let batch_w = widths.iter().copied().max().unwrap_or(8).max(INPUT_H);
        let plane = INPUT_H * batch_w;
        let mut data = vec![0f32; chunk.len() * 3 * plane];
        for (b, (&i, &w)) in chunk.iter().zip(&widths).enumerate() {
            let resized =
                image::imageops::resize(&crops[i], w as u32, INPUT_H as u32, FilterType::Triangle);
            let base = b * 3 * plane;
            for (x, y, p) in resized.enumerate_pixels() {
                let at = y as usize * batch_w + x as usize;
                for c in 0..3 {
                    // BGR, scaled to [-1, 1]; padding stays 0 as in PaddleOCR.
                    data[base + c * plane + at] = f32::from(p[2 - c]) / 127.5 - 1.0;
                }
            }
        }
        let input = NdTensor::from_data([chunk.len(), 3, INPUT_H, batch_w], data);
        let out = model
            .run_one(input.view().into(), None)
            .map_err(|e| ImportError::Ocr(format!("文字识别推理失败：{e}")))?;
        let logits: NdTensor<f32, 3> = out
            .try_into()
            .map_err(|_| ImportError::Ocr("文字识别输出形状异常".into()))?;
        let [n, steps, classes] = logits.shape();
        if classes != charset.len() {
            return Err(ImportError::Ocr(format!(
                "识别模型类别数 {classes} 与字典 {} 不一致",
                charset.len()
            )));
        }
        let flat = logits.to_vec();
        for b in 0..n {
            let rows = &flat[b * steps * classes..(b + 1) * steps * classes];
            results[chunk[b]] = Some(ctc_greedy(rows, classes, charset));
        }
    }
    Ok(results
        .into_iter()
        .map(|r| {
            r.unwrap_or(Recognized {
                text: String::new(),
                confidence: 0.0,
            })
        })
        .collect())
}

/// Greedy CTC: best class per step, collapse repeats, drop blanks.
fn ctc_greedy(rows: &[f32], classes: usize, charset: &Charset) -> Recognized {
    let mut text = String::new();
    let mut probs = Vec::new();
    let mut prev = 0usize;
    for step in rows.chunks_exact(classes) {
        let (best, p) =
            step.iter().enumerate().fold(
                (0usize, f32::MIN),
                |acc, (i, &v)| if v > acc.1 { (i, v) } else { acc },
            );
        if best != 0 && best != prev {
            text.push_str(charset.get(best));
            probs.push(p);
        }
        prev = best;
    }
    let confidence = if probs.is_empty() {
        0.0
    } else {
        probs.iter().sum::<f32>() / probs.len() as f32
    };
    Recognized {
        text: text.trim().to_owned(),
        confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctc_collapses_repeats_and_blanks() {
        let charset = Charset::from_dict("a\nb");
        // classes: blank, a, b, space
        let rows = [
            0.0, 0.9, 0.0, 0.0, // a
            0.0, 0.9, 0.0, 0.0, // a (repeat)
            0.9, 0.0, 0.0, 0.0, // blank
            0.0, 0.8, 0.0, 0.0, // a
            0.0, 0.0, 0.0, 0.7, // space
            0.0, 0.0, 0.6, 0.0, // b
        ];
        let r = ctc_greedy(&rows, 4, &charset);
        assert_eq!(r.text, "aa b");
        assert!((r.confidence - 0.75).abs() < 1e-5);
    }
}
