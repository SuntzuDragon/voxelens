//! A minimal, I/O-free RGB image buffer.
//!
//! Decoding/encoding lives in the I/O shells (CLI now, browser later); the core
//! operates only on decoded pixels so it stays pure and WASM-compatible.

/// A tightly-packed 8-bit RGB image, row-major (`3 * width * height` bytes).
#[derive(Debug, Clone)]
pub struct RgbImage {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl RgbImage {
    /// Wrap raw RGB bytes. Panics if `data.len() != 3 * width * height`.
    pub fn from_rgb(width: u32, height: u32, data: Vec<u8>) -> Self {
        assert_eq!(
            data.len(),
            3 * width as usize * height as usize,
            "RGB buffer length must be 3*width*height"
        );
        Self {
            width,
            height,
            data,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The RGB triple at `(x, y)`. Panics if out of bounds.
    #[inline]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        assert!(
            x < self.width && y < self.height,
            "({x}, {y}) out of bounds"
        );
        let i = 3 * (y as usize * self.width as usize + x as usize);
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    /// Convert to grayscale luminance (Rec. 601 weights), values in `0..=255`.
    pub fn to_grayscale(&self) -> GrayImage {
        let data = self
            .data
            .chunks_exact(3)
            .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
            .collect();
        GrayImage::from_data(self.width, self.height, data)
    }
}

/// A single-channel floating-point image (luminance `0..=255`), row-major.
#[derive(Debug, Clone)]
pub struct GrayImage {
    width: u32,
    height: u32,
    data: Vec<f32>,
}

impl GrayImage {
    /// Wrap raw luminance values. Panics if `data.len() != width * height`.
    pub fn from_data(width: u32, height: u32, data: Vec<f32>) -> Self {
        assert_eq!(
            data.len(),
            width as usize * height as usize,
            "gray buffer length must be width*height"
        );
        Self {
            width,
            height,
            data,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[f32] {
        &self.data
    }

    /// Luminance at `(x, y)`. Panics if out of bounds.
    #[inline]
    pub fn at(&self, x: u32, y: u32) -> f32 {
        self.data[y as usize * self.width as usize + x as usize]
    }
}

/// Convert an 8-bit RGB triple to HSV: hue in degrees `[0, 360)`, saturation and
/// value in `[0, 1]`.
pub fn rgb_to_hsv(rgb: [u8; 3]) -> (f32, f32, f32) {
    let r = rgb[0] as f32 / 255.0;
    let g = rgb[1] as f32 / 255.0;
    let b = rgb[2] as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_access() {
        let img = RgbImage::from_rgb(2, 1, vec![10, 20, 30, 40, 50, 60]);
        assert_eq!(img.pixel(0, 0), [10, 20, 30]);
        assert_eq!(img.pixel(1, 0), [40, 50, 60]);
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn pixel_out_of_bounds_panics() {
        RgbImage::from_rgb(1, 1, vec![0, 0, 0]).pixel(1, 0);
    }

    #[test]
    fn hsv_of_known_colors() {
        // pure red -> hue 0, full sat/value
        let (h, s, v) = rgb_to_hsv([255, 0, 0]);
        assert!(h.abs() < 0.1 && (s - 1.0).abs() < 1e-6 && (v - 1.0).abs() < 1e-6);
        // pure green -> hue 120
        assert!((rgb_to_hsv([0, 255, 0]).0 - 120.0).abs() < 0.1);
        // pure blue -> hue 240
        assert!((rgb_to_hsv([0, 0, 255]).0 - 240.0).abs() < 0.1);
        // gray -> zero saturation
        assert!(rgb_to_hsv([128, 128, 128]).1.abs() < 1e-6);
    }

    #[test]
    fn grayscale_uses_luma_weights() {
        let img = RgbImage::from_rgb(3, 1, vec![255, 0, 0, 0, 255, 0, 0, 0, 255]);
        let g = img.to_grayscale();
        assert!((g.at(0, 0) - 0.299 * 255.0).abs() < 0.5); // red
        assert!((g.at(1, 0) - 0.587 * 255.0).abs() < 0.5); // green
        assert!((g.at(2, 0) - 0.114 * 255.0).abs() < 0.5); // blue
    }
}
