//! Edge detection: Gaussian blur → Sobel gradients → Canny.
//!
//! Hand-written in pure Rust (no native CV dependency) so the core stays
//! WASM-safe and every stage is independently testable. Operates on
//! [`GrayImage`] (luminance), produced from an [`crate::image::RgbImage`] via
//! `to_grayscale`.

use crate::image::GrayImage;

/// Separable Gaussian blur with standard deviation `sigma` (edges clamped).
pub fn gaussian_blur(img: &GrayImage, sigma: f32) -> GrayImage {
    if sigma <= 0.0 {
        return img.clone();
    }
    let radius = (3.0 * sigma).ceil() as i32;
    let mut kernel: Vec<f32> = (-radius..=radius)
        .map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = kernel.iter().sum();
    for k in &mut kernel {
        *k /= sum;
    }

    let (w, h) = (img.width() as i32, img.height() as i32);
    let sample = |data: &[f32], x: i32, y: i32| -> f32 {
        data[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize]
    };

    // Horizontal then vertical pass.
    let src = img.data();
    let mut tmp = vec![0.0f32; src.len()];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for (k, &kv) in kernel.iter().enumerate() {
                acc += kv * sample(src, x + k as i32 - radius, y);
            }
            tmp[(y * w + x) as usize] = acc;
        }
    }
    let mut out = vec![0.0f32; src.len()];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for (k, &kv) in kernel.iter().enumerate() {
                acc += kv * sample(&tmp, x, y + k as i32 - radius);
            }
            out[(y * w + x) as usize] = acc;
        }
    }
    GrayImage::from_data(img.width(), img.height(), out)
}

/// Per-pixel Sobel gradient magnitude and orientation (`atan2(gy, gx)`, radians).
pub struct Gradients {
    pub width: u32,
    pub height: u32,
    pub magnitude: Vec<f32>,
    pub angle: Vec<f32>,
}

/// Compute Sobel gradients (edges clamped).
pub fn sobel(img: &GrayImage) -> Gradients {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let at = |x: i32, y: i32| img.at(x.clamp(0, w - 1) as u32, y.clamp(0, h - 1) as u32);
    let mut magnitude = vec![0.0f32; (w * h) as usize];
    let mut angle = vec![0.0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let gx = -at(x - 1, y - 1) - 2.0 * at(x - 1, y) - at(x - 1, y + 1)
                + at(x + 1, y - 1)
                + 2.0 * at(x + 1, y)
                + at(x + 1, y + 1);
            let gy = -at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + 2.0 * at(x, y + 1)
                + at(x + 1, y + 1);
            let i = (y * w + x) as usize;
            magnitude[i] = (gx * gx + gy * gy).sqrt();
            angle[i] = gy.atan2(gx);
        }
    }
    Gradients {
        width: img.width(),
        height: img.height(),
        magnitude,
        angle,
    }
}

/// A binary edge map.
pub struct Edges {
    pub width: u32,
    pub height: u32,
    pub data: Vec<bool>,
}

impl Edges {
    #[inline]
    pub fn at(&self, x: u32, y: u32) -> bool {
        self.data[y as usize * self.width as usize + x as usize]
    }

    pub fn count(&self) -> usize {
        self.data.iter().filter(|&&e| e).count()
    }
}

/// Canny edge detector: blur → Sobel → non-maximum suppression → double
/// threshold with hysteresis. `low`/`high` are gradient-magnitude thresholds.
pub fn canny(img: &GrayImage, sigma: f32, low: f32, high: f32) -> Edges {
    let grad = sobel(&gaussian_blur(img, sigma));
    let (w, h) = (grad.width as i32, grad.height as i32);
    let idx = |x: i32, y: i32| (y * w + x) as usize;

    // Non-maximum suppression: keep local maxima along the gradient direction
    // (quantized to 0/45/90/135°).
    let mut thin = vec![0.0f32; (w * h) as usize];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = idx(x, y);
            let m = grad.magnitude[i];
            let mut a = grad.angle[i].to_degrees();
            if a < 0.0 {
                a += 180.0;
            }
            let (n1, n2) = if !(22.5..157.5).contains(&a) {
                (grad.magnitude[idx(x - 1, y)], grad.magnitude[idx(x + 1, y)])
            } else if a < 67.5 {
                (
                    grad.magnitude[idx(x - 1, y + 1)],
                    grad.magnitude[idx(x + 1, y - 1)],
                )
            } else if a < 112.5 {
                (grad.magnitude[idx(x, y - 1)], grad.magnitude[idx(x, y + 1)])
            } else {
                (
                    grad.magnitude[idx(x - 1, y - 1)],
                    grad.magnitude[idx(x + 1, y + 1)],
                )
            };
            if m >= n1 && m >= n2 {
                thin[i] = m;
            }
        }
    }

    // Double threshold + hysteresis: flood-fill from strong edges through any
    // connected weak edges.
    let mut data = vec![false; (w * h) as usize];
    let mut stack: Vec<usize> = Vec::new();
    for (i, &m) in thin.iter().enumerate() {
        if m >= high {
            data[i] = true;
            stack.push(i);
        }
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i as i32 % w, i as i32 / w);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && nx < w && ny >= 0 && ny < h {
                    let j = idx(nx, ny);
                    if !data[j] && thin[j] >= low {
                        data[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
    }
    Edges {
        width: grad.width,
        height: grad.height,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertical_edge(w: usize, h: usize, split: usize) -> GrayImage {
        let mut data = vec![0.0f32; w * h];
        for y in 0..h {
            for x in split..w {
                data[y * w + x] = 255.0;
            }
        }
        GrayImage::from_data(w as u32, h as u32, data)
    }

    #[test]
    fn blur_preserves_a_flat_image() {
        let img = GrayImage::from_data(8, 8, vec![100.0; 64]);
        let out = gaussian_blur(&img, 1.5);
        assert!(out.data().iter().all(|&v| (v - 100.0).abs() < 1e-3));
    }

    #[test]
    fn sobel_detects_a_vertical_edge() {
        let g = sobel(&vertical_edge(10, 5, 5));
        let mag = |x: u32, y: u32| g.magnitude[(y * g.width + x) as usize];
        // Strong response straddling the boundary, ~flat away from it.
        assert!(mag(4, 2) > 100.0 || mag(5, 2) > 100.0, "edge response");
        assert!(mag(1, 2) < 1.0 && mag(8, 2) < 1.0, "flat regions");
        // Gradient points along x (angle ~0 or ~π).
        let a = g.angle[(2 * g.width + 5) as usize].abs();
        assert!(
            a < 0.25 || (a - std::f32::consts::PI).abs() < 0.25,
            "angle {a}"
        );
    }

    #[test]
    fn canny_finds_rectangle_borders_not_flat_areas() {
        let (w, h) = (40usize, 30usize);
        let mut data = vec![0.0f32; w * h];
        for y in 8..22 {
            for x in 10..30 {
                data[y * w + x] = 255.0;
            }
        }
        let edges = canny(
            &GrayImage::from_data(w as u32, h as u32, data),
            1.0,
            20.0,
            50.0,
        );

        assert!(edges.count() > 20, "should trace the border");
        // Flat solid interior and flat background carry no edges.
        assert!(!edges.at(20, 15), "interior flat");
        assert!(!edges.at(3, 3), "background flat");
        // Each side of the rectangle has an edge nearby.
        let near = |x: u32, y: u32| {
            (-1..=1).any(|dx| {
                (-1..=1).any(|dy| edges.at((x as i32 + dx) as u32, (y as i32 + dy) as u32))
            })
        };
        assert!(near(10, 15), "left border");
        assert!(near(29, 15), "right border");
        assert!(near(20, 8), "top border");
        assert!(near(20, 21), "bottom border");
    }
}
