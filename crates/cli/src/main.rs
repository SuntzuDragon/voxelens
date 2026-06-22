//! voxelens CLI — thin I/O shell around `voxelens-core`.
//!
//! Subcommands are added milestone by milestone. The pipeline logic lives in the
//! core crate; this binary only does argument parsing and file I/O.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use voxelens_core::schematic::{to_schem, SchematicOptions, SchematicVersion};
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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::EmitTestSchem {
            out,
            data_version,
            schem_version,
        } => emit_test_schem(&out, data_version, schem_version),
    }
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

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }
    }
    std::fs::write(out, &bytes).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "wrote {} bytes (v{}) -> {}",
        bytes.len(),
        schem_version,
        out.display()
    );
    Ok(())
}
