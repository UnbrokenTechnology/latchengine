//! Spatial hash grid accelerator for position-based queries.
//!
//! This accelerator uses a uniform grid to efficiently find entities
//! within a given radius of a point.

use super::{QueryAccelerator, QueryResult};
use crate::ecs::{ComponentId, Entity, World};
use std::collections::HashMap;

/// Configuration for spatial hash grid.
#[derive(Debug, Clone)]
pub struct SpatialHashConfig {
    /// Size of each grid cell (in game units).
    pub cell_size: i32,
    /// Component ID to index (must be a position-like component with x, y fields).
    pub component_id: ComponentId,
}

impl SpatialHashConfig {
    /// Create a new spatial hash config.
    pub fn new(component_id: ComponentId, cell_size: i32) -> Self {
        Self {
            cell_size,
            component_id,
        }
    }
}

/// Grid cell coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CellCoord {
    x: i32,
    y: i32,
}

impl CellCoord {
    fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Get the 8 surrounding cells (and self, so 9 total).
    fn neighbors(&self) -> [CellCoord; 9] {
        [
            CellCoord::new(self.x - 1, self.y - 1),
            CellCoord::new(self.x, self.y - 1),
            CellCoord::new(self.x + 1, self.y - 1),
            CellCoord::new(self.x - 1, self.y),
            CellCoord::new(self.x, self.y),
            CellCoord::new(self.x + 1, self.y),
            CellCoord::new(self.x - 1, self.y + 1),
            CellCoord::new(self.x, self.y + 1),
            CellCoord::new(self.x + 1, self.y + 1),
        ]
    }
}

/// Entry in the spatial hash grid.
#[derive(Debug, Clone)]
struct GridEntry {
    entity: Entity,
    x: i32,
    y: i32,
}

/// Spatial hash grid accelerator.
pub struct SpatialHashGrid {
    config: SpatialHashConfig,
    /// Map from cell coordinates to entities in that cell.
    cells: HashMap<CellCoord, Vec<GridEntry>>,
}

impl SpatialHashGrid {
    /// Create a new spatial hash grid.
    pub fn new(config: SpatialHashConfig) -> Self {
        Self {
            config,
            cells: HashMap::new(),
        }
    }

    /// Query for entities within a radius of a point.
    pub fn query_radius(&self, x: i32, y: i32, radius: i32) -> QueryResult {
        let mut result = QueryResult::new();
        let radius_squared = (radius as i64) * (radius as i64);

        // Determine which cells to check
        let cell = self.pos_to_cell(x, y);
        let cells_to_check = cell.neighbors();

        for cell_coord in &cells_to_check {
            if let Some(entries) = self.cells.get(cell_coord) {
                for entry in entries {
                    let dx = (entry.x - x) as i64;
                    let dy = (entry.y - y) as i64;
                    let dist_squared = dx * dx + dy * dy;

                    if dist_squared <= radius_squared {
                        result.push(entry.entity);
                    }
                }
            }
        }

        result
    }

    /// Convert a position to a cell coordinate.
    fn pos_to_cell(&self, x: i32, y: i32) -> CellCoord {
        CellCoord::new(
            x.div_euclid(self.config.cell_size),
            y.div_euclid(self.config.cell_size),
        )
    }

    /// Insert an entity at a position.
    fn insert(&mut self, entity: Entity, x: i32, y: i32) {
        let cell = self.pos_to_cell(x, y);
        self.cells.entry(cell).or_default().push(GridEntry {
            entity,
            x,
            y,
        });
    }

    /// Clear all entries.
    fn clear(&mut self) {
        self.cells.clear();
    }
}

impl QueryAccelerator for SpatialHashGrid {
    fn rebuild(&mut self, world: &World) {
        self.clear();

        // Find all archetypes with the position component
        let archetypes = world.archetypes_with(self.config.component_id);

        for &arch_id in archetypes {
            if let Some(storage) = world.storage(arch_id) {
                // Get the position column
                let pos_col = match storage.column(self.config.component_id) {
                    Ok(col) => col,
                    Err(_) => continue,
                };

                // Iterate through all pages
                for page_idx in 0..pos_col.page_count() {
                    let range = pos_col.page_range(page_idx);
                    if range.is_empty() {
                        continue;
                    }

                    let start = range.start;
                    let end = range.end;

                    // Get entity IDs
                    let entity_ids = match storage.entity_ids_slice(start..end) {
                        Ok(ids) => ids,
                        Err(_) => continue,
                    };

                    // Read position data as raw bytes
                    // We assume the first 8 bytes are x (i32) and y (i32)
                    let positions = match pos_col.slice_read(start..end) {
                        Ok(data) => data,
                        Err(_) => continue,
                    };

                    // Extract positions (assuming i32 x, i32 y layout)
                    let pos_stride = 8; // 2 × i32
                    for (i, &entity_id) in entity_ids.iter().enumerate() {
                        if i * pos_stride + 8 > positions.len() {
                            break;
                        }

                        let x = i32::from_ne_bytes([
                            positions[i * pos_stride],
                            positions[i * pos_stride + 1],
                            positions[i * pos_stride + 2],
                            positions[i * pos_stride + 3],
                        ]);

                        let y = i32::from_ne_bytes([
                            positions[i * pos_stride + 4],
                            positions[i * pos_stride + 5],
                            positions[i * pos_stride + 6],
                            positions[i * pos_stride + 7],
                        ]);

                        // Get generation for this entity
                        if let Ok(loc) = world.locate(Entity::new(entity_id, 0)) {
                            let entity = Entity::new(entity_id, loc.generation);
                            self.insert(entity, x, y);
                        }
                    }
                }
            }
        }
    }

    fn query(&self, params: &[u8]) -> QueryResult {
        // Params format: [x: i32, y: i32, radius: i32] = 12 bytes
        if params.len() < 12 {
            return QueryResult::new();
        }

        let x = i32::from_ne_bytes([params[0], params[1], params[2], params[3]]);
        let y = i32::from_ne_bytes([params[4], params[5], params[6], params[7]]);
        let radius = i32::from_ne_bytes([params[8], params[9], params[10], params[11]]);

        self.query_radius(x, y, radius)
    }

    fn name(&self) -> &str {
        "SpatialHashGrid"
    }
}

/// Helper to create query parameters for spatial hash queries.
pub fn query_params_radius(x: i32, y: i32, radius: i32) -> [u8; 12] {
    let mut params = [0u8; 12];
    params[0..4].copy_from_slice(&x.to_ne_bytes());
    params[4..8].copy_from_slice(&y.to_ne_bytes());
    params[8..12].copy_from_slice(&radius.to_ne_bytes());
    params
}
