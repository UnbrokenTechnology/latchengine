//! Query accelerator system for efficient spatial and relational queries.
//!
//! This module provides an extensible framework for indexing entity data
//! to answer queries like "find all entities near position X" without
//! iterating through all entities.

mod accelerator;
mod spatial_hash;

pub use accelerator::{QueryAccelerator, QueryResult};
pub use spatial_hash::{query_params_radius, SpatialHashConfig, SpatialHashGrid};

use crate::ecs::ComponentId;
use std::collections::HashMap;

/// Registry of query accelerators per component.
pub struct QueryRegistry {
    /// Maps component IDs to their registered accelerators.
    accelerators: HashMap<ComponentId, Box<dyn QueryAccelerator + Send + Sync>>,
}

impl QueryRegistry {
    /// Create a new empty query registry.
    pub fn new() -> Self {
        Self {
            accelerators: HashMap::new(),
        }
    }

    /// Register a query accelerator for a component.
    pub fn register(
        &mut self,
        component_id: ComponentId,
        accelerator: Box<dyn QueryAccelerator + Send + Sync>,
    ) {
        self.accelerators.insert(component_id, accelerator);
    }

    /// Get a reference to a registered accelerator.
    pub fn get(&self, component_id: ComponentId) -> Option<&dyn QueryAccelerator> {
        self.accelerators.get(&component_id).map(|b| &**b as &dyn QueryAccelerator)
    }

    /// Get a mutable reference to a registered accelerator.
    pub fn get_mut(&mut self, component_id: ComponentId) -> Option<&mut dyn QueryAccelerator> {
        self.accelerators.get_mut(&component_id).map(|b| &mut **b as &mut dyn QueryAccelerator)
    }

    /// Check if an accelerator is registered for a component.
    pub fn has_accelerator(&self, component_id: ComponentId) -> bool {
        self.accelerators.contains_key(&component_id)
    }

    /// Get the list of component IDs that have accelerators.
    pub fn component_ids(&self) -> Vec<ComponentId> {
        self.accelerators.keys().copied().collect()
    }
}

impl Default for QueryRegistry {
    fn default() -> Self {
        Self::new()
    }
}
