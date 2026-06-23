//! Block-face detection.
//!
//! Step one: assign each detected line segment to the world axis whose
//! **vanishing point** it points toward (using the camera). This is robust to
//! the perspective "fanning" that makes raw orientation clustering split a
//! single axis — e.g. the vertical (world-Y) edges all converge on the same
//! vanishing point even though their image orientations differ.

use std::f32::consts::PI;

use nalgebra::Vector3;

use crate::camera::Camera;
use crate::lines::LineSegment;

/// A world axis (+X east, +Y up, +Z south).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

fn ori_diff(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(PI);
    d.min(PI - d)
}

/// Label each segment with the world axis whose vanishing point best matches
/// the segment's orientation, or `None` if no axis is within `max_err_deg`.
pub fn assign_axes(segments: &[LineSegment], cam: &Camera, max_err_deg: f32) -> Vec<Option<Axis>> {
    let vps = [
        (Axis::X, cam.vanishing_point(Vector3::x())),
        (Axis::Y, cam.vanishing_point(Vector3::y())),
        (Axis::Z, cam.vanishing_point(Vector3::z())),
    ];
    let max_err = max_err_deg.to_radians();

    segments
        .iter()
        .map(|seg| {
            let mid = ((seg.a.0 + seg.b.0) / 2.0, (seg.a.1 + seg.b.1) / 2.0);
            let seg_ori = seg.orientation();
            let mut best: Option<(Axis, f32)> = None;
            for (axis, vp) in vps {
                let Some((vx, vy)) = vp else { continue };
                // Expected orientation: the direction from the segment's
                // midpoint toward this axis's vanishing point.
                let expected = (vy as f32 - mid.1).atan2(vx as f32 - mid.0).rem_euclid(PI);
                let d = ori_diff(seg_ori, expected);
                if best.is_none_or(|(_, e)| d < e) {
                    best = Some((axis, d));
                }
            }
            best.filter(|&(_, e)| e <= max_err).map(|(a, _)| a)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Point3;

    fn fixture_cam() -> Camera {
        Camera::new(70.0, 2560, 1439, Point3::new(6.0, -54.38, 3.0), 129.0, 14.0)
    }

    fn seg(cam: &Camera, a: [f64; 3], b: [f64; 3]) -> LineSegment {
        let pa = cam.world_to_pixel(Point3::new(a[0], a[1], a[2])).unwrap();
        let pb = cam.world_to_pixel(Point3::new(b[0], b[1], b[2])).unwrap();
        let (a, b) = ((pa.0 as f32, pa.1 as f32), (pb.0 as f32, pb.1 as f32));
        let theta = ((b.1 - a.1).atan2(b.0 - a.0) - PI / 2.0).rem_euclid(PI);
        LineSegment { a, b, theta }
    }

    #[test]
    fn vanishing_points_are_distinct_and_sensible() {
        let cam = fixture_cam();
        // Looking down slightly, the vertical axis converges far below the frame.
        let vy = cam.vanishing_point(Vector3::y()).unwrap();
        assert!((vy.0 - 1280.0).abs() < 1.0, "Y vp x ~ center: {vy:?}");
        assert!(vy.1 > cam.height(), "Y vp below the image: {vy:?}");
    }

    #[test]
    fn assigns_segments_to_their_world_axis() {
        let cam = fixture_cam();
        // Edges of a block near the tree, each along one world axis.
        let y_seg = seg(&cam, [-1.0, -60.0, -3.0], [-1.0, -56.0, -3.0]);
        let x_seg = seg(&cam, [-2.0, -59.0, -3.0], [-1.0, -59.0, -3.0]);
        let z_seg = seg(&cam, [-1.0, -59.0, -4.0], [-1.0, -59.0, -3.0]);
        let labels = assign_axes(&[y_seg, x_seg, z_seg], &cam, 10.0);
        assert_eq!(labels, vec![Some(Axis::Y), Some(Axis::X), Some(Axis::Z)]);
    }
}
