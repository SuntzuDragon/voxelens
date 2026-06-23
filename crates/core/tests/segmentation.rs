//! Fixture-based segmentation tests. Decodes the committed screenshot (via the
//! `image` dev-dependency) and checks the colour classifier against the known
//! scene layout.

use voxelens_core::image::RgbImage;
use voxelens_core::segmentation::{segment, Class};

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
fn segments_scene_into_expected_regions() {
    let img = load_fixture();
    let (w, h) = (img.width(), img.height());
    let seg = segment(&img);

    let wood = seg.bbox(Class::Wood).expect("trunk present");
    let canopy = seg.bbox(Class::Canopy).expect("canopy present");
    let sky = seg.bbox(Class::Sky).expect("sky present");
    let ground = seg.bbox(Class::Ground).expect("ground present");

    // Trunk: a narrow vertical region, right of centre, in the lower-middle.
    assert!(
        (1200..=1420).contains(&wood.x0) && (1200..=1420).contains(&wood.x1),
        "trunk x off: {wood:?}"
    );
    assert!(wood.y0 > 700 && wood.y1 < 1100, "trunk y off: {wood:?}");
    assert!(
        wood.x1 - wood.x0 < 200,
        "trunk should be ~1 block wide: {wood:?}"
    );

    // Canopy: a blob above the trunk top and wider than the trunk, localized
    // near the tree (not smeared across the horizon).
    assert!(
        canopy.y0 < wood.y0,
        "canopy should start above the trunk top"
    );
    assert!(
        canopy.x0 < wood.x0 && canopy.x1 > wood.x1,
        "canopy should be wider than the trunk: {canopy:?}"
    );
    assert!(
        canopy.x0 > 800 && canopy.x1 < 1800,
        "canopy should be localized near the tree: {canopy:?}"
    );

    // Sky fills the top band; ground reaches the bottom edge.
    assert_eq!(sky.y0, 0, "sky should touch the top");
    assert!(
        sky.y1 < h * 2 / 3,
        "sky should stay in the upper image: {sky:?}"
    );
    assert_eq!(ground.y1, h - 1, "ground should reach the bottom");

    // The four real classes dominate; the leftover horizon sliver is small.
    assert!(
        seg.count(Class::Other) < seg.count(Class::Canopy),
        "unexpected amount of unclassified pixels"
    );
    assert_eq!(seg.labels.len(), (w * h) as usize);
}
