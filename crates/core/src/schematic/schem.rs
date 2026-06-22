//! Sponge Schematic serialization (versions **2** and **3**).
//!
//! Verified against the Sponge Schematic Specification and against real `.schem`
//! files produced by current tooling:
//! - **v2** — root NBT compound named `"Schematic"` with the fields directly
//!   inside; `PaletteMax` is the palette entry count; `BlockData` is a byte
//!   array of unsigned-LEB128 palette indices ordered `x + z*W + y*W*L`. Read by
//!   essentially every WorldEdit/FAWE install.
//! - **v3** — an *unnamed* root compound holding a single `"Schematic"` child;
//!   blocks live under a `Blocks` compound as `Palette` (blockstate → int) and
//!   `Data` (the same varint byte array). What current tooling writes; required
//!   by some modern viewers. (Note: the real field names are `Palette`/`Data`,
//!   not `BlockPalette` — confirmed by decoding a real v3 file.)
//!
//! NBT is big-endian (Java edition); the `.schem` file is gzip-compressed.

use std::collections::BTreeMap;
use std::io::Write;

use fastnbt::{ByteArray, IntArray, SerOpts};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use super::varint;
use super::VoxelModel;

/// The v2 root compound name (WorldEdit's de-facto convention).
const V2_ROOT_NAME: &str = "Schematic";

/// Sponge schematic format version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchematicVersion {
    /// Version 2: root compound named `Schematic`, fields at root. Broadest
    /// WorldEdit/FAWE compatibility.
    V2,
    /// Version 3: unnamed root with a `Schematic` child and a `Blocks` compound.
    /// What current tooling writes; required by some modern viewers.
    V3,
}

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

// --- On-disk structures -----------------------------------------------------
// Field order here is the NBT field order (serde serializes struct fields in
// declaration order), keeping uncompressed output byte-for-byte deterministic.

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
    #[serde(rename = "BlockEntities")]
    block_entities: Vec<BlockEntity>,
    #[serde(rename = "Metadata", default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Metadata>,
}

/// Sponge v3 file: an unnamed root compound whose sole `Schematic` child holds
/// the data.
#[derive(Debug, Serialize, Deserialize)]
struct SchematicV3File {
    #[serde(rename = "Schematic")]
    schematic: SchematicV3Body,
}

#[derive(Debug, Serialize, Deserialize)]
struct SchematicV3Body {
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
    #[serde(rename = "Metadata", default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Metadata>,
    #[serde(rename = "Blocks")]
    blocks: BlocksV3,
}

#[derive(Debug, Serialize, Deserialize)]
struct BlocksV3 {
    #[serde(rename = "Palette")]
    palette: BTreeMap<String, i32>,
    #[serde(rename = "Data")]
    data: ByteArray,
    #[serde(rename = "BlockEntities")]
    block_entities: Vec<BlockEntity>,
}

/// A block entity (chest, sign, ...). We never emit any yet, but the (empty)
/// `BlockEntities` list is always written: the spec marks it optional, yet some
/// readers (e.g. schemat.io) reject files that omit it, and real tools include
/// it.
#[derive(Debug, Serialize, Deserialize)]
struct BlockEntity {}

#[derive(Debug, Serialize, Deserialize)]
struct Metadata {
    #[serde(rename = "Name", default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl Metadata {
    fn named(name: String) -> Self {
        Metadata { name: Some(name) }
    }
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

// --- Builders ---------------------------------------------------------------

/// Build the shared palette compound and varint `Data`/`BlockData` byte array.
fn palette_and_data(model: &VoxelModel) -> (BTreeMap<String, i32>, ByteArray) {
    // One varint per cell, already in x + z*W + y*W*L order.
    let mut block_bytes = Vec::with_capacity(model.volume());
    for &index in model.block_indices() {
        varint::write_unsigned(index, &mut block_bytes);
    }
    let data = ByteArray::new(block_bytes.into_iter().map(|b| b as i8).collect());

    // blockstate -> its index (its position in the palette).
    let mut palette = BTreeMap::new();
    for (i, state) in model.palette().iter().enumerate() {
        palette.insert(state.clone(), i as i32);
    }
    (palette, data)
}

/// `u16 -> i16` is a bit-reinterpretation: the spec treats these as unsigned
/// shorts, so dimensions above 32767 round-trip correctly.
fn dims(model: &VoxelModel) -> (i16, i16, i16) {
    (
        model.width() as i16,
        model.height() as i16,
        model.length() as i16,
    )
}

fn build_v2(model: &VoxelModel, opts: &SchematicOptions) -> SchematicV2 {
    let (palette, block_data) = palette_and_data(model);
    let (width, height, length) = dims(model);
    SchematicV2 {
        version: 2,
        data_version: opts.data_version,
        width,
        height,
        length,
        offset: opts.offset.map(|o| IntArray::new(o.to_vec())),
        palette_max: model.palette().len() as i32,
        palette,
        block_data,
        block_entities: Vec::new(),
        metadata: opts.name.clone().map(Metadata::named),
    }
}

fn build_v3(model: &VoxelModel, opts: &SchematicOptions) -> SchematicV3File {
    let (palette, data) = palette_and_data(model);
    let (width, height, length) = dims(model);
    SchematicV3File {
        schematic: SchematicV3Body {
            version: 3,
            data_version: opts.data_version,
            width,
            height,
            length,
            offset: opts.offset.map(|o| IntArray::new(o.to_vec())),
            metadata: opts.name.clone().map(Metadata::named),
            blocks: BlocksV3 {
                palette,
                data,
                block_entities: Vec::new(),
            },
        },
    }
}

fn gzip(nbt: Vec<u8>) -> Result<Vec<u8>, SchemError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&nbt).map_err(SchemError::Io)?;
    encoder.finish().map_err(SchemError::Io)
}

// --- Public API -------------------------------------------------------------

/// Serialize to uncompressed Sponge v2 NBT (root compound named `"Schematic"`).
pub fn to_nbt_v2(model: &VoxelModel, opts: &SchematicOptions) -> Result<Vec<u8>, SchemError> {
    fastnbt::to_bytes_with_opts(
        &build_v2(model, opts),
        SerOpts::new().root_name(V2_ROOT_NAME),
    )
    .map_err(SchemError::Nbt)
}

/// Serialize to uncompressed Sponge v3 NBT (unnamed root holding a `"Schematic"`
/// child, blocks under `Blocks { Palette, Data }`).
pub fn to_nbt_v3(model: &VoxelModel, opts: &SchematicOptions) -> Result<Vec<u8>, SchemError> {
    fastnbt::to_bytes(&build_v3(model, opts)).map_err(SchemError::Nbt)
}

/// Serialize to a gzip-compressed Sponge v2 `.schem` (`//schem load`).
pub fn to_schem_v2(model: &VoxelModel, opts: &SchematicOptions) -> Result<Vec<u8>, SchemError> {
    gzip(to_nbt_v2(model, opts)?)
}

/// Serialize to a gzip-compressed Sponge v3 `.schem`.
pub fn to_schem_v3(model: &VoxelModel, opts: &SchematicOptions) -> Result<Vec<u8>, SchemError> {
    gzip(to_nbt_v3(model, opts)?)
}

/// Serialize to a gzip-compressed `.schem` of the requested version.
pub fn to_schem(
    version: SchematicVersion,
    model: &VoxelModel,
    opts: &SchematicOptions,
) -> Result<Vec<u8>, SchemError> {
    match version {
        SchematicVersion::V2 => to_schem_v2(model, opts),
        SchematicVersion::V3 => to_schem_v3(model, opts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DATA_VERSION: i32 = 3700;
    const AIR_KEY: &str = "minecraft:air";
    const OAK: &str = "minecraft:oak_log[axis=y]";

    fn oak_column() -> VoxelModel {
        let mut m = VoxelModel::new(1, 2, 1).unwrap();
        m.set(0, 0, 0, OAK);
        m.set(0, 1, 0, OAK);
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

    fn decode_block_data(data: &ByteArray) -> Vec<u32> {
        let raw: Vec<u8> = data.iter().map(|&b| b as u8).collect();
        varint::read_all_unsigned(&raw).unwrap()
    }

    // --- v2 ---------------------------------------------------------------

    #[test]
    fn v2_nbt_round_trips() {
        let nbt = to_nbt_v2(&oak_column(), &opts()).unwrap();
        let parsed: SchematicV2 = fastnbt::from_bytes(&nbt).unwrap();

        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.data_version, TEST_DATA_VERSION);
        assert_eq!((parsed.width, parsed.height, parsed.length), (1, 2, 1));
        assert_eq!(parsed.palette_max, 2);
        assert_eq!(parsed.palette.get(AIR_KEY), Some(&0));
        assert_eq!(parsed.palette.get(OAK), Some(&1));
        assert!(parsed.offset.is_none());
        assert!(parsed.metadata.is_none());
        assert_eq!(decode_block_data(&parsed.block_data), vec![1, 1]);
    }

    #[test]
    fn v2_root_compound_is_named_schematic() {
        let nbt = to_nbt_v2(&oak_column(), &opts()).unwrap();
        assert_eq!(nbt[0], 0x0a, "TAG_Compound");
        assert_eq!(&nbt[1..3], &[0x00, 0x09], "root name length 9");
        assert_eq!(&nbt[3..12], b"Schematic", "root name");
    }

    #[test]
    fn v2_schem_is_gzipped_nbt() {
        let model = oak_column();
        let o = opts();
        let nbt = to_nbt_v2(&model, &o).unwrap();
        let schem = to_schem_v2(&model, &o).unwrap();
        assert_eq!(&schem[0..2], &[0x1f, 0x8b], "gzip magic");
        assert_eq!(gunzip(&schem), nbt);
    }

    #[test]
    fn v2_offset_and_metadata_are_emitted_when_set() {
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

    /// Pins the exact uncompressed bytes against a committed fixture (verified by
    /// hand against the Sponge v2 spec). Catches any future drift in field
    /// order, types, or palette ordering.
    #[test]
    fn v2_matches_golden_nbt() {
        let nbt = to_nbt_v2(&oak_column(), &opts()).unwrap();
        let golden = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/golden/oak_column_v2.nbt"
        ));
        assert_eq!(
            nbt.as_slice(),
            &golden[..],
            "uncompressed v2 NBT drifted from the committed golden fixture"
        );
    }

    // --- v3 ---------------------------------------------------------------

    #[test]
    fn v3_nbt_round_trips() {
        let nbt = to_nbt_v3(&oak_column(), &opts()).unwrap();
        let parsed: SchematicV3File = fastnbt::from_bytes(&nbt).unwrap();
        let s = parsed.schematic;

        assert_eq!(s.version, 3);
        assert_eq!(s.data_version, TEST_DATA_VERSION);
        assert_eq!((s.width, s.height, s.length), (1, 2, 1));
        assert_eq!(s.blocks.palette.get(AIR_KEY), Some(&0));
        assert_eq!(s.blocks.palette.get(OAK), Some(&1));
        assert_eq!(decode_block_data(&s.blocks.data), vec![1, 1]);
    }

    #[test]
    fn v3_root_is_unnamed_with_schematic_child() {
        let nbt = to_nbt_v3(&oak_column(), &opts()).unwrap();
        // Unnamed root compound.
        assert_eq!(nbt[0], 0x0a, "root TAG_Compound");
        assert_eq!(&nbt[1..3], &[0x00, 0x00], "root name length 0 (unnamed)");
        // First child: a compound named "Schematic".
        assert_eq!(nbt[3], 0x0a, "child TAG_Compound");
        assert_eq!(&nbt[4..6], &[0x00, 0x09], "child name length 9");
        assert_eq!(&nbt[6..15], b"Schematic", "child name");
    }

    #[test]
    fn v3_schem_is_gzipped_nbt() {
        let model = oak_column();
        let o = opts();
        let nbt = to_nbt_v3(&model, &o).unwrap();
        let schem = to_schem_v3(&model, &o).unwrap();
        assert_eq!(&schem[0..2], &[0x1f, 0x8b], "gzip magic");
        assert_eq!(gunzip(&schem), nbt);
    }

    #[test]
    fn dispatcher_matches_version_specific() {
        let m = oak_column();
        let o = opts();
        assert_eq!(
            to_schem(SchematicVersion::V2, &m, &o).unwrap(),
            to_schem_v2(&m, &o).unwrap()
        );
        assert_eq!(
            to_schem(SchematicVersion::V3, &m, &o).unwrap(),
            to_schem_v3(&m, &o).unwrap()
        );
    }

    /// Pins the exact uncompressed v3 bytes against a committed, hand-verified
    /// fixture.
    #[test]
    fn v3_matches_golden_nbt() {
        let nbt = to_nbt_v3(&oak_column(), &opts()).unwrap();
        let golden = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/golden/oak_column_v3.nbt"
        ));
        assert_eq!(
            nbt.as_slice(),
            &golden[..],
            "uncompressed v3 NBT drifted from the committed golden fixture"
        );
    }

    // --- spec conformance -------------------------------------------------
    // These parse the output as a *generic* NBT tree (not our typed structs),
    // then assert the invariants the Sponge spec documents. This catches
    // wrong field names/types/structure independently of the serializer.

    use fastnbt::Value;
    use std::collections::HashMap;

    fn as_compound(v: &Value) -> &HashMap<String, Value> {
        match v {
            Value::Compound(m) => m,
            other => panic!("expected Compound, got {other:?}"),
        }
    }

    fn require<'a>(m: &'a HashMap<String, Value>, key: &str) -> &'a Value {
        m.get(key)
            .unwrap_or_else(|| panic!("missing required field {key:?}"))
    }

    fn as_int(v: &Value) -> i32 {
        match v {
            Value::Int(i) => *i,
            other => panic!("expected Int, got {other:?}"),
        }
    }

    fn as_short(v: &Value) -> i16 {
        match v {
            Value::Short(s) => *s,
            other => panic!("expected Short, got {other:?}"),
        }
    }

    fn as_byte_array(v: &Value) -> &ByteArray {
        match v {
            Value::ByteArray(b) => b,
            other => panic!("expected ByteArray, got {other:?}"),
        }
    }

    /// Assert the spec's "block container" invariants for a Palette + block-data
    /// pair (v2's root fields, or v3's `Blocks` compound).
    fn assert_block_container(palette: &Value, data: &Value, w: i16, h: i16, l: i16) {
        // Palette: each value is an Int, and the indices are contiguous 0..N.
        let palette = as_compound(palette);
        let mut indices: Vec<i32> = palette.values().map(as_int).collect();
        indices.sort_unstable();
        let n = indices.len() as i32;
        assert_eq!(
            indices,
            (0..n).collect::<Vec<_>>(),
            "palette indices must be contiguous 0..N"
        );

        // Block data: a varint byte array; one entry per cell; every index valid.
        let raw: Vec<u8> = as_byte_array(data).iter().map(|&b| b as u8).collect();
        let decoded = varint::read_all_unsigned(&raw).expect("block data must be valid varints");
        let volume = w as u16 as usize * h as u16 as usize * l as u16 as usize;
        assert_eq!(
            decoded.len(),
            volume,
            "block count must equal Width*Height*Length"
        );
        assert!(
            decoded.iter().all(|&i| (i as i32) < n),
            "every block index must be < palette size"
        );
    }

    #[test]
    fn v2_conforms_to_spec() {
        let nbt = to_nbt_v2(&oak_column(), &opts()).unwrap();
        let root = fastnbt::from_bytes::<Value>(&nbt).unwrap();
        let m = as_compound(&root);

        assert_eq!(as_int(require(m, "Version")), 2);
        as_int(require(m, "DataVersion")); // present and an Int
        let (w, h, l) = (
            as_short(require(m, "Width")),
            as_short(require(m, "Height")),
            as_short(require(m, "Length")),
        );
        assert_eq!(
            as_int(require(m, "PaletteMax")),
            as_compound(require(m, "Palette")).len() as i32,
            "PaletteMax must equal the palette entry count"
        );
        assert_block_container(require(m, "Palette"), require(m, "BlockData"), w, h, l);
        assert!(matches!(require(m, "BlockEntities"), Value::List(_)));
    }

    #[test]
    fn v3_conforms_to_spec() {
        let nbt = to_nbt_v3(&oak_column(), &opts()).unwrap();
        let root = fastnbt::from_bytes::<Value>(&nbt).unwrap();
        let schematic = as_compound(require(as_compound(&root), "Schematic"));

        assert_eq!(as_int(require(schematic, "Version")), 3);
        as_int(require(schematic, "DataVersion"));
        let (w, h, l) = (
            as_short(require(schematic, "Width")),
            as_short(require(schematic, "Height")),
            as_short(require(schematic, "Length")),
        );
        assert!(
            !schematic.contains_key("PaletteMax"),
            "v3 must not carry the v2-only PaletteMax field"
        );

        let blocks = as_compound(require(schematic, "Blocks"));
        assert_block_container(require(blocks, "Palette"), require(blocks, "Data"), w, h, l);
        assert!(matches!(require(blocks, "BlockEntities"), Value::List(_)));
    }
}
