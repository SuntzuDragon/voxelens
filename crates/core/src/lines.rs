//! Hough line detection and orientation clustering.
//!
//! Turns the Canny edge map into straight lines (filtering grass-texture
//! speckle, which doesn't accumulate into line peaks), then groups the lines by
//! orientation. For an axis-aligned Minecraft build the block edges fall into
//! three families (one per world axis); for a small/distant object like the
//! wool tree the families are ~3 distinct image orientations (the vanishing
//! points are far off-screen). Camera-driven labeling of which cluster is which
//! world axis comes later, in face assembly.

use std::f32::consts::PI;

use crate::edges::Edges;

/// A line in normal form: `x·cos(theta) + y·sin(theta) = rho`.
#[derive(Debug, Clone, Copy)]
pub struct Line {
    /// Normal direction in radians, `[0, π)`.
    pub theta: f32,
    /// Signed distance from the origin (pixels).
    pub rho: f32,
    /// Number of edge pixels supporting the line.
    pub votes: u32,
}

impl Line {
    /// Orientation of the line itself (perpendicular to the normal), `[0, π)`.
    pub fn orientation(&self) -> f32 {
        (self.theta + PI / 2.0).rem_euclid(PI)
    }

    /// The two points where the line crosses the image border, for drawing.
    pub fn endpoints(&self, width: u32, height: u32) -> Option<((f32, f32), (f32, f32))> {
        let (c, s) = (self.theta.cos(), self.theta.sin());
        let (w, h) = (width as f32 - 1.0, height as f32 - 1.0);
        let mut pts: Vec<(f32, f32)> = Vec::new();
        let mut add = |x: f32, y: f32| {
            if (-0.5..=w + 0.5).contains(&x)
                && (-0.5..=h + 0.5).contains(&y)
                && !pts
                    .iter()
                    .any(|p| (p.0 - x).abs() < 1.0 && (p.1 - y).abs() < 1.0)
            {
                pts.push((x, y));
            }
        };
        if s.abs() > 1e-6 {
            add(0.0, self.rho / s); // x = 0
            add(w, (self.rho - w * c) / s); // x = w
        }
        if c.abs() > 1e-6 {
            add(self.rho / c, 0.0); // y = 0
            add((self.rho - h * s) / c, h); // y = h
        }
        (pts.len() >= 2).then(|| (pts[0], pts[1]))
    }
}

/// Detect lines via the classic Hough transform with peak non-max suppression.
/// Returns up to `max_lines` lines with at least `threshold` votes, strongest
/// first.
pub fn hough_lines(edges: &Edges, threshold: u32, max_lines: usize) -> Vec<Line> {
    const THETA_BINS: usize = 180;
    let (w, h) = (edges.width as f32, edges.height as f32);
    let diag = (w * w + h * h).sqrt();
    let rho_bins = (2.0 * diag).ceil() as usize + 1;

    let trig: Vec<(f32, f32)> = (0..THETA_BINS)
        .map(|t| {
            let a = PI * t as f32 / THETA_BINS as f32;
            (a.cos(), a.sin())
        })
        .collect();

    let mut acc = vec![0u32; rho_bins * THETA_BINS];
    for y in 0..edges.height {
        for x in 0..edges.width {
            if edges.at(x, y) {
                let (xf, yf) = (x as f32, y as f32);
                for (ti, &(c, s)) in trig.iter().enumerate() {
                    let ri = ((xf * c + yf * s + diag).round() as usize).min(rho_bins - 1);
                    acc[ri * THETA_BINS + ti] += 1;
                }
            }
        }
    }

    // Peaks = cells >= threshold that are local maxima in a 5x5 window.
    let mut peaks: Vec<(u32, usize, usize)> = Vec::new();
    for ri in 0..rho_bins {
        for ti in 0..THETA_BINS {
            let v = acc[ri * THETA_BINS + ti];
            if v < threshold {
                continue;
            }
            let mut is_max = true;
            'w: for dr in -2i32..=2 {
                for dt in -2i32..=2 {
                    let nr = ri as i32 + dr;
                    let nt = ti as i32 + dt;
                    if nr >= 0
                        && (nr as usize) < rho_bins
                        && nt >= 0
                        && (nt as usize) < THETA_BINS
                        && acc[nr as usize * THETA_BINS + nt as usize] > v
                    {
                        is_max = false;
                        break 'w;
                    }
                }
            }
            if is_max {
                peaks.push((v, ri, ti));
            }
        }
    }
    peaks.sort_by(|a, b| b.0.cmp(&a.0));
    peaks.truncate(max_lines);
    peaks
        .into_iter()
        .map(|(votes, ri, ti)| Line {
            theta: PI * ti as f32 / THETA_BINS as f32,
            rho: ri as f32 - diag,
            votes,
        })
        .collect()
}

/// A finite line segment with concrete endpoints.
#[derive(Debug, Clone, Copy)]
pub struct LineSegment {
    pub a: (f32, f32),
    pub b: (f32, f32),
    /// Normal direction of the parent line, `[0, π)`.
    pub theta: f32,
}

impl LineSegment {
    pub fn length(&self) -> f32 {
        ((self.a.0 - self.b.0).powi(2) + (self.a.1 - self.b.1).powi(2)).sqrt()
    }

    /// Orientation of the segment (perpendicular to the normal), `[0, π)`.
    pub fn orientation(&self) -> f32 {
        (self.theta + PI / 2.0).rem_euclid(PI)
    }
}

/// Walk each Hough line and cut it into concrete segments where the edge map
/// actually supports it: runs of edge-backed pixels (allowing gaps up to
/// `max_gap`), keeping those at least `min_len` long.
pub fn extract_segments(
    edges: &Edges,
    lines: &[Line],
    max_gap: i32,
    min_len: f32,
) -> Vec<LineSegment> {
    let (w, h) = (edges.width as i32, edges.height as i32);
    let supported = |x: f32, y: f32| -> bool {
        let (xi, yi) = (x.round() as i32, y.round() as i32);
        (-1..=1).any(|ox| {
            (-1..=1).any(|oy| {
                let (px, py) = (xi + ox, yi + oy);
                px >= 0 && px < w && py >= 0 && py < h && edges.at(px as u32, py as u32)
            })
        })
    };

    let mut segments = Vec::new();
    for line in lines {
        let (c, s) = (line.theta.cos(), line.theta.sin());
        let dir = (-s, c); // unit vector along the line
        let foot = (line.rho * c, line.rho * s); // line point nearest the origin
                                                 // Parameter range covering the image (project the corners onto `dir`).
        let (mut tmin, mut tmax) = (f32::MAX, f32::MIN);
        for &(cx, cy) in &[
            (0.0, 0.0),
            (w as f32, 0.0),
            (0.0, h as f32),
            (w as f32, h as f32),
        ] {
            let t = (cx - foot.0) * dir.0 + (cy - foot.1) * dir.1;
            tmin = tmin.min(t);
            tmax = tmax.max(t);
        }

        let point = |t: f32| (foot.0 + t * dir.0, foot.1 + t * dir.1);
        let mut run_start: Option<f32> = None;
        let mut last_support = tmin;
        let mut gap = 0;
        let mut t = tmin.floor();
        while t <= tmax {
            let (x, y) = point(t);
            if supported(x, y) {
                run_start.get_or_insert(t);
                last_support = t;
                gap = 0;
            } else if run_start.is_some() {
                gap += 1;
                if gap > max_gap {
                    let st = run_start.take().unwrap();
                    if last_support - st >= min_len {
                        segments.push(LineSegment {
                            a: point(st),
                            b: point(last_support),
                            theta: line.theta,
                        });
                    }
                }
            }
            t += 1.0;
        }
        if let Some(st) = run_start {
            if last_support - st >= min_len {
                segments.push(LineSegment {
                    a: point(st),
                    b: point(last_support),
                    theta: line.theta,
                });
            }
        }
    }
    segments
}

/// Circular distance between two orientations modulo π.
fn ori_dist(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(PI);
    d.min(PI - d)
}

/// Weight-weighted circular k-means over orientations (mod π). Returns a cluster
/// index per item and the cluster centroid orientations.
pub fn cluster_orientations(
    orientations: &[f32],
    weights: &[f32],
    k: usize,
    iterations: usize,
) -> (Vec<usize>, Vec<f32>) {
    let mut centroids: Vec<f32> = (0..k).map(|i| PI * i as f32 / k as f32).collect();
    let mut assign = vec![0usize; orientations.len()];

    for _ in 0..iterations {
        for (i, &o) in orientations.iter().enumerate() {
            assign[i] = (0..k)
                .min_by(|&a, &b| {
                    ori_dist(o, centroids[a])
                        .partial_cmp(&ori_dist(o, centroids[b]))
                        .unwrap()
                })
                .unwrap();
        }
        for (c, centroid) in centroids.iter_mut().enumerate() {
            // Weighted circular mean (double-angle trick for mod-π data).
            let (mut sx, mut sy) = (0.0f32, 0.0f32);
            for (i, &o) in orientations.iter().enumerate() {
                if assign[i] == c {
                    sx += weights[i] * (2.0 * o).cos();
                    sy += weights[i] * (2.0 * o).sin();
                }
            }
            if sx != 0.0 || sy != 0.0 {
                *centroid = (0.5 * sy.atan2(sx)).rem_euclid(PI);
            }
        }
    }
    (assign, centroids)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an edge map with a straight line drawn at the given normal form.
    fn edges_with_line(w: u32, h: u32, draw: impl Fn(u32, u32) -> bool) -> Edges {
        let mut data = vec![false; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                data[(y * w + x) as usize] = draw(x, y);
            }
        }
        Edges {
            width: w,
            height: h,
            data,
        }
    }

    #[test]
    fn finds_a_vertical_line() {
        // vertical line at x = 30 -> normal theta = 0, rho = 30
        let edges = edges_with_line(64, 64, |x, _| x == 30);
        let lines = hough_lines(&edges, 40, 4);
        assert!(!lines.is_empty(), "should find the line");
        let best = lines[0];
        assert!(
            best.theta < 0.05 || (best.theta - PI).abs() < 0.05,
            "theta {}",
            best.theta
        );
        assert!((best.rho.abs() - 30.0).abs() < 1.5, "rho {}", best.rho);
    }

    #[test]
    fn finds_a_horizontal_line() {
        // horizontal line at y = 20 -> normal theta = π/2, rho = 20
        let edges = edges_with_line(64, 64, |_, y| y == 20);
        let lines = hough_lines(&edges, 40, 4);
        assert!(
            (lines[0].theta - PI / 2.0).abs() < 0.05,
            "theta {}",
            lines[0].theta
        );
        assert!((lines[0].rho - 20.0).abs() < 1.5, "rho {}", lines[0].rho);
    }

    #[test]
    fn clusters_three_orientations() {
        // three line families: vertical, horizontal, 45°
        let mut data = vec![false; 80 * 80];
        for i in 0..80 {
            data[i * 80 + 20] = true; // vertical x=20
            data[i * 80 + 50] = true; // vertical x=50
            data[20 * 80 + i] = true; // horizontal y=20
            data[i * 80 + i] = true; // diagonal
        }
        let edges = Edges {
            width: 80,
            height: 80,
            data,
        };
        let lines = hough_lines(&edges, 30, 20);
        let oris: Vec<f32> = lines.iter().map(|l| l.orientation()).collect();
        let wts: Vec<f32> = lines.iter().map(|l| l.votes as f32).collect();
        let (assign, _) = cluster_orientations(&oris, &wts, 3, 10);
        // the two vertical lines must land in the same cluster
        let verticals: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.theta < 0.1 || (l.theta - PI).abs() < 0.1)
            .map(|(i, _)| assign[i])
            .collect();
        assert!(
            verticals.windows(2).all(|w| w[0] == w[1]),
            "verticals share a cluster"
        );
        // at least 2 distinct orientation clusters are populated
        let distinct: std::collections::HashSet<_> = assign.iter().collect();
        assert!(distinct.len() >= 2, "multiple orientation families");
    }

    #[test]
    fn extracts_a_partial_segment() {
        // a vertical edge that only spans y in 10..40 at x = 20
        let mut data = vec![false; 64 * 64];
        for y in 10..40 {
            data[y * 64 + 20] = true;
        }
        let edges = Edges {
            width: 64,
            height: 64,
            data,
        };
        let lines = hough_lines(&edges, 20, 4);
        let segments = extract_segments(&edges, &lines, 3, 10.0);
        let seg = segments
            .iter()
            .max_by(|a, b| a.length().partial_cmp(&b.length()).unwrap())
            .expect("a segment");
        assert!(
            (25.0..33.0).contains(&seg.length()),
            "length {}",
            seg.length()
        );
        let ys = [seg.a.1, seg.b.1];
        assert!(
            ys.iter().any(|&y| (y - 10.0).abs() < 3.0)
                && ys.iter().any(|&y| (y - 39.0).abs() < 3.0),
            "endpoints {:?} {:?}",
            seg.a,
            seg.b
        );
    }
}
