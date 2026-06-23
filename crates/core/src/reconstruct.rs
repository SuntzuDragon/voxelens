//! Single-view reconstruction.
//!
//! Principle: place a block only where its 3D position is *determinable* from
//! this one view — no speculation about occluded blocks. Today that means
//! anything anchored to the known ground plane: the trunk (its base sits on the
//! ground; its visible height is where the wood mask ends, i.e. where the canopy
//! starts occluding it). The canopy's depth is not single-view-determinable and
//! is deliberately left out until a second viewpoint — or an explicit
//! connectivity assumption — is available.

use nalgebra::Point3;

use crate::camera::Camera;
use crate::schematic::VoxelModel;
use crate::segmentation::{Class, Segmentation};

/// A reconstructed voxel model plus the world coordinates of its origin cell.
pub struct Reconstruction {
    pub model: VoxelModel,
    pub offset: [i32; 3],
}

impl Segmentation {
    /// Class at integer pixel `(x, y)`, or `None` if out of bounds.
    pub fn class_at(&self, x: i32, y: i32) -> Option<Class> {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return None;
        }
        Some(self.labels[y as usize * self.width as usize + x as usize])
    }
}

/// Reconstruct the visible, ground-anchored trunk as a column of `oak_log`.
/// Returns `None` if no trunk (wood) is visible.
pub fn reconstruct_trunk(
    seg: &Segmentation,
    cam: &Camera,
    ground_y: i32,
    max_height: u16,
) -> Option<Reconstruction> {
    let wood = seg.bbox(Class::Wood)?;

    // Ground cell: the trunk base = bottom-centre of the wood mask, back-projected
    // onto the known ground plane.
    let base = (((wood.x0 + wood.x1) / 2) as f64, wood.y1 as f64);
    let ray = cam.pixel_to_ray(base.0, base.1);
    let ground = cam.intersect_horizontal_plane(&ray, ground_y as f64)?;
    let (cx, cz) = (ground.x.floor() as i32, ground.z.floor() as i32);

    // Visible height: walk up the column, counting levels whose projected centre
    // is still classified wood (stops where the canopy occludes the trunk).
    let mut height: u16 = 0;
    for level in 0..max_height {
        let center = Point3::new(
            cx as f64 + 0.5,
            (ground_y + level as i32) as f64 + 0.5,
            cz as f64 + 0.5,
        );
        let Some((u, v)) = cam.world_to_pixel(center) else {
            break;
        };
        if seg.class_at(u.round() as i32, v.round() as i32) == Some(Class::Wood) {
            height = level + 1;
        } else {
            break;
        }
    }
    let height = height.max(1);

    let mut model = VoxelModel::new(1, height, 1).ok()?;
    for y in 0..height {
        model.set(0, y, 0, "minecraft:oak_log[axis=y]");
    }
    Some(Reconstruction {
        model,
        offset: [cx, ground_y, cz],
    })
}
