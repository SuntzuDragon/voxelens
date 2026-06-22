//! Sponge Schematic **version 2** serialization.
//!
//! Verified against the Sponge Schematic Specification v2 and the WorldEdit
//! reference writer (`SpongeSchematicV2Writer`):
//! - the root NBT compound is named `"Schematic"` and holds the fields directly;
//! - `PaletteMax` is the palette entry count;
//! - `BlockData` is a byte array of unsigned-LEB128 palette indices ordered
//!   `x + z * Width + y * Width * Length`.
//!
//! NBT is big-endian (Java edition) and the `.schem` file is gzip-compressed.

use std::collections::BTreeMap;
use std::io::Write;

use fastnbt::{ByteArray, IntArray, SerOpts};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use super::varint;
use super::VoxelModel;

const SPONGE_SCHEMATIC_VERSION: i32 = 2;
const ROOT_NAME: &str = "Schematic";

/// Options controlling schematic output.
#[derive(Debug, Clone)]
pub struct SchematicOptions {
    /// Minecraft world data version. It must match the target version so the
    /// game's data-fixer interprets block states correctly. Do **not** hardcode
    /// this for real output — read it from the rendering jar's `version.json`.
    pub data_version: i32,
    /// Optional paste offset relative to the paste origin.
    pub offset: Option<[i32; 3]>,
    /// Optional schematic name, stored under `Metadata`.
    pub name: Option<String>,
}

/// The on-disk schematic structure. Field order here is the NBT field order
/// (serde serializes struct fields in declaration order), which keeps the
/// uncompressed output byte-for-byte deterministic.
#[derive(Debug, Serialize, Deserialize)]
struct SchematicV2 {
    #[serde(rename = "Version")]
    version: i32,
    #[serde(rename = "DataVersion")]
    data_version: i32,
    #[serde(rename = "Width")]
    width: i16,
    #[serde(rename = "Height")]
    height: i16,
    #[serde(rename = "Length")]
    length: i16,
    #[serde(rename = "Offset", default, skip_serializing_if = "Option::is_none")]
    offset: Option<IntArray>,
    #[serde(rename = "PaletteMax")]
    palette_max: i32,
    #[serde(rename = "Palette")]
    palette: BTreeMap<String, i32>,
    #[serde(rename = "BlockData")]
    block_data: ByteArray,
    #[serde(rename = "Metadata", default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Metadata>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Metadata {
    #[serde(rename = "Name", default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

/// Error serializing a schematic.
#[derive(Debug)]
pub enum SchemError {
    /// NBT serialization failed.
    Nbt(fastnbt::error::Error),
    /// Gzip compression failed.
    Io(std::io::Error),
}

impl std::fmt::Display for SchemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemError::Nbt(e) => write!(f, "NBT serialization error: {e}"),
            SchemError::Io(e) => write!(f, "gzip error: {e}"),
        }
    }
}

impl std::error::Error for SchemError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SchemError::Nbt(e) => Some(e),
            SchemError::Io(e) => Some(e),
        }
    }
}

fn build(model: &VoxelModel, opts: &SchematicOptions) -> SchematicV2 {
    // BlockData: one varint per cell, already in x + z*W + y*W*L order.
    let mut block_bytes = Vec::with_capacity(model.volume());
    for &index in model.block_indices() {
        varint::write_unsigned(index, &mut block_bytes);
    }
    let block_data = ByteArray::new(block_bytes.into_iter().map(|b| b as i8).collect());

    // Palette: blockstate -> its index (which is its position in the palette).
    let mut palette = BTreeMap::new();
    for (i, state) in model.palette().iter().enumerate() {
        palette.insert(state.clone(), i as i32);
    }

    SchematicV2 {
        version: SPONGE_SCHEMATIC_VERSION,
        data_version: opts.data_version,
        // u16 -> i16 is a bit-reinterpretation: the spec treats these as
        // unsigned shorts, so dimensions above 32767 round-trip correctly.
        width: model.width() as i16,
        height: model.height() as i16,
        length: model.length() as i16,
        offset: opts.offset.map(|o| IntArray::new(o.to_vec())),
        palette_max: model.palette().len() as i32,
        palette,
        block_data,
        metadata: opts.name.clone().map(|name| Metadata { name: Some(name) }),
    }
}

/// Serialize to uncompressed Sponge v2 NBT bytes (root compound named
/// `"Schematic"`). Mostly useful for testing; real files are gzipped via
/// [`to_schem_v2`].
pub fn to_nbt_v2(model: &VoxelModel, opts: &SchematicOptions) -> Result<Vec<u8>, SchemError> {
    let schem = build(model, opts);
    fastnbt::to_bytes_with_opts(&schem, SerOpts::new().root_name(ROOT_NAME))
        .map_err(SchemError::Nbt)
}

/// Serialize to a gzip-compressed `.schem` file (Sponge v2), loadable with
/// WorldEdit's `//schem load`.
pub fn to_schem_v2(model: &VoxelModel, opts: &SchematicOptions) -> Result<Vec<u8>, SchemError> {
    let nbt = to_nbt_v2(model, opts)?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&nbt).map_err(SchemError::Io)?;
    encoder.finish().map_err(SchemError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DATA_VERSION: i32 = 3700;

    fn oak_column() -> VoxelModel {
        let mut m = VoxelModel::new(1, 2, 1).unwrap();
        m.set(0, 0, 0, "minecraft:oak_log[axis=y]");
        m.set(0, 1, 0, "minecraft:oak_log[axis=y]");
        m
    }

    fn opts() -> SchematicOptions {
        SchematicOptions {
            data_version: TEST_DATA_VERSION,
            offset: None,
            name: None,
        }
    }

    fn gunzip(bytes: &[u8]) -> Vec<u8> {
        use std::io::Read;
        let mut decoder = flate2::read::GzDecoder::new(bytes);
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn nbt_round_trips() {
        let nbt = to_nbt_v2(&oak_column(), &opts()).unwrap();
        let parsed: SchematicV2 = fastnbt::from_bytes(&nbt).unwrap();

        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.data_version, TEST_DATA_VERSION);
        assert_eq!((parsed.width, parsed.height, parsed.length), (1, 2, 1));
        assert_eq!(parsed.palette_max, 2);
        assert_eq!(parsed.palette.get(AIR_KEY), Some(&0));
        assert_eq!(parsed.palette.get("minecraft:oak_log[axis=y]"), Some(&1));
        assert!(parsed.offset.is_none());
        assert!(parsed.metadata.is_none());

        let raw: Vec<u8> = parsed.block_data.iter().map(|&b| b as u8).collect();
        assert_eq!(varint::read_all_unsigned(&raw).unwrap(), vec![1, 1]);
    }

    const AIR_KEY: &str = "minecraft:air";

    #[test]
    fn root_compound_is_named_schematic() {
        let nbt = to_nbt_v2(&oak_column(), &opts()).unwrap();
        assert_eq!(nbt[0], 0x0a, "TAG_Compound");
        assert_eq!(&nbt[1..3], &[0x00, 0x09], "root name length 9");
        assert_eq!(&nbt[3..12], b"Schematic", "root name");
    }

    #[test]
    fn schem_is_gzipped_nbt() {
        let model = oak_column();
        let o = opts();
        let nbt = to_nbt_v2(&model, &o).unwrap();
        let schem = to_schem_v2(&model, &o).unwrap();
        assert_eq!(&schem[0..2], &[0x1f, 0x8b], "gzip magic");
        assert_eq!(gunzip(&schem), nbt);
    }

    #[test]
    fn offset_and_metadata_are_emitted_when_set() {
        let o = SchematicOptions {
            data_version: TEST_DATA_VERSION,
            offset: Some([1, 2, 3]),
            name: Some("col".to_string()),
        };
        let nbt = to_nbt_v2(&oak_column(), &o).unwrap();
        let parsed: SchematicV2 = fastnbt::from_bytes(&nbt).unwrap();
        assert_eq!(
            parsed.offset.unwrap().iter().copied().collect::<Vec<i32>>(),
            vec![1, 2, 3]
        );
        assert_eq!(parsed.metadata.unwrap().name.as_deref(), Some("col"));
    }

    /// Pins the exact uncompressed bytes against a committed fixture (whose
    /// correctness was verified by hand against the Sponge v2 spec). Catches any
    /// future drift in field order, types, or palette ordering.
    #[test]
    fn matches_golden_nbt() {
        let nbt = to_nbt_v2(&oak_column(), &opts()).unwrap();
        let golden = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/golden/oak_column_v2.nbt"
        ));
        assert_eq!(
            nbt.as_slice(),
            &golden[..],
            "uncompressed NBT drifted from the committed golden fixture"
        );
    }
}
