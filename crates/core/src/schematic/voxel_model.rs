//! The central voxel data structure.

use std::collections::HashMap;

/// The block state filling empty space. Always palette index 0.
pub const AIR: &str = "minecraft:air";

/// Error constructing a [`VoxelModel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoxelError {
    /// A dimension was zero; schematics must be at least 1×1×1.
    ZeroDimension,
}

impl std::fmt::Display for VoxelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VoxelError::ZeroDimension => write!(f, "voxel model dimensions must each be >= 1"),
        }
    }
}

impl std::error::Error for VoxelError {}

/// A 3D grid of Minecraft block states.
///
/// Storage mirrors the Sponge schematic convention: a palette of distinct block
/// state strings plus a flat array of palette indices addressed by
/// `x + z * width + y * width * length`. Empty space is [`AIR`] at palette index
/// 0. Dimensions are `u16` to match the schematic `Short` fields.
#[derive(Debug, Clone)]
pub struct VoxelModel {
    width: u16,
    height: u16,
    length: u16,
    /// Palette index → block state string. Index 0 is always [`AIR`].
    palette: Vec<String>,
    /// Block state string → palette index, for O(1) interning.
    lookup: HashMap<String, u32>,
    /// Palette indices, length `width * height * length`, in [`Self::index`] order.
    data: Vec<u32>,
}

impl VoxelModel {
    /// Create an all-[`AIR`] model of the given dimensions.
    pub fn new(width: u16, height: u16, length: u16) -> Result<Self, VoxelError> {
        if width == 0 || height == 0 || length == 0 {
            return Err(VoxelError::ZeroDimension);
        }
        let volume = width as usize * height as usize * length as usize;
        let mut lookup = HashMap::new();
        lookup.insert(AIR.to_string(), 0);
        Ok(Self {
            width,
            height,
            length,
            palette: vec![AIR.to_string()],
            lookup,
            data: vec![0; volume],
        })
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn length(&self) -> u16 {
        self.length
    }

    /// Number of cells (`width * height * length`).
    pub fn volume(&self) -> usize {
        self.data.len()
    }

    /// The palette in index order; `palette()[i]` is the block state for index `i`.
    pub fn palette(&self) -> &[String] {
        &self.palette
    }

    /// The flat palette-index array, in `x + z*W + y*W*L` order.
    pub fn block_indices(&self) -> &[u32] {
        &self.data
    }

    /// Whether `(x, y, z)` lies within the grid.
    pub fn in_bounds(&self, x: u16, y: u16, z: u16) -> bool {
        x < self.width && y < self.height && z < self.length
    }

    /// Set the block at `(x, y, z)`.
    ///
    /// # Panics
    /// Panics if the coordinate is out of bounds (a programmer error, like
    /// indexing past the end of a slice).
    pub fn set(&mut self, x: u16, y: u16, z: u16, state: &str) {
        let idx = self.index(x, y, z);
        let pid = self.intern(state);
        self.data[idx] = pid;
    }

    /// Get the block state at `(x, y, z)`.
    ///
    /// # Panics
    /// Panics if the coordinate is out of bounds.
    pub fn get(&self, x: u16, y: u16, z: u16) -> &str {
        &self.palette[self.data[self.index(x, y, z)] as usize]
    }

    /// Linear index for a coordinate.
    ///
    /// # Panics
    /// Panics if the coordinate is out of bounds.
    #[inline]
    fn index(&self, x: u16, y: u16, z: u16) -> usize {
        assert!(
            self.in_bounds(x, y, z),
            "({x}, {y}, {z}) out of bounds for {}×{}×{}",
            self.width,
            self.height,
            self.length
        );
        x as usize
            + z as usize * self.width as usize
            + y as usize * self.width as usize * self.length as usize
    }

    /// Intern a block state, returning its palette index.
    fn intern(&mut self, state: &str) -> u32 {
        if let Some(&id) = self.lookup.get(state) {
            return id;
        }
        let id = self.palette.len() as u32;
        self.palette.push(state.to_string());
        self.lookup.insert(state.to_string(), id);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_dimensions() {
        assert_eq!(
            VoxelModel::new(0, 1, 1).unwrap_err(),
            VoxelError::ZeroDimension
        );
        assert_eq!(
            VoxelModel::new(1, 0, 1).unwrap_err(),
            VoxelError::ZeroDimension
        );
        assert_eq!(
            VoxelModel::new(1, 1, 0).unwrap_err(),
            VoxelError::ZeroDimension
        );
    }

    #[test]
    fn initializes_to_air() {
        let m = VoxelModel::new(2, 3, 4).unwrap();
        assert_eq!((m.width(), m.height(), m.length()), (2, 3, 4));
        assert_eq!(m.volume(), 24);
        assert_eq!(m.palette(), [AIR.to_string()]);
        assert!(m.block_indices().iter().all(|&i| i == 0));
        assert_eq!(m.get(1, 2, 3), AIR);
    }

    #[test]
    fn set_then_get() {
        let mut m = VoxelModel::new(1, 2, 1).unwrap();
        m.set(0, 0, 0, "minecraft:oak_log[axis=y]");
        m.set(0, 1, 0, "minecraft:oak_log[axis=y]");
        assert_eq!(m.get(0, 0, 0), "minecraft:oak_log[axis=y]");
        assert_eq!(m.get(0, 1, 0), "minecraft:oak_log[axis=y]");
    }

    #[test]
    fn index_formula_is_x_plus_zw_plus_ywl() {
        // 2×2×2: verify the three unit steps land at 1, W, and W*L.
        let mut m = VoxelModel::new(2, 2, 2).unwrap();
        m.set(1, 0, 0, "a"); // +x  -> index 1
        m.set(0, 0, 1, "b"); // +z  -> index width (2)
        m.set(0, 1, 0, "c"); // +y  -> index width*length (4)
        let d = m.block_indices();
        // palette order: air=0, a=1, b=2, c=3
        assert_eq!(d[1], 1, "+x step");
        assert_eq!(d[2], 2, "+z step");
        assert_eq!(d[4], 3, "+y step");
    }

    #[test]
    fn interns_palette_in_insertion_order() {
        let mut m = VoxelModel::new(3, 1, 1).unwrap();
        m.set(0, 0, 0, "x");
        m.set(1, 0, 0, "x"); // reuse
        m.set(2, 0, 0, "y");
        assert_eq!(
            m.palette(),
            [
                "minecraft:air".to_string(),
                "x".to_string(),
                "y".to_string()
            ]
        );
        assert_eq!(m.block_indices(), [1, 1, 2]);
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn set_out_of_bounds_panics() {
        let mut m = VoxelModel::new(1, 1, 1).unwrap();
        m.set(1, 0, 0, "x");
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn get_out_of_bounds_panics() {
        let m = VoxelModel::new(1, 1, 1).unwrap();
        let _ = m.get(0, 2, 0);
    }
}
