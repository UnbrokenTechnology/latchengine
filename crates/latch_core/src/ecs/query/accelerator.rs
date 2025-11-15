//! Core trait and types for query accelerators.

use crate::ecs::{Entity, World};

/// Result of a query operation.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Entities matching the query.
    pub entities: Vec<Entity>,
}

impl QueryResult {
    /// Create a new empty query result.
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
        }
    }

    /// Create a query result with a pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entities: Vec::with_capacity(capacity),
        }
    }

    /// Add an entity to the result.
    pub fn push(&mut self, entity: Entity) {
        self.entities.push(entity);
    }

    /// Get the number of entities in the result.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Check if the result is empty.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Iterate over entities in the result.
    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.entities.iter()
    }
}

impl Default for QueryResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for query accelerators.
///
/// Query accelerators are index-like structures that enable efficient
/// queries over entity data without iterating through all entities.
///
/// Accelerators must be rebuilt or updated when component data changes.
pub trait QueryAccelerator: Send + Sync {
    /// Rebuild the accelerator from the current world state.
    ///
    /// This is called after component mutations to update the index.
    /// Takes immutable access to the world since accelerators only read state.
    fn rebuild(&mut self, world: &World);

    /// Query for entities matching specific criteria.
    ///
    /// The query parameters are accelerator-specific and passed as bytes.
    fn query(&self, params: &[u8]) -> QueryResult;

    /// Name of this accelerator type (for debugging).
    fn name(&self) -> &str;
}
