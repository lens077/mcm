//! Local OCR: PaddleOCR PP-OCRv6 models on the pure-Rust `rten` engine.
//!
//! Two recognizers share one detector:
//! - [`OcrModel::Fast`] (default): PP-OCRv6 tiny, compiled into the binary,
//!   works offline out of the box.
//! - [`OcrModel::Accurate`]: PP-OCRv6 small, 21 MB, downloaded on request
//!   and verified against [`crate::models::ACCURATE_FILES`].
//!
//! Both behave identically on every platform (宪法 I). Selection rationale
//! and benchmarks: `docs/image-import.md`.

mod det;
mod rec;

use std::path::Path;
use std::sync::OnceLock;

use image::RgbImage;
use rten::Model;

use crate::ImportError;
use crate::models::{ACCURATE_FILES, read_verified};
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

/// Which recognizer reads the text. Detection is the same for both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrModel {
    /// PP-OCRv6 tiny: built in, about 0.3 s for a typical screenshot.
    #[default]
    Fast,
    /// PP-OCRv6 small: separate download, about twice as slow, fewer
    /// case and punctuation slips.
    Accurate,
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

    /// Engine with the downloaded PP-OCRv6 small recognizer from `dir`.
    /// Every file is checked against its pinned digest before use, so a
    /// truncated or tampered download is refused rather than misread.
    ///
    /// # Errors
    /// [`ImportError::Model`] when files are missing or do not match.
    pub fn accurate(dir: &Path) -> Result<Self, ImportError> {
        let [rec_file, dict_file] = &ACCURATE_FILES;
        let rec_bytes = read_verified(dir, rec_file)?;
        let dict_bytes = read_verified(dir, dict_file)?;
        let dict = String::from_utf8(dict_bytes)
            .map_err(|_| ImportError::Model(format!("{} 不是 UTF-8 文本", dict_file.name)))?;
        let det = Model::load_static_slice(DET_ONNX)
            .map_err(|e| ImportError::Ocr(format!("检测模型加载失败：{e}")))?;
        let rec = Model::load(rec_bytes)
            .map_err(|e| ImportError::Model(format!("高精度识别模型加载失败：{e}")))?;
        Ok(Self {
            det,
            rec,
            charset: Charset::from_dict(&dict),
        })
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
