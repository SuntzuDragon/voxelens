//! `voxelens-core` — the pure, WASM-compatible engine for reconstructing
//! Minecraft schematics from screenshots.
//!
//! Architecture rule: every algorithm lives here as a pure function over pixel
//! buffers and plain data structures — no file or DOM I/O. This keeps the core
//! identical under `cargo test`, the native CLI, and (later) WebAssembly. The
//! CLI and web front-end are thin I/O shells around this crate.
//!
//! Modules are added milestone by milestone (see `docs/ROADMAP.md`).

#![forbid(unsafe_code)]

pub mod camera;
pub mod edges;
pub mod faces;
pub mod image;
pub mod lines;
pub mod reconstruct;
pub mod schematic;
pub mod segmentation;
pub mod texture;

pub use camera::{Camera, Ray};
pub use edges::{canny, Edges};
pub use faces::{assign_axes, Axis};
pub use image::{GrayImage, RgbImage};
pub use lines::{cluster_orientations, extract_segments, hough_lines, Line, LineSegment};
pub use reconstruct::{reconstruct_trunk, Reconstruction};
pub use schematic::{SchematicOptions, VoxelModel};
pub use segmentation::{segment, Class, Segmentation};
pub use texture::{classify, Match, Tile};
