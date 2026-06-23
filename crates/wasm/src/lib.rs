//! WebAssembly bindings for `voxelens-core` — run the pipeline client-side.
//!
//! Functions take an RGBA byte buffer (as produced by a canvas `getImageData`)
//! plus dimensions, and return either an RGBA overlay (for display) or the
//! gzipped `.schem` bytes (for download). No file or network I/O.

use nalgebra::Point3;
use wasm_bindgen::prelude::*;

use voxelens_core::schematic::{to_schem, SchematicOptions, SchematicVersion};
use voxelens_core::{reconstruct_trunk, segment, Camera, Class, RgbImage};

fn rgba_to_rgb(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect()
}

fn class_color(class: Class) -> [u8; 3] {
    match class {
        Class::Sky => [120, 170, 255],
        Class::Ground => [70, 150, 60],
        Class::Wood => [130, 80, 40],
        Class::Canopy => [30, 90, 25],
        Class::Other => [255, 0, 255],
    }
}

/// Segment an RGBA image into scene classes; returns an RGBA overlay.
#[wasm_bindgen]
pub fn segment_rgba(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let rgb = RgbImage::from_rgb(width, height, rgba_to_rgb(rgba));
    let seg = segment(&rgb);
    let mut out = Vec::with_capacity(rgba.len());
    for &class in &seg.labels {
        let [r, g, b] = class_color(class);
        out.extend_from_slice(&[r, g, b, 255]);
    }
    out
}

/// Reconstruct the determinable (ground-anchored) trunk; returns the gzipped
/// Sponge `.schem` bytes, or a JS error if no trunk is visible.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn reconstruct_schem(
    rgba: &[u8],
    width: u32,
    height: u32,
    fov: f64,
    yaw: f64,
    pitch: f64,
    eye_x: f64,
    eye_y: f64,
    eye_z: f64,
    ground_y: i32,
    max_height: u16,
    data_version: i32,
    schem_version: u8,
) -> Result<Vec<u8>, JsError> {
    let rgb = RgbImage::from_rgb(width, height, rgba_to_rgb(rgba));
    let seg = segment(&rgb);
    let cam = Camera::new(
        fov,
        width,
        height,
        Point3::new(eye_x, eye_y, eye_z),
        yaw,
        pitch,
    );
    let recon = reconstruct_trunk(&seg, &cam, ground_y, max_height)
        .ok_or_else(|| JsError::new("no trunk (wood) detected in the image"))?;
    let version = match schem_version {
        2 => SchematicVersion::V2,
        _ => SchematicVersion::V3,
    };
    let opts = SchematicOptions {
        data_version,
        offset: Some(recon.offset),
        name: Some("voxelens".to_string()),
    };
    to_schem(version, &recon.model, &opts).map_err(|e| JsError::new(&e.to_string()))
}
