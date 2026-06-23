//! Per-face texture classification.
//!
//! A rectified block face is matched against a set of known textures via
//! **rotation-aware normalized cross-correlation (NCC)** on luma. NCC is
//! invariant to affine brightness changes, so it shrugs off Minecraft's
//! directional face shading and biome tint (both ~multiplicative on luma). A
//! confidence threshold rejects unknown / custom blocks rather than forcing a
//! wrong label.
//!
//! The core is I/O-free: it works on `Tile`s (square luma buffers). Loading real
//! Minecraft textures into tiles is the CLI's job; tests use synthetic tiles.

/// A square texture tile stored as per-pixel luma.
#[derive(Clone, Debug)]
pub struct Tile {
    size: usize,
    luma: Vec<f32>,
}

impl Tile {
    /// Build from a `size × size` luma buffer (row-major).
    pub fn from_luma(size: usize, luma: Vec<f32>) -> Self {
        assert_eq!(luma.len(), size * size, "luma length must be size*size");
        Self { size, luma }
    }

    /// Build from a `size × size` RGB buffer (3 bytes/pixel, row-major), via
    /// the Rec.601 luma weights.
    pub fn from_rgb(size: usize, rgb: &[u8]) -> Self {
        assert_eq!(rgb.len(), size * size * 3, "rgb length must be size*size*3");
        let luma = rgb
            .chunks_exact(3)
            .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
            .collect();
        Self { size, luma }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn luma(&self) -> &[f32] {
        &self.luma
    }

    /// Rotate 90° clockwise.
    pub fn rot90(&self) -> Tile {
        let n = self.size;
        let mut out = vec![0.0; n * n];
        for y in 0..n {
            for x in 0..n {
                out[x * n + (n - 1 - y)] = self.luma[y * n + x];
            }
        }
        Tile { size: n, luma: out }
    }
}

/// Normalized cross-correlation of two equal-length signals, in `[-1, 1]`.
/// Returns 0 if either input has no variance (degenerate, e.g. a flat tile).
fn ncc(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    let ma = a.iter().sum::<f32>() / n;
    let mb = b.iter().sum::<f32>() / n;
    let (mut num, mut da, mut db) = (0.0f32, 0.0f32, 0.0f32);
    for (&x, &y) in a.iter().zip(b) {
        let (xa, yb) = (x - ma, y - mb);
        num += xa * yb;
        da += xa * xa;
        db += yb * yb;
    }
    if da <= 0.0 || db <= 0.0 {
        return 0.0;
    }
    num / (da.sqrt() * db.sqrt())
}

/// Best texture match for a face.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub name: String,
    /// NCC score in `[-1, 1]`; higher is a better match.
    pub score: f32,
    /// Face rotation (count of 90° CW turns) at which it matched.
    pub rotation: u8,
}

/// Classify `face` against `atlas`, trying all four 90° rotations. Returns the
/// best match, or `None` if its score is below `threshold` (unknown texture).
pub fn classify(face: &Tile, atlas: &[(String, Tile)], threshold: f32) -> Option<Match> {
    // The four rotations of the face.
    let mut rotations = Vec::with_capacity(4);
    let mut cur = face.clone();
    for _ in 0..4 {
        rotations.push(cur.clone());
        cur = cur.rot90();
    }

    let mut best: Option<Match> = None;
    for (name, tile) in atlas {
        if tile.size != face.size {
            continue;
        }
        for (r, rot) in rotations.iter().enumerate() {
            let s = ncc(&rot.luma, &tile.luma);
            if best.as_ref().is_none_or(|m| s > m.score) {
                best = Some(Match {
                    name: name.clone(),
                    score: s,
                    rotation: r as u8,
                });
            }
        }
    }
    best.filter(|m| m.score >= threshold)
}

/// Multiply every luma value by `k` (models shading / tint, ~multiplicative).
pub fn scale_brightness(t: &Tile, k: f32) -> Tile {
    Tile {
        size: t.size,
        luma: t.luma.iter().map(|v| v * k).collect(),
    }
}

/// Add `b` to every luma value (models an additive brightness offset).
pub fn shift_brightness(t: &Tile, b: f32) -> Tile {
    Tile {
        size: t.size,
        luma: t.luma.iter().map(|v| v + b).collect(),
    }
}

/// 3×3 box blur (models the resolution loss of distant / downscaled faces).
pub fn box_blur(t: &Tile) -> Tile {
    let n = t.size as isize;
    let mut out = vec![0.0; t.luma.len()];
    for y in 0..n {
        for x in 0..n {
            let (mut sum, mut cnt) = (0.0f32, 0.0f32);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (yy, xx) = (y + dy, x + dx);
                    if yy >= 0 && yy < n && xx >= 0 && xx < n {
                        sum += t.luma[(yy * n + xx) as usize];
                        cnt += 1.0;
                    }
                }
            }
            out[(y * n + x) as usize] = sum / cnt;
        }
    }
    Tile {
        size: t.size,
        luma: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 16;

    /// Vertical stripes (4 px period) — rotationally asymmetric, "log"-like.
    fn vstripes() -> Tile {
        let mut l = vec![0.0; N * N];
        for y in 0..N {
            for x in 0..N {
                l[y * N + x] = if (x / 2) % 2 == 0 { 40.0 } else { 200.0 };
            }
        }
        Tile::from_luma(N, l)
    }

    /// Smooth vertical gradient — distinct from stripes.
    fn gradient() -> Tile {
        let mut l = vec![0.0; N * N];
        for y in 0..N {
            for x in 0..N {
                l[y * N + x] = 20.0 + (y as f32 / N as f32) * 200.0;
            }
        }
        Tile::from_luma(N, l)
    }

    /// Deterministic pseudo-random noise — stands in for an unknown texture.
    fn noise() -> Tile {
        let mut s = 12345u32;
        let mut l = vec![0.0; N * N];
        for v in l.iter_mut() {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            *v = (s >> 24) as f32;
        }
        Tile::from_luma(N, l)
    }

    fn atlas() -> Vec<(String, Tile)> {
        vec![("log".into(), vstripes()), ("grad".into(), gradient())]
    }

    #[test]
    fn matches_itself_perfectly() {
        let m = classify(&vstripes(), &atlas(), 0.5).unwrap();
        assert_eq!(m.name, "log");
        assert!(m.score > 0.999, "score {}", m.score);
        assert_eq!(m.rotation, 0);
    }

    #[test]
    fn invariant_to_brightness_and_contrast() {
        // Affine luma change (shading + tint): (face + 35) * 0.4.
        let face = scale_brightness(&shift_brightness(&vstripes(), 35.0), 0.4);
        let m = classify(&face, &atlas(), 0.9).unwrap();
        assert_eq!(m.name, "log");
        assert!(
            m.score > 0.999,
            "NCC should be brightness-invariant: {}",
            m.score
        );
    }

    #[test]
    fn recovers_rotated_face() {
        let face = vstripes().rot90(); // horizontal stripes
        let m = classify(&face, &atlas(), 0.5).unwrap();
        assert_eq!(m.name, "log");
        assert!(m.score > 0.999, "score {}", m.score);
        assert_ne!(m.rotation, 0, "should report a non-zero rotation");
    }

    #[test]
    fn distinct_textures_not_confused() {
        assert_eq!(classify(&gradient(), &atlas(), 0.5).unwrap().name, "grad");
        assert_eq!(classify(&vstripes(), &atlas(), 0.5).unwrap().name, "log");
    }

    #[test]
    fn rejects_unknown_texture() {
        // Noise correlates with nothing in the atlas → below threshold.
        assert!(classify(&noise(), &atlas(), 0.7).is_none());
    }

    #[test]
    fn survives_blur() {
        let m = classify(&box_blur(&vstripes()), &atlas(), 0.6).unwrap();
        assert_eq!(m.name, "log");
    }
}
