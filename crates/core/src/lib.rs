//! `voxelens-core` — the pure, WASM-compatible engine for reconstructing
//! Minecraft schematics from screenshots.
//!
//! Architecture rule: every algorithm lives here as a pure function over pixel
//! buffers and plain data structures — no file or DOM I/O. This keeps the core
//! identical under `cargo test`, the native CLI, and (later) WebAssembly. The
//! CLI and web front-end are thin I/O shells around this crate.
//!
//! Modules are added milestone by milestone (see `docs/ROADMAP.md`):
//! `schematic` (M1), `camera` (M2), `image` (M3), `faces` (M4),
//! `texture` (M5), `reconstruct` (M6).

#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    /// Smoke test proving the workspace compiles and the test harness runs.
    /// Replaced by real coverage starting in M1.
    #[test]
    fn workspace_builds() {
        assert_eq!(2 + 2, 4);
    }
}
