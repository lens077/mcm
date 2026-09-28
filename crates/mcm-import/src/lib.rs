#![forbid(unsafe_code)]

//! Import diagrams drawn elsewhere (screenshots, exported HTML) into an MCM
//! outline, so they can be edited here and exported to XMind / Visio.
//!
//! Pipeline for images: local OCR → shape and connector analysis →
//! [`Diagram`] → [`mcm_core::Plan`] → canonical outline text. The outline goes
//! back through the normal parse/validate path, so imported content obeys the
//! same rules as hand-written plans (宪法 IV).

pub mod connectors;
pub mod diagram;
pub mod html;
pub mod ocr;
pub mod raster;
pub mod shapes;
pub mod to_plan;

use std::time::Instant;

pub use diagram::Diagram;
pub use to_plan::ImportReport;

/// Why an import could not produce an outline.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("无法解码图片：{0}")]
    Decode(String),
    #[error("OCR 失败：{0}")]
    Ocr(String),
    #[error("图中没有识别到任何方框或文字")]
    Empty,
    #[error("{0}")]
    Unsupported(String),
}

/// Result of importing one diagram.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Imported {
    /// Canonical outline text, ready for the editor or a `.mcm` file.
    pub outline: String,
    pub report: ImportReport,
    /// Intermediate structure, kept for diagnostics and tests.
    pub diagram: Diagram,
    pub elapsed_ms: u64,
}

/// Every intermediate product of the image pipeline, for debugging overlays.
pub struct ImageAnalysis {
    pub text: Vec<ocr::TextLine>,
    pub boxes: Vec<shapes::Shape>,
    pub frames: Vec<shapes::Shape>,
    pub diagram: Diagram,
}

/// Run OCR and shape analysis on decoded pixels.
///
/// # Errors
/// OCR engine failures.
pub fn analyse_image(img: &image::RgbImage) -> Result<ImageAnalysis, ImportError> {
    let text = ocr::OcrEngine::shared()?.read(img)?;
    let paper = raster::background(img);
    let text_rects: Vec<raster::Rect> = text.iter().map(|t| t.rect).collect();
    let ink = shapes::InkMap::new(img, paper, &text_rects);
    let all = shapes::detect_boxes(img, &ink);
    let (boxes, frames) = diagram::classify(&all);
    let box_rects: Vec<raster::Rect> = boxes.iter().map(|b| b.rect).collect();
    let frame_masks: Vec<(raster::Rect, raster::Rgb)> =
        frames.iter().map(|f| (f.rect, f.stroke)).collect();
    let links = connectors::detect_links(
        img,
        paper,
        &connectors::Masks {
            boxes: &box_rects,
            frames: &frame_masks,
            texts: &text_rects,
        },
    );
    let (boxes, links) = drop_chrome(boxes, links);
    let diagram = diagram::assemble(&boxes, &frames, &links, &text);
    Ok(ImageAnalysis {
        text,
        boxes,
        frames,
        diagram,
    })
}

/// Screenshots of diagram viewers carry the viewer's own panels (zoom bars,
/// toolbars): pale grey, unconnected boxes. They are not diagram content;
/// their text still surfaces as loose text, so nothing is lost.
fn drop_chrome(
    boxes: Vec<shapes::Shape>,
    links: Vec<connectors::Link>,
) -> (Vec<shapes::Shape>, Vec<connectors::Link>) {
    let is_chrome = |i: usize, s: &shapes::Shape| {
        let [r, g, b] = s.stroke.map(u32::from);
        let luminance = (299 * r + 587 * g + 114 * b) / 1000;
        let chroma = r.max(g).max(b) - r.min(g).min(b);
        let connected = links.iter().any(|l| l.from == i || l.to == i);
        !connected && luminance > 190 && chroma < 40
    };
    let keep: Vec<bool> = boxes
        .iter()
        .enumerate()
        .map(|(i, s)| !is_chrome(i, s))
        .collect();
    let mut remap = vec![usize::MAX; boxes.len()];
    let mut kept = Vec::new();
    for (i, b) in boxes.into_iter().enumerate() {
        if keep[i] {
            remap[i] = kept.len();
            kept.push(b);
        }
    }
    let links = links
        .into_iter()
        .map(|l| connectors::Link {
            from: remap[l.from],
            to: remap[l.to],
            ..l
        })
        .collect();
    (kept, links)
}

/// Import a PNG/JPEG/WebP/BMP diagram as an outline titled `title`.
///
/// # Errors
/// Undecodable bytes, OCR failure, or an image with nothing recognisable.
pub fn import_image(bytes: &[u8], title: &str) -> Result<Imported, ImportError> {
    let started = Instant::now();
    let img = raster::decode(bytes)?;
    let analysis = analyse_image(&img)?;
    if analysis.diagram.nodes.is_empty() && analysis.text.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(finish(analysis.diagram, title, started))
}

/// Import an archify-generated HTML diagram. The document's `<title>` wins
/// over `fallback_title` (usually the file name).
///
/// # Errors
/// [`ImportError::Unsupported`] for HTML without archify markup.
pub fn import_html(bytes: &[u8], fallback_title: &str) -> Result<Imported, ImportError> {
    let started = Instant::now();
    let text = String::from_utf8_lossy(bytes);
    let (diagram, title) = html::parse(&text)?;
    let title = if title.is_empty() {
        fallback_title
    } else {
        &title
    };
    Ok(finish(diagram, title, started))
}

fn finish(diagram: Diagram, title: &str, started: Instant) -> Imported {
    let (plan, report) = to_plan::to_plan(&diagram, title);
    Imported {
        outline: mcm_core::outline::serialize(&plan),
        report,
        diagram,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    }
}
