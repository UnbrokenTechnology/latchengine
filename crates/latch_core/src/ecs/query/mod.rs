//! Relation accelerators build per-tick indices so systems can consume
//! collision/visibility/trigger data without performing their own scans.

mod accelerator;
mod relation;
mod spatial_hash;

pub use accelerator::RelationAccelerator;
pub use relation::{
    EntityRelationEntry, RelationBuffer, RelationDelta, RelationIter, RelationPayloadRange,
    RelationRecord, RelationType,
};
pub use spatial_hash::{
    reset_spatial_hash_metrics, spatial_hash_metrics_snapshot, SpatialHashConfig, SpatialHashGrid,
    SpatialHashMetricsSnapshot,
};

use crate::ecs::World;
use std::collections::HashMap;

/// Owns all registered relation accelerators and coordinates rebuild/emit passes.
pub struct QueryRegistry {
    accelerators: Vec<Box<dyn RelationAccelerator + Send + Sync>>,
    by_type: HashMap<u16, usize>,
}

impl QueryRegistry {
    pub fn new() -> Self {
        Self {
            accelerators: Vec::new(),
            by_type: HashMap::new(),
        }
    }

    pub fn register(&mut self, accelerator: Box<dyn RelationAccelerator + Send + Sync>) {
        let ty = accelerator.relation_type().raw();
        if self.by_type.contains_key(&ty) {
            panic!("relation accelerator for type {} already registered", ty);
        }
        self.accelerators.push(accelerator);
        self.by_type.insert(ty, self.accelerators.len() - 1);
    }

    pub fn rebuild_all(&mut self, world: &World, buffer: &mut RelationBuffer) {
        for accelerator in &mut self.accelerators {
            accelerator.rebuild(world, buffer);
        }
    }

    pub fn get(&self, relation: RelationType) -> Option<&dyn RelationAccelerator> {
        self.by_type
            .get(&relation.raw())
            .and_then(|&idx| self.accelerators.get(idx))
            .map(|boxed| &**boxed as &dyn RelationAccelerator)
    }
}

impl Default for QueryRegistry {
    fn default() -> Self {
        Self::new()
    }
}
