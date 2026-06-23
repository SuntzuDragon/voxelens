//! End-to-end smoke tests for the `voxelens` binary.
//!
//! These spawn the real compiled CLI (via the cargo-provided `CARGO_BIN_EXE_*`
//! path), exercising `main.rs` itself — argument parsing, the version dispatch,
//! and file output — which unit tests of the core crate can't reach.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_voxelens")
}

fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("voxelens-cli-test-{}-{name}", std::process::id()))
}

#[test]
fn emits_a_gzipped_v3_schem_by_default() {
    let out = tmp_path("default.schem");
    let status = Command::new(bin())
        .args(["emit-test-schem", "--out", out.to_str().unwrap()])
        .status()
        .expect("failed to run voxelens");
    assert!(status.success(), "CLI should exit successfully");

    let bytes = std::fs::read(&out).expect("output file should exist");
    assert_eq!(
        &bytes[..2],
        &[0x1f, 0x8b],
        "output should be gzip-compressed"
    );
    assert!(bytes.len() > 50, "schematic should be non-trivial");

    std::fs::remove_file(&out).ok();
}

#[test]
fn emits_a_v2_schem_when_requested() {
    let out = tmp_path("v2.schem");
    let status = Command::new(bin())
        .args([
            "emit-test-schem",
            "--out",
            out.to_str().unwrap(),
            "--schem-version",
            "2",
        ])
        .status()
        .expect("failed to run voxelens");
    assert!(status.success());
    assert!(std::fs::metadata(&out).expect("output file").len() > 0);

    std::fs::remove_file(&out).ok();
}

#[test]
fn rejects_an_unknown_schematic_version() {
    let out = tmp_path("bad.schem");
    let status = Command::new(bin())
        .args([
            "emit-test-schem",
            "--out",
            out.to_str().unwrap(),
            "--schem-version",
            "9",
        ])
        .status()
        .expect("failed to run voxelens");
    assert!(
        !status.success(),
        "version 9 is unsupported and should fail"
    );

    std::fs::remove_file(&out).ok();
}
