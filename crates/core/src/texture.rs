//! Per-face texture classification.
//!
//! A rectified block face is matched against known textures using two
//! complementary cues:
//!   * **colour** — the face's brightness-normalised mean chromaticity. Dividing
//!     out brightness makes it invariant to Minecraft's directional face shading
//!     (a per-orientation brightness scale), and colour is the dominant, robust
//!     signal for distinctly-coloured blocks (e.g. the wool variants).
//!   * **structure** — rotation-aware normalised cross-correlation (NCC) on
//!     luma, which separates same-coloured blocks by pattern (e.g. oak_log vs
//!     oak_planks). NCC needs a precisely rectified face to be reliable.
//!
//! The two are blended by how *colourful* the face is: a saturated face is
//! judged mostly on colour, a near-grey face mostly on structure (so an unknown
//! grey patch with no matching pattern is correctly rejected). A confidence
//! threshold rejects unknown / custom blocks rather than forcing a label.
//!
//! Honest limitations (validated against real 1.21.10 jar textures, see the M5
//! notes): the brown family (log/dirt/planks) only separates once the structure
//! cue fires, which needs good rectification; biome-tinted blocks (grass,
//! leaves) need their tint applied to the texture before matching. Today the
//! reliable domain is distinctly-coloured, untinted blocks.

const NEUTRAL: [f32; 3] = [1.0 / 3.0; 3];
/// Chroma distance at which colour similarity falls to zero.
const CHROMA_SCALE: f32 = 0.25;
/// Face chroma saturation at which colour is fully trusted over structure.
const SAT_REF: f32 = 0.18;

/// A square texture tile: luma (for structure) plus mean chromaticity (colour).
#[derive(Clone, Debug)]
pub struct Tile {
    size: usize,
    luma: Vec<f32>,
    chroma: [f32; 3],
}

impl Tile {
    /// Build from a `size × size` luma buffer (row-major). Chromaticity is set
    /// neutral (grey) — such a tile is matched on structure alone.
    pub fn from_luma(size: usize, luma: Vec<f32>) -> Self {
        assert_eq!(luma.len(), size * size, "luma length must be size*size");
        Self {
            size,
            luma,
            chroma: NEUTRAL,
        }
    }

    /// Build from a `size × size` RGB buffer (3 bytes/pixel, row-major). Computes
    /// both the luma pattern and the brightness-normalised mean chromaticity.
    pub fn from_rgb(size: usize, rgb: &[u8]) -> Self {
        assert_eq!(rgb.len(), size * size * 3, "rgb length must be size*size*3");
        let luma = rgb
            .chunks_exact(3)
            .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
            .collect();
        let mut sum = [0.0f32; 3];
        for p in rgb.chunks_exact(3) {
            sum[0] += p[0] as f32;
            sum[1] += p[1] as f32;
            sum[2] += p[2] as f32;
        }
        let t = sum[0] + sum[1] + sum[2];
        let chroma = if t > 0.0 {
            [sum[0] / t, sum[1] / t, sum[2] / t]
        } else {
            NEUTRAL
        };
        Self { size, luma, chroma }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn luma(&self) -> &[f32] {
        &self.luma
    }

    pub fn chroma(&self) -> [f32; 3] {
        self.chroma
    }

    /// Rotate 90° clockwise. Chromaticity (a mean) is rotation-invariant.
    pub fn rot90(&self) -> Tile {
        let n = self.size;
        let mut out = vec![0.0; n * n];
        for y in 0..n {
            for x in 0..n {
                out[x * n + (n - 1 - y)] = self.luma[y * n + x];
            }
        }
        Tile {
            size: n,
            luma: out,
            chroma: self.chroma,
        }
    }
}

/// Normalized cross-correlation of two equal-length signals, in `[-1, 1]`.
/// Returns 0 if either input has no variance (e.g. a flat tile).
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

fn chroma_dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Best texture match for a face.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub name: String,
    /// Blended colour+structure confidence in `[0, 1]`; higher is better.
    pub score: f32,
    /// Face rotation (count of 90° CW turns) at which structure matched best.
    pub rotation: u8,
}

/// Classify `face` against `atlas`. Returns the best match, or `None` if its
/// blended confidence is below `threshold` (unknown / custom texture).
pub fn classify(face: &Tile, atlas: &[(String, Tile)], threshold: f32) -> Option<Match> {
    // Trust colour in proportion to how saturated (non-grey) the face is.
    let face_sat = chroma_dist(face.chroma, NEUTRAL);
    let w = (face_sat / SAT_REF).min(1.0);

    // Pre-rotate the face luma for the structure (NCC) cue.
    let mut lumas = Vec::with_capacity(4);
    let mut cur = face.clone();
    for _ in 0..4 {
        lumas.push(cur.luma.clone());
        cur = cur.rot90();
    }

    let mut best: Option<Match> = None;
    for (name, tile) in atlas {
        if tile.size != face.size {
            continue;
        }
        let color_sim = (1.0 - chroma_dist(face.chroma, tile.chroma) / CHROMA_SCALE).max(0.0);
        let (mut struct_sim, mut rotation) = (f32::MIN, 0u8);
        for (r, l) in lumas.iter().enumerate() {
            let s = ncc(l, &tile.luma);
            if s > struct_sim {
                struct_sim = s;
                rotation = r as u8;
            }
        }
        let score = w * color_sim + (1.0 - w) * struct_sim.max(0.0);
        if best.as_ref().is_none_or(|m| score > m.score) {
            best = Some(Match {
                name: name.clone(),
                score,
                rotation,
            });
        }
    }
    best.filter(|m| m.score >= threshold)
}

/// Multiply every luma value by `k` (models shading / tint, ~multiplicative).
pub fn scale_brightness(t: &Tile, k: f32) -> Tile {
    Tile {
        size: t.size,
        luma: t.luma.iter().map(|v| v * k).collect(),
        chroma: t.chroma,
    }
}

/// Add `b` to every luma value (models an additive brightness offset).
pub fn shift_brightness(t: &Tile, b: f32) -> Tile {
    Tile {
        size: t.size,
        luma: t.luma.iter().map(|v| v + b).collect(),
        chroma: t.chroma,
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
        chroma: t.chroma,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 16;

    /// Grey vertical stripes (4 px period) — rotationally asymmetric.
    fn vstripes() -> Tile {
        let mut l = vec![0.0; N * N];
        for y in 0..N {
            for x in 0..N {
                l[y * N + x] = if (x / 2).is_multiple_of(2) {
                    40.0
                } else {
                    200.0
                };
            }
        }
        Tile::from_luma(N, l)
    }

    /// Smooth grey vertical gradient — distinct from stripes.
    fn gradient() -> Tile {
        let mut l = vec![0.0; N * N];
        for y in 0..N {
            for x in 0..N {
                l[y * N + x] = 20.0 + (y as f32 / N as f32) * 200.0;
            }
        }
        Tile::from_luma(N, l)
    }

    /// Deterministic grey pseudo-random noise — an unknown texture.
    fn noise() -> Tile {
        let mut s = 12345u32;
        let mut l = vec![0.0; N * N];
        for v in l.iter_mut() {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            *v = (s >> 24) as f32;
        }
        Tile::from_luma(N, l)
    }

    fn rgb_tile(f: impl Fn(usize, usize) -> [u8; 3]) -> Tile {
        let mut buf = Vec::with_capacity(N * N * 3);
        for y in 0..N {
            for x in 0..N {
                buf.extend_from_slice(&f(x, y));
            }
        }
        Tile::from_rgb(N, &buf)
    }

    fn grey_atlas() -> Vec<(String, Tile)> {
        vec![("log".into(), vstripes()), ("grad".into(), gradient())]
    }

    #[test]
    fn matches_itself_perfectly() {
        let m = classify(&vstripes(), &grey_atlas(), 0.5).unwrap();
        assert_eq!(m.name, "log");
        assert!(m.score > 0.999, "score {}", m.score);
    }

    #[test]
    fn invariant_to_brightness_and_contrast() {
        // Affine luma change (shading): (face + 35) * 0.4. Grey tile → structure
        // path, and NCC is affine-invariant.
        let face = scale_brightness(&shift_brightness(&vstripes(), 35.0), 0.4);
        let m = classify(&face, &grey_atlas(), 0.9).unwrap();
        assert_eq!(m.name, "log");
        assert!(
            m.score > 0.999,
            "should be brightness-invariant: {}",
            m.score
        );
    }

    #[test]
    fn recovers_rotated_face() {
        let m = classify(&vstripes().rot90(), &grey_atlas(), 0.5).unwrap();
        assert_eq!(m.name, "log");
        assert!(m.score > 0.999, "score {}", m.score);
        assert_ne!(m.rotation, 0);
    }

    #[test]
    fn rejects_unknown_grey_texture() {
        // Grey + no matching pattern → both cues fail → rejected.
        assert!(classify(&noise(), &grey_atlas(), 0.7).is_none());
    }

    #[test]
    fn colour_separates_same_pattern() {
        // Same luma pattern, different colour (the wool-variant case that pure
        // luma-NCC cannot tell apart).
        let stripe = |x: usize| {
            if (x / 2).is_multiple_of(2) {
                120u8
            } else {
                180
            }
        };
        let green = rgb_tile(|x, _| [25, stripe(x), 20]);
        let red = rgb_tile(|x, _| [stripe(x), 30, 25]);
        let atlas = vec![
            ("green".to_string(), green.clone()),
            ("red".to_string(), red.clone()),
        ];
        assert_eq!(classify(&green, &atlas, 0.5).unwrap().name, "green");
        assert_eq!(classify(&red, &atlas, 0.5).unwrap().name, "red");
    }

    #[test]
    fn structure_breaks_same_colour_tie() {
        // Same brown colour, different pattern → the structure cue decides.
        let vlog = rgb_tile(|x, _| {
            if (x / 2).is_multiple_of(2) {
                [110, 80, 55]
            } else {
                [150, 110, 75]
            }
        });
        let brown_grad = rgb_tile(|_, y| {
            let v = (80 + y * 4) as u8;
            [v, (v as f32 * 0.72) as u8, (v as f32 * 0.5) as u8]
        });
        let atlas = vec![
            ("vlog".to_string(), vlog.clone()),
            ("grad".to_string(), brown_grad.clone()),
        ];
        assert_eq!(classify(&vlog, &atlas, 0.5).unwrap().name, "vlog");
    }
}
