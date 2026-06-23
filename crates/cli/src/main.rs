//! voxelens CLI — thin I/O shell around `voxelens-core`.
//!
//! Subcommands are added milestone by milestone. The pipeline logic lives in the
//! core crate; this binary only does argument parsing and file I/O.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use voxelens_core::schematic::{to_schem, SchematicOptions, SchematicVersion};
use voxelens_core::segmentation::{segment, Class};
use voxelens_core::VoxelModel;

#[derive(Parser)]
#[command(
    name = "voxelens",
    about = "Reconstruct Minecraft schematics from screenshots",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Emit a 1×2×1 oak-log column as a `.schem` (M1 end-to-end check).
    ///
    /// Load it in-game with WorldEdit: `//schem load <name>` then `//paste`.
    EmitTestSchem {
        /// Output path for the gzipped `.schem`.
        #[arg(long, default_value = "out/oak_column.schem")]
        out: PathBuf,
        /// Minecraft world DataVersion. Should match your target version.
        #[arg(long, default_value_t = 3700)]
        data_version: i32,
        /// Sponge schematic version to write (2 or 3). v3 is what current
        /// tooling expects; v2 has the broadest WorldEdit/FAWE reach.
        #[arg(long, default_value_t = 3)]
        schem_version: u8,
    },

    /// Segment a screenshot into sky/ground/wood/canopy and write a colorized
    /// label map (M3 stage dump).
    Segment {
        /// Input screenshot (PNG).
        input: PathBuf,
        /// Output colorized segmentation PNG.
        #[arg(long, default_value = "out/segmentation.png")]
        out: PathBuf,
    },

    /// Detect edges (Canny) and write a binary edge map (M4 stage dump).
    Edges {
        /// Input screenshot (PNG).
        input: PathBuf,
        /// Output edge-map PNG (white edges on black).
        #[arg(long, default_value = "out/edges.png")]
        out: PathBuf,
        /// Gaussian blur standard deviation.
        #[arg(long, default_value_t = 1.4)]
        sigma: f32,
        /// Canny low gradient-magnitude threshold.
        #[arg(long, default_value_t = 40.0)]
        low: f32,
        /// Canny high gradient-magnitude threshold.
        #[arg(long, default_value_t = 90.0)]
        high: f32,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::EmitTestSchem {
            out,
            data_version,
            schem_version,
        } => emit_test_schem(&out, data_version, schem_version),
        Command::Segment { input, out } => segment_image(&input, &out),
        Command::Edges {
            input,
            out,
            sigma,
            low,
            high,
        } => edges_image(&input, &out, sigma, low, high),
    }
}

fn edges_image(input: &Path, out: &Path, sigma: f32, low: f32, high: f32) -> Result<()> {
    let decoded = image::open(input)
        .with_context(|| format!("decoding {}", input.display()))?
        .to_rgb8();
    let (w, h) = decoded.dimensions();
    let rgb = voxelens_core::RgbImage::from_rgb(w, h, decoded.into_raw());

    let edges = voxelens_core::edges::canny(&rgb.to_grayscale(), sigma, low, high);
    let mut buf = image::GrayImage::new(w, h);
    for (i, &edge) in edges.data.iter().enumerate() {
        buf.put_pixel(
            i as u32 % w,
            i as u32 / w,
            image::Luma([if edge { 255 } else { 0 }]),
        );
    }

    ensure_parent(out)?;
    buf.save(out)
        .with_context(|| format!("writing {}", out.display()))?;
    println!("{} edge pixels -> {}", edges.count(), out.display());
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }
    }
    Ok(())
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

fn segment_image(input: &Path, out: &Path) -> Result<()> {
    let decoded = image::open(input)
        .with_context(|| format!("decoding {}", input.display()))?
        .to_rgb8();
    let (w, h) = decoded.dimensions();
    let rgb = voxelens_core::RgbImage::from_rgb(w, h, decoded.into_raw());

    let seg = segment(&rgb);
    let mut buf = image::RgbImage::new(w, h);
    for (i, &class) in seg.labels.iter().enumerate() {
        buf.put_pixel(i as u32 % w, i as u32 / w, image::Rgb(class_color(class)));
    }

    ensure_parent(out)?;
    buf.save(out)
        .with_context(|| format!("writing {}", out.display()))?;

    let total = (w * h) as f64;
    for class in [
        Class::Sky,
        Class::Ground,
        Class::Wood,
        Class::Canopy,
        Class::Other,
    ] {
        println!("{class:?}: {:.1}%", 100.0 * seg.count(class) as f64 / total);
    }
    println!("wrote {}", out.display());
    Ok(())
}

fn emit_test_schem(out: &Path, data_version: i32, schem_version: u8) -> Result<()> {
    let version = match schem_version {
        2 => SchematicVersion::V2,
        3 => SchematicVersion::V3,
        other => bail!("unsupported schematic version {other} (expected 2 or 3)"),
    };

    let mut model = VoxelModel::new(1, 2, 1)?;
    model.set(0, 0, 0, "minecraft:oak_log[axis=y]");
    model.set(0, 1, 0, "minecraft:oak_log[axis=y]");

    let opts = SchematicOptions {
        data_version,
        offset: None,
        name: None,
    };
    let bytes = to_schem(version, &model, &opts)?;

    ensure_parent(out)?;
    std::fs::write(out, &bytes).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "wrote {} bytes (v{}) -> {}",
        bytes.len(),
        schem_version,
        out.display()
    );
    Ok(())
}
