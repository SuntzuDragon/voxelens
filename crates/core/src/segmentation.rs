//! Per-pixel scene segmentation by colour.
//!
//! HSV thresholds calibrated on the wool-tree fixture (sampled colours):
//! sky `(219°, 0.46, 0.98)`, grass `(87°, 0.52, 0.43)`, green-wool
//! `(~79°, 0.72, 0.25)`, oak-log `(35°, 0.51, 0.23)`. Grass and the green wool
//! sit at nearly the same hue, so they separate on **value + saturation**: the
//! wool is markedly darker and more saturated than the grass.
//!
//! Limitation: dark, saturated grass near the far horizon can still be mistaken
//! for canopy; cleaning that up needs connected-component / spatial reasoning
//! (a later step), not colour alone.

use crate::image::{rgb_to_hsv, RgbImage};

/// Scene class for a pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Sky,
    Ground,
    /// Tree trunk (oak log).
    Wood,
    /// Canopy (green wool, in this fixture).
    Canopy,
    Other,
}

/// Classify a single RGB pixel.
pub fn classify(rgb: [u8; 3]) -> Class {
    let (h, s, v) = rgb_to_hsv(rgb);
    if v > 0.70 && (195.0..=250.0).contains(&h) {
        Class::Sky
    } else if (15.0..=50.0).contains(&h) && v < 0.40 {
        Class::Wood
    } else if (60.0..=110.0).contains(&h) && v < 0.35 && s > 0.58 {
        // green wool: same hue as grass, but darker and more saturated
        Class::Canopy
    } else if (60.0..=115.0).contains(&h) && s > 0.30 {
        Class::Ground
    } else {
        Class::Other
    }
}

/// An inclusive axis-aligned pixel bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BBox {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

/// A per-pixel label map (`width * height`, row-major).
pub struct Segmentation {
    pub width: u32,
    pub height: u32,
    pub labels: Vec<Class>,
}

impl Segmentation {
    pub fn count(&self, class: Class) -> usize {
        self.labels.iter().filter(|&&l| l == class).count()
    }

    /// Bounding box of all pixels of `class`, or `None` if there are none.
    pub fn bbox(&self, class: Class) -> Option<BBox> {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        let mut any = false;
        for (i, &l) in self.labels.iter().enumerate() {
            if l == class {
                any = true;
                let x = i as u32 % self.width;
                let y = i as u32 / self.width;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
        any.then_some(BBox { x0, y0, x1, y1 })
    }
}

/// Classify every pixel of an image.
pub fn segment(img: &RgbImage) -> Segmentation {
    let mut labels = Vec::with_capacity((img.width() * img.height()) as usize);
    for y in 0..img.height() {
        for x in 0..img.width() {
            labels.push(classify(img.pixel(x, y)));
        }
    }
    Segmentation {
        width: img.width(),
        height: img.height(),
        labels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_sampled_fixture_colours() {
        assert_eq!(classify([133, 174, 249]), Class::Sky);
        assert_eq!(classify([85, 110, 53]), Class::Ground);
        assert_eq!(classify([50, 64, 17]), Class::Canopy);
        assert_eq!(classify([59, 47, 29]), Class::Wood);
    }
}
