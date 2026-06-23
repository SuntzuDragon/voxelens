//! voxelens CLI — thin I/O shell around `voxelens-core`.
//!
//! Subcommands are added milestone by milestone. The pipeline logic lives in the
//! core crate; this binary only does argument parsing and file I/O.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use nalgebra::Point3;
use voxelens_core::faces::Axis;
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

    /// Detect straight lines (Hough), colored by orientation cluster (M4).
    Lines {
        /// Input screenshot (PNG).
        input: PathBuf,
        /// Output PNG with detected lines drawn on the photo.
        #[arg(long, default_value = "out/lines.png")]
        out: PathBuf,
        #[arg(long, default_value_t = 1.4)]
        sigma: f32,
        #[arg(long, default_value_t = 40.0)]
        low: f32,
        #[arg(long, default_value_t = 90.0)]
        high: f32,
        /// Minimum Hough votes for a line.
        #[arg(long, default_value_t = 60)]
        threshold: u32,
        /// Maximum number of lines to keep.
        #[arg(long, default_value_t = 60)]
        max_lines: usize,
        /// Max gap (px) bridged within a segment.
        #[arg(long, default_value_t = 6)]
        max_gap: i32,
        /// Minimum segment length (px).
        #[arg(long, default_value_t = 25.0)]
        min_len: f32,
    },

    /// Color segments by their world axis via camera vanishing points (M4).
    /// Camera pose defaults to the wool-tree fixture.
    Axes {
        input: PathBuf,
        #[arg(long, default_value = "out/axes.png")]
        out: PathBuf,
        #[arg(long, default_value_t = 1.4)]
        sigma: f32,
        #[arg(long, default_value_t = 40.0)]
        low: f32,
        #[arg(long, default_value_t = 90.0)]
        high: f32,
        #[arg(long, default_value_t = 50)]
        threshold: u32,
        #[arg(long, default_value_t = 80)]
        max_lines: usize,
        #[arg(long, default_value_t = 6)]
        max_gap: i32,
        #[arg(long, default_value_t = 25.0)]
        min_len: f32,
        #[arg(long, default_value_t = 70.0)]
        fov: f64,
        #[arg(long, default_value_t = 129.0)]
        yaw: f64,
        #[arg(long, default_value_t = 14.0)]
        pitch: f64,
        #[arg(long, default_value_t = 6.0)]
        eye_x: f64,
        #[arg(long, default_value_t = -54.38)]
        eye_y: f64,
        #[arg(long, default_value_t = 3.0)]
        eye_z: f64,
        #[arg(long, default_value_t = 12.0)]
        max_err: f32,
    },

    /// Reconstruct the determinable (ground-anchored) trunk to a `.schem` (M6).
    /// Camera pose defaults to the wool-tree fixture.
    Reconstruct {
        input: PathBuf,
        #[arg(long, default_value = "out/reconstruction.schem")]
        out: PathBuf,
        #[arg(long, default_value_t = 70.0)]
        fov: f64,
        #[arg(long, default_value_t = 129.0)]
        yaw: f64,
        #[arg(long, default_value_t = 14.0)]
        pitch: f64,
        #[arg(long, default_value_t = 6.0)]
        eye_x: f64,
        #[arg(long, default_value_t = -54.38)]
        eye_y: f64,
        #[arg(long, default_value_t = 3.0)]
        eye_z: f64,
        #[arg(long, default_value_t = -60)]
        ground_y: i32,
        #[arg(long, default_value_t = 32)]
        max_height: u16,
        /// Minecraft DataVersion (4556 = 1.21.10).
        #[arg(long, default_value_t = 4556)]
        data_version: i32,
        #[arg(long, default_value_t = 3)]
        schem_version: u8,
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
        Command::Lines {
            input,
            out,
            sigma,
            low,
            high,
            threshold,
            max_lines,
            max_gap,
            min_len,
        } => lines_image(LinesArgs {
            input: &input,
            out: &out,
            sigma,
            low,
            high,
            threshold,
            max_lines,
            max_gap,
            min_len,
        }),
        Command::Axes {
            input,
            out,
            sigma,
            low,
            high,
            threshold,
            max_lines,
            max_gap,
            min_len,
            fov,
            yaw,
            pitch,
            eye_x,
            eye_y,
            eye_z,
            max_err,
        } => axes_image(AxesArgs {
            input: &input,
            out: &out,
            sigma,
            low,
            high,
            threshold,
            max_lines,
            max_gap,
            min_len,
            fov,
            yaw,
            pitch,
            eye: [eye_x, eye_y, eye_z],
            max_err,
        }),
        Command::Reconstruct {
            input,
            out,
            fov,
            yaw,
            pitch,
            eye_x,
            eye_y,
            eye_z,
            ground_y,
            max_height,
            data_version,
            schem_version,
        } => reconstruct_image(ReconstructArgs {
            input: &input,
            out: &out,
            fov,
            yaw,
            pitch,
            eye: [eye_x, eye_y, eye_z],
            ground_y,
            max_height,
            data_version,
            schem_version,
        }),
    }
}

struct ReconstructArgs<'a> {
    input: &'a Path,
    out: &'a Path,
    fov: f64,
    yaw: f64,
    pitch: f64,
    eye: [f64; 3],
    ground_y: i32,
    max_height: u16,
    data_version: i32,
    schem_version: u8,
}

fn reconstruct_image(args: ReconstructArgs) -> Result<()> {
    let decoded = image::open(args.input)
        .with_context(|| format!("decoding {}", args.input.display()))?
        .to_rgb8();
    let (w, h) = decoded.dimensions();
    let rgb = voxelens_core::RgbImage::from_rgb(w, h, decoded.into_raw());
    let seg = voxelens_core::segment(&rgb);
    let cam = voxelens_core::Camera::new(
        args.fov,
        w,
        h,
        Point3::new(args.eye[0], args.eye[1], args.eye[2]),
        args.yaw,
        args.pitch,
    );

    let recon =
        voxelens_core::reconstruct::reconstruct_trunk(&seg, &cam, args.ground_y, args.max_height)
            .ok_or_else(|| anyhow::anyhow!("no trunk (wood) detected in the image"))?;

    let version = match args.schem_version {
        2 => SchematicVersion::V2,
        3 => SchematicVersion::V3,
        other => bail!("unsupported schematic version {other} (expected 2 or 3)"),
    };
    let opts = SchematicOptions {
        data_version: args.data_version,
        offset: Some(recon.offset),
        name: Some("voxelens trunk".to_string()),
    };
    let bytes = to_schem(version, &recon.model, &opts)?;
    ensure_parent(args.out)?;
    std::fs::write(args.out, &bytes).with_context(|| format!("writing {}", args.out.display()))?;
    println!(
        "trunk {}x{}x{} at world {:?} -> {} ({} bytes)",
        recon.model.width(),
        recon.model.height(),
        recon.model.length(),
        recon.offset,
        args.out.display(),
        bytes.len()
    );
    Ok(())
}

struct AxesArgs<'a> {
    input: &'a Path,
    out: &'a Path,
    sigma: f32,
    low: f32,
    high: f32,
    threshold: u32,
    max_lines: usize,
    max_gap: i32,
    min_len: f32,
    fov: f64,
    yaw: f64,
    pitch: f64,
    eye: [f64; 3],
    max_err: f32,
}

fn axes_image(args: AxesArgs) -> Result<()> {
    let decoded = image::open(args.input)
        .with_context(|| format!("decoding {}", args.input.display()))?
        .to_rgb8();
    let (w, h) = decoded.dimensions();
    let rgb = voxelens_core::RgbImage::from_rgb(w, h, decoded.as_raw().clone());

    let edges = voxelens_core::edges::canny(&rgb.to_grayscale(), args.sigma, args.low, args.high);
    let lines = voxelens_core::lines::hough_lines(&edges, args.threshold, args.max_lines);
    let segments =
        voxelens_core::lines::extract_segments(&edges, &lines, args.max_gap, args.min_len);

    let cam = voxelens_core::Camera::new(
        args.fov,
        w,
        h,
        Point3::new(args.eye[0], args.eye[1], args.eye[2]),
        args.yaw,
        args.pitch,
    );
    let labels = voxelens_core::faces::assign_axes(&segments, &cam, args.max_err);

    let color = |axis: Option<Axis>| match axis {
        Some(Axis::X) => [255, 80, 80],
        Some(Axis::Y) => [80, 220, 90],
        Some(Axis::Z) => [90, 140, 255],
        None => [150, 150, 150],
    };
    let mut buf = decoded;
    let (mut nx, mut ny, mut nz, mut nn) = (0, 0, 0, 0);
    for (seg, &label) in segments.iter().zip(&labels) {
        draw_line(&mut buf, seg.a, seg.b, color(label));
        match label {
            Some(Axis::X) => nx += 1,
            Some(Axis::Y) => ny += 1,
            Some(Axis::Z) => nz += 1,
            None => nn += 1,
        }
    }

    ensure_parent(args.out)?;
    buf.save(args.out)
        .with_context(|| format!("writing {}", args.out.display()))?;
    println!(
        "segments: X={nx} Y={ny} Z={nz} none={nn} -> {}",
        args.out.display()
    );
    Ok(())
}

struct LinesArgs<'a> {
    input: &'a Path,
    out: &'a Path,
    sigma: f32,
    low: f32,
    high: f32,
    threshold: u32,
    max_lines: usize,
    max_gap: i32,
    min_len: f32,
}

/// Draw a thick line segment (Bresenham) onto an RGB buffer.
fn draw_line(buf: &mut image::RgbImage, a: (f32, f32), b: (f32, f32), color: [u8; 3]) {
    let (w, h) = (buf.width() as i32, buf.height() as i32);
    let (mut x0, mut y0) = (a.0.round() as i32, a.1.round() as i32);
    let (x1, y1) = (b.0.round() as i32, b.1.round() as i32);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        for ty in -1..=1 {
            for tx in -1..=1 {
                let (px, py) = (x0 + tx, y0 + ty);
                if px >= 0 && px < w && py >= 0 && py < h {
                    buf.put_pixel(px as u32, py as u32, image::Rgb(color));
                }
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn lines_image(args: LinesArgs) -> Result<()> {
    let decoded = image::open(args.input)
        .with_context(|| format!("decoding {}", args.input.display()))?
        .to_rgb8();
    let (w, h) = decoded.dimensions();
    let rgb = voxelens_core::RgbImage::from_rgb(w, h, decoded.as_raw().clone());

    let edges = voxelens_core::edges::canny(&rgb.to_grayscale(), args.sigma, args.low, args.high);
    let lines = voxelens_core::lines::hough_lines(&edges, args.threshold, args.max_lines);
    let segments =
        voxelens_core::lines::extract_segments(&edges, &lines, args.max_gap, args.min_len);

    let oris: Vec<f32> = segments.iter().map(|s| s.orientation()).collect();
    let weights: Vec<f32> = segments.iter().map(|s| s.length()).collect();
    let (assign, _) = voxelens_core::lines::cluster_orientations(&oris, &weights, 3, 12);

    let colors = [[255, 60, 60], [60, 220, 90], [90, 140, 255]];
    let mut buf = decoded;
    for (i, seg) in segments.iter().enumerate() {
        draw_line(
            &mut buf,
            seg.a,
            seg.b,
            colors[assign.get(i).copied().unwrap_or(0) % 3],
        );
    }

    ensure_parent(args.out)?;
    buf.save(args.out)
        .with_context(|| format!("writing {}", args.out.display()))?;
    println!(
        "{} lines, {} segments -> {}",
        lines.len(),
        segments.len(),
        args.out.display()
    );
    Ok(())
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
