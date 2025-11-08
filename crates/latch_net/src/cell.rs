//! Cell-based world partitioning

use crate::CellId;
use latch_core::math::Vec3;

/// Cell configuration
pub struct CellConfig {
    pub cell_size: f32, // Size in world units (e.g., 192.0 meters)
}

impl Default for CellConfig {
    fn default() -> Self {
        Self { cell_size: 192.0 }
    }
}

/// Convert world position to cell ID
pub fn world_pos_to_cell(pos: Vec3, config: &CellConfig) -> CellId {
    let x = (pos.x / config.cell_size).floor() as i32;
    let z = (pos.z / config.cell_size).floor() as i32;

    // Simple 2D grid encoding (z * large_prime + x)
    let id = ((z as i64) << 32) | (x as i64 & 0xFFFFFFFF);
    CellId(id as u64)
}
