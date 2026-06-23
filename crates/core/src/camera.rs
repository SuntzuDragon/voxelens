//! Minecraft camera model: world <-> screen projection.
//!
//! Constants and conventions are verified against the **decompiled local
//! Minecraft jar** (1.21.10, Mojang-mapped), not web claims:
//!
//! - `GameRenderer.getProjectionMatrix(fov)` calls
//!   `Matrix4f.perspective(fov * PI/180, width/height, 0.05, far)`. JOML's first
//!   argument is the **vertical** field of view, so the in-game FOV slider is a
//!   vertical FOV in degrees, the aspect ratio is `width/height`, and the near
//!   plane is `0.05`. (`GameRenderer.PROJECTION_Z_NEAR = 0.05f`.)
//! - `Entity.calculateViewVector(pitch, yaw)` returns
//!   `(-sin(yaw)·cos(pitch), -sin(pitch), cos(yaw)·cos(pitch))` with angles in
//!   degrees — the look-direction convention (yaw 0 = +Z/south, increasing
//!   clockwise; positive pitch looks down).
//! - Eye height (`1.62`) is supplied by the caller via the eye position; it is
//!   validated empirically by the fixture projection test.
//!
//! Pixel mapping (square pixels, derived from the JOML perspective matrix with
//! `aspect = W/H`, so `fx == fy`):
//! ```text
//! fy = (H/2) / tan(vfov/2)
//! u  = W/2 + fy * (x_cam / depth)      // depth = distance along forward (> 0 if in front)
//! v  = H/2 - fy * (y_cam / depth)
//! ```

use nalgebra::{Point3, Vector3};

/// A ray in world space (origin at the camera eye, `dir` normalized).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Point3<f64>,
    pub dir: Vector3<f64>,
}

/// A perspective camera matching Minecraft's projection and view conventions.
#[derive(Debug, Clone)]
pub struct Camera {
    eye: Point3<f64>,
    /// Orthonormal view basis. `forward` is the look direction; `right` points
    /// to screen-right; `up` points to screen-up.
    forward: Vector3<f64>,
    right: Vector3<f64>,
    up: Vector3<f64>,
    width: f64,
    height: f64,
    /// Focal length in pixels; `fx == fy` for square pixels.
    fy: f64,
    fov_vertical_deg: f64,
}

/// Minecraft's `Entity.calculateViewVector(pitch, yaw)` (degrees).
fn mc_view_vector(yaw_deg: f64, pitch_deg: f64) -> Vector3<f64> {
    let yaw = yaw_deg.to_radians();
    let pitch = pitch_deg.to_radians();
    Vector3::new(
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    )
}

impl Camera {
    /// Build a camera from the in-game capture parameters.
    ///
    /// - `fov_vertical_deg`: the FOV slider value (vertical FOV in degrees).
    /// - `width`/`height`: the screenshot's pixel dimensions.
    /// - `eye`: camera eye position in world space (player feet + eye height).
    /// - `yaw_deg`/`pitch_deg`: facing, in Minecraft's convention.
    pub fn new(
        fov_vertical_deg: f64,
        width: u32,
        height: u32,
        eye: Point3<f64>,
        yaw_deg: f64,
        pitch_deg: f64,
    ) -> Self {
        let forward = mc_view_vector(yaw_deg, pitch_deg);
        // Standard look-at basis with world-up = +Y and no roll.
        let world_up = Vector3::new(0.0, 1.0, 0.0);
        let right = forward.cross(&world_up).normalize();
        let up = right.cross(&forward);
        let height_f = height as f64;
        let fy = (height_f / 2.0) / (fov_vertical_deg.to_radians() / 2.0).tan();
        Self {
            eye,
            forward,
            right,
            up,
            width: width as f64,
            height: height_f,
            fy,
            fov_vertical_deg,
        }
    }

    pub fn eye(&self) -> Point3<f64> {
        self.eye
    }

    pub fn forward(&self) -> Vector3<f64> {
        self.forward
    }

    pub fn fy(&self) -> f64 {
        self.fy
    }

    pub fn fov_vertical_deg(&self) -> f64 {
        self.fov_vertical_deg
    }

    pub fn width(&self) -> f64 {
        self.width
    }

    pub fn height(&self) -> f64 {
        self.height
    }

    /// Image point where lines parallel to world `axis` converge (the vanishing
    /// point), or `None` if it is at infinity (axis perpendicular to the view).
    /// The result is the same for `axis` and `-axis` (it's a line direction).
    pub fn vanishing_point(&self, axis: Vector3<f64>) -> Option<(f64, f64)> {
        let fd = self.forward.dot(&axis);
        if fd.abs() < 1e-9 {
            return None;
        }
        let u = self.width / 2.0 + self.fy * (self.right.dot(&axis) / fd);
        let v = self.height / 2.0 - self.fy * (self.up.dot(&axis) / fd);
        Some((u, v))
    }

    /// Horizontal field of view in degrees, derived from the vertical FOV and
    /// aspect ratio: `2·atan(tan(vfov/2) · W/H)`.
    pub fn horizontal_fov_deg(&self) -> f64 {
        let half = (self.fov_vertical_deg.to_radians() / 2.0).tan() * (self.width / self.height);
        2.0 * half.atan().to_degrees()
    }

    /// Project a world point to pixel coordinates, or `None` if it is at or
    /// behind the camera plane.
    pub fn world_to_pixel(&self, p: Point3<f64>) -> Option<(f64, f64)> {
        let rel = p - self.eye;
        let depth = self.forward.dot(&rel);
        if depth <= 0.0 {
            return None;
        }
        let x_cam = self.right.dot(&rel);
        let y_cam = self.up.dot(&rel);
        let u = self.width / 2.0 + self.fy * (x_cam / depth);
        let v = self.height / 2.0 - self.fy * (y_cam / depth);
        Some((u, v))
    }

    /// The world-space ray through a pixel (origin at the eye).
    pub fn pixel_to_ray(&self, u: f64, v: f64) -> Ray {
        let x = (u - self.width / 2.0) / self.fy;
        let y = (self.height / 2.0 - v) / self.fy;
        let dir = (self.forward + x * self.right + y * self.up).normalize();
        Ray {
            origin: self.eye,
            dir,
        }
    }

    /// Intersect a ray with the horizontal plane `y = plane_y`, returning the
    /// world point if the ray hits it in front of the origin.
    pub fn intersect_horizontal_plane(&self, ray: &Ray, plane_y: f64) -> Option<Point3<f64>> {
        if ray.dir.y.abs() < 1e-12 {
            return None;
        }
        let t = (plane_y - ray.origin.y) / ray.dir.y;
        if t <= 0.0 {
            return None;
        }
        Some(ray.origin + t * ray.dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn focal_length_matches_fov70() {
        // fy/H = 0.5 / tan(35°) = 0.71407...
        let cam = Camera::new(70.0, 2560, 1439, Point3::origin(), 0.0, 0.0);
        assert!(
            approx(cam.fy() / 1439.0, 0.714_074, 1e-5),
            "fy/H was {}",
            cam.fy() / 1439.0
        );
    }

    #[test]
    fn horizontal_fov_for_16_9() {
        // 70 vertical at exactly 16:9 -> ~102.46 horizontal (matches the wiki).
        let cam = Camera::new(70.0, 1920, 1080, Point3::origin(), 0.0, 0.0);
        assert!(
            approx(cam.horizontal_fov_deg(), 102.46, 0.05),
            "hfov was {}",
            cam.horizontal_fov_deg()
        );
    }

    #[test]
    fn forward_point_projects_to_image_center() {
        let cam = Camera::new(70.0, 2560, 1439, Point3::new(6.0, -54.38, 3.0), 129.0, 14.0);
        let ahead = cam.eye() + cam.forward() * 10.0;
        let (u, v) = cam.world_to_pixel(ahead).unwrap();
        assert!(
            approx(u, 1280.0, 1e-6) && approx(v, 719.5, 1e-6),
            "got ({u}, {v})"
        );
    }

    #[test]
    fn points_behind_camera_do_not_project() {
        let cam = Camera::new(70.0, 2560, 1439, Point3::new(6.0, -54.38, 3.0), 129.0, 14.0);
        let behind = cam.eye() - cam.forward() * 5.0;
        assert!(cam.world_to_pixel(behind).is_none());
    }

    #[test]
    fn pixel_to_ray_round_trips_with_world_to_pixel() {
        let cam = Camera::new(70.0, 2560, 1439, Point3::new(6.0, -54.38, 3.0), 129.0, 14.0);
        for &(u, v) in &[(1280.0, 719.5), (300.0, 1000.0), (2200.0, 200.0)] {
            let ray = cam.pixel_to_ray(u, v);
            // A point some distance along the ray must project back to (u, v).
            let p = ray.origin + ray.dir * 25.0;
            let (u2, v2) = cam.world_to_pixel(p).unwrap();
            assert!(
                approx(u, u2, 1e-6) && approx(v, v2, 1e-6),
                "({u},{v}) -> ({u2},{v2})"
            );
        }
    }

    #[test]
    fn ray_through_center_hits_ground_in_front() {
        let cam = Camera::new(70.0, 2560, 1439, Point3::new(6.0, -54.38, 3.0), 129.0, 14.0);
        let ray = cam.pixel_to_ray(1280.0, 719.5); // image center == forward ray
        let hit = cam.intersect_horizontal_plane(&ray, -60.0).unwrap();
        assert!(approx(hit.y, -60.0, 1e-9));
        // Looking down (pitch 14) and forward, the hit is ahead of the eye.
        assert!(cam.forward().dot(&(hit - cam.eye())) > 0.0);
    }

    /// Empirical end-to-end check against the actual fixture screenshot.
    ///
    /// Using the manifest's exact (flying) pose and the known world geometry,
    /// the trunk's base must project to where it visibly sits in the image.
    /// The trunk-base block is at `(-2,-60,-4)`; its bottom-face centre is
    /// `(-1.5,-60,-3.5)`. Measured by eye on the 2560x1439 fixture, the trunk
    /// base sits at roughly `(1300, 1000)` px. A wrong projection convention
    /// (FOV axis, yaw/pitch sign) would miss this by hundreds of pixels.
    #[test]
    fn fixture_pose_projects_trunk_base_to_screenshot_location() {
        let cam = Camera::new(70.0, 2560, 1439, Point3::new(6.0, -54.38, 3.0), 129.0, 14.0);
        let (u, v) = cam.world_to_pixel(Point3::new(-1.5, -60.0, -3.5)).unwrap();
        assert!(
            (u - 1300.0).abs() < 80.0 && (v - 1000.0).abs() < 80.0,
            "trunk base projected to ({u:.0}, {v:.0}), expected near (1300, 1000)"
        );
        // Sanity: a ground point below the eye and ahead, in the lower image half.
        assert!(
            v > cam.height / 2.0,
            "looking down, the ground sits below center"
        );
    }
}
