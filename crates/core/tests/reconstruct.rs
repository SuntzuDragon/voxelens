//! Fixture-based reconstruction test: the determinable, ground-anchored trunk.

use nalgebra::Point3;
use voxelens_core::reconstruct::reconstruct_trunk;
use voxelens_core::{segment, Camera, RgbImage};

fn load_fixture() -> RgbImage {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/wool_tree_superflat_fov70_2560x1439.png"
    );
    let img = image::open(path).expect("decode fixture").to_rgb8();
    let (w, h) = img.dimensions();
    RgbImage::from_rgb(w, h, img.into_raw())
}

#[test]
fn reconstructs_the_ground_anchored_trunk() {
    let img = load_fixture();
    let seg = segment(&img);
    // Recorded fixture pose.
    let cam = Camera::new(
        70.0,
        img.width(),
        img.height(),
        Point3::new(6.0, -54.38, 3.0),
        129.0,
        14.0,
    );

    let recon = reconstruct_trunk(&seg, &cam, -60, 32).expect("a trunk");

    // The ground-anchored cell matches the known trunk position (-2,-60,-4).
    assert!(
        (-3..=-1).contains(&recon.offset[0])
            && recon.offset[1] == -60
            && (-5..=-3).contains(&recon.offset[2]),
        "trunk cell {:?}",
        recon.offset
    );
    // A plausible visible height of solid oak logs.
    let h = recon.model.height();
    assert!((1..=6).contains(&h), "visible trunk height {h}");
    for y in 0..h {
        assert_eq!(recon.model.get(0, y, 0), "minecraft:oak_log[axis=y]");
    }
}
