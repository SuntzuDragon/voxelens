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
pub mod image;
pub mod schematic;
pub mod segmentation;

pub use camera::{Camera, Ray};
pub use image::RgbImage;
pub use schematic::{SchematicOptions, VoxelModel};
pub use segmentation::{segment, Class, Segmentation};
