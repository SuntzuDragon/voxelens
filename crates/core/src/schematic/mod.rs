//! The voxel data model and Sponge `.schem` (v2) serialization.
//!
//! [`VoxelModel`] is the central 3D grid type produced by reconstruction and
//! consumed by the schematic writer. [`to_schem_v2`] serializes it to a
//! WorldEdit-loadable `.schem` file.

mod schem;
mod varint;
mod voxel_model;

pub use schem::{
    to_nbt_v2, to_nbt_v3, to_schem, to_schem_v2, to_schem_v3, SchemError, SchematicOptions,
    SchematicVersion,
};
pub use varint::{read_all_unsigned, read_unsigned, write_unsigned, VarintError};
pub use voxel_model::{VoxelError, VoxelModel, AIR};
