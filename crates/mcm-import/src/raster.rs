//! Pixel access and the handful of colour measures the analysers share.

use image::RgbImage;

use crate::ImportError;

/// RGB colour.
pub type Rgb = [u8; 3];

/// Axis-aligned rectangle in source-image pixels, half-open on the far edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    #[must_use]
    pub fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    #[must_use]
    pub fn width(&self) -> i32 {
        self.x1 - self.x0
    }

    #[must_use]
    pub fn height(&self) -> i32 {
        self.y1 - self.y0
    }

    #[must_use]
    pub fn area(&self) -> i64 {
        i64::from(self.width().max(0)) * i64::from(self.height().max(0))
    }

    #[must_use]
    pub fn center(&self) -> (i32, i32) {
        ((self.x0 + self.x1) / 2, (self.y0 + self.y1) / 2)
    }

    #[must_use]
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }

    /// `other` lies entirely inside `self`.
    #[must_use]
    pub fn contains(&self, other: &Rect) -> bool {
        other.x0 >= self.x0 && other.y0 >= self.y0 && other.x1 <= self.x1 && other.y1 <= self.y1
    }

    #[must_use]
    pub fn inflate(&self, d: i32) -> Rect {
        Rect::new(self.x0 - d, self.y0 - d, self.x1 + d, self.y1 + d)
    }

    #[must_use]
    pub fn clamp_to(self, w: i32, h: i32) -> Rect {
        Rect::new(
            self.x0.max(0),
            self.y0.max(0),
            self.x1.min(w),
            self.y1.min(h),
        )
    }

    #[must_use]
    pub fn intersection_area(&self, other: &Rect) -> i64 {
        let w = (self.x1.min(other.x1) - self.x0.max(other.x0)).max(0);
        let h = (self.y1.min(other.y1) - self.y0.max(other.y0)).max(0);
        i64::from(w) * i64::from(h)
    }

    /// Chebyshev distance from a point to the rectangle (0 when inside).
    #[must_use]
    pub fn distance_to(&self, x: i32, y: i32) -> i32 {
        let dx = (self.x0 - x).max(x - (self.x1 - 1)).max(0);
        let dy = (self.y0 - y).max(y - (self.y1 - 1)).max(0);
        dx.max(dy)
    }
}

/// Decode PNG/JPEG/WebP/BMP bytes, flattening transparency onto white.
///
/// # Errors
/// Returns [`ImportError::Decode`] when the bytes are not a supported image.
pub fn decode(bytes: &[u8]) -> Result<RgbImage, ImportError> {
    let img = image::load_from_memory(bytes).map_err(|e| ImportError::Decode(e.to_string()))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w < 16 || h < 16 {
        return Err(ImportError::Decode(format!("图片过小（{w}×{h}）")));
    }
    let mut out = RgbImage::new(w, h);
    for (dst, src) in out.pixels_mut().zip(rgba.pixels()) {
        let a = u32::from(src[3]);
        for c in 0..3 {
            // Alpha-composite onto white: screenshots with transparent margins
            // must not read as black ink.
            let v = (u32::from(src[c]) * a + 255 * (255 - a)) / 255;
            dst[c] = u8::try_from(v).unwrap_or(255);
        }
    }
    Ok(out)
}

/// Largest per-channel difference — cheap, and matches how a person judges
/// "that line is clearly a different colour from the paper".
#[must_use]
pub fn color_dist(a: Rgb, b: Rgb) -> u8 {
    (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0)
}

/// The paper colour: the most common colour after 5-bit quantisation.
#[must_use]
pub fn background(img: &RgbImage) -> Rgb {
    let mut hist = vec![0u32; 1 << 15];
    let mut sums = vec![[0u64; 3]; 1 << 15];
    // Sampling every other pixel is plenty for a mode and halves the cost.
    for p in img.pixels().step_by(2) {
        let key =
            (usize::from(p[0] >> 3) << 10) | (usize::from(p[1] >> 3) << 5) | usize::from(p[2] >> 3);
        hist[key] += 1;
        for c in 0..3 {
            sums[key][c] += u64::from(p[c]);
        }
    }
    let (best, count) = hist
        .iter()
        .enumerate()
        .max_by_key(|&(_, n)| *n)
        .map(|(k, n)| (k, u64::from(*n)))
        .unwrap_or((0, 0));
    if count == 0 {
        return [255, 255, 255];
    }
    let mean = |c: usize| u8::try_from(sums[best][c] / count).unwrap_or(255);
    [mean(0), mean(1), mean(2)]
}

/// Pixel at `(x, y)`; callers guarantee bounds.
#[inline]
#[must_use]
pub fn px(img: &RgbImage, x: i32, y: i32) -> Rgb {
    #[allow(clippy::cast_sign_loss)]
    let p = img.get_pixel(x as u32, y as u32);
    [p[0], p[1], p[2]]
}

/// Mean colour of a set of samples.
#[must_use]
pub fn mean_color(samples: &[Rgb]) -> Option<Rgb> {
    if samples.is_empty() {
        return None;
    }
    let mut s = [0u64; 3];
    for p in samples {
        for c in 0..3 {
            s[c] += u64::from(p[c]);
        }
    }
    let n = samples.len() as u64;
    Some([0, 1, 2].map(|c| u8::try_from(s[c] / n).unwrap_or(255)))
}

/// Per-channel median — robust to a few foreign pixels (a crossing line).
#[must_use]
pub fn median_color(samples: &[Rgb]) -> Option<Rgb> {
    if samples.is_empty() {
        return None;
    }
    let mut out = [0u8; 3];
    for (c, slot) in out.iter_mut().enumerate() {
        let mut v: Vec<u8> = samples.iter().map(|p| p[c]).collect();
        let mid = v.len() / 2;
        *slot = *v.select_nth_unstable(mid).1;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_distance_is_zero_inside_and_chebyshev_outside() {
        let r = Rect::new(10, 10, 20, 20);
        assert_eq!(r.distance_to(15, 15), 0);
        assert_eq!(r.distance_to(5, 15), 5);
        assert_eq!(r.distance_to(25, 30), 11);
    }

    #[test]
    fn transparent_pixels_become_white() {
        let mut img = image::RgbaImage::new(20, 20);
        img.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let rgb = decode(&bytes).unwrap();
        assert_eq!(rgb.get_pixel(0, 0).0, [0, 0, 0]);
        assert_eq!(rgb.get_pixel(5, 5).0, [255, 255, 255]);
        assert_eq!(background(&rgb), [255, 255, 255]);
    }
}
