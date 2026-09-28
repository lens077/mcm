//! Local OCR: PaddleOCR PP-OCRv6 tiny models on the pure-Rust `rten` engine.
//!
//! The models are compiled into the binary, so recognition works offline and
//! identically on every platform (宪法 I、本地优先). Selection rationale and
//! the benchmark behind it: `docs/image-import.md`.

mod det;
mod rec;

use std::sync::OnceLock;

use image::RgbImage;
use rten::Model;

use crate::ImportError;
use crate::raster::Rect;

pub use rec::Charset;

static DET_ONNX: &[u8] = include_bytes!("../../models/pp-ocrv6_tiny_det.onnx");
static REC_ONNX: &[u8] = include_bytes!("../../models/pp-ocrv6_tiny_rec.onnx");
static DICT: &str = include_str!("../../models/ppocrv6_tiny_dict.txt");

/// Lines scoring below this are almost always icons or arrowheads read as
/// glyphs ("0", "A", "口"); real diagram text scores above 0.8.
const MIN_CONFIDENCE: f32 = 0.65;
/// Single characters need more certainty — a lone "-" is usually a dash of a line.
const MIN_SINGLE_CHAR_CONFIDENCE: f32 = 0.9;

/// One recognised line of text.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TextLine {
    pub rect: Rect,
    pub text: String,
    pub confidence: f32,
}

pub struct OcrEngine {
    det: Model,
    rec: Model,
    charset: Charset,
}

impl OcrEngine {
    /// The process-wide engine; model parsing happens once (~15 ms).
    ///
    /// # Errors
    /// Fails only if the embedded models are corrupt.
    pub fn shared() -> Result<&'static OcrEngine, ImportError> {
        static ENGINE: OnceLock<Result<OcrEngine, String>> = OnceLock::new();
        ENGINE
            .get_or_init(|| OcrEngine::load().map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| ImportError::Ocr(e.clone()))
    }

    fn load() -> Result<Self, ImportError> {
        let det = Model::load_static_slice(DET_ONNX)
            .map_err(|e| ImportError::Ocr(format!("检测模型加载失败：{e}")))?;
        let rec = Model::load_static_slice(REC_ONNX)
            .map_err(|e| ImportError::Ocr(format!("识别模型加载失败：{e}")))?;
        Ok(Self {
            det,
            rec,
            charset: Charset::from_dict(DICT),
        })
    }

    /// Detect and recognise every text line, in reading order.
    ///
    /// # Errors
    /// Propagates inference failures.
    pub fn read(&self, img: &RgbImage) -> Result<Vec<TextLine>, ImportError> {
        let detections = det::detect(&self.det, img)?;
        let crops: Vec<RgbImage> = detections
            .iter()
            .map(|d| {
                let r = d.rect;
                #[allow(clippy::cast_sign_loss)]
                image::imageops::crop_imm(
                    img,
                    r.x0 as u32,
                    r.y0 as u32,
                    r.width() as u32,
                    r.height() as u32,
                )
                .to_image()
            })
            .collect();
        let recognized = rec::recognize(&self.rec, &self.charset, &crops)?;
        let mut lines: Vec<TextLine> = detections
            .into_iter()
            .zip(recognized)
            .filter(|(_, r)| keep(&r.text, r.confidence))
            .map(|(d, r)| TextLine {
                rect: d.rect,
                text: r.text,
                confidence: r.confidence,
            })
            .collect();
        lines.sort_by_key(|l| (l.rect.y0, l.rect.x0));
        Ok(lines)
    }
}

fn keep(text: &str, confidence: f32) -> bool {
    let chars = text.chars().filter(|c| !c.is_whitespace()).count();
    match chars {
        0 => false,
        1 => confidence >= MIN_SINGLE_CHAR_CONFIDENCE,
        _ => confidence >= MIN_CONFIDENCE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_filter_prefers_long_confident_text() {
        assert!(keep("internal/server", 0.99));
        assert!(!keep("A", 0.63));
        assert!(!keep("0", 0.47));
        assert!(keep("?", 0.95));
        assert!(!keep("口", 0.77));
        assert!(!keep("  ", 1.0));
        assert!(!keep("Buf", 0.5));
    }
}
