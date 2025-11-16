//! Core trait definition for relation accelerators.

use super::relation::{RelationBuffer, RelationType};
use crate::ecs::{Entity, World};

/// Trait representing a structure capable of emitting relations for a specific
/// semantic category (collisions, triggers, visibility, etc.).
pub trait RelationAccelerator: Send + Sync {
    /// Type discriminator for the relations produced by this accelerator.
    fn relation_type(&self) -> RelationType;

    /// Notify the accelerator that an entity that participates in this relation
    /// has entered the world.
    fn register(&mut self, _entity: Entity) {}

    /// Notify the accelerator that a tracked entity changed relevant data.
    fn update(&mut self, _entity: Entity) {}

    /// Notify the accelerator that an entity should be removed from tracking.
    fn unregister(&mut self, _entity: Entity) {}

    /// Rebuild internal structures from the current world snapshot and emit relations
    /// directly into the provided buffer. Implementations should prefer streaming
    /// passes that avoid unnecessary allocations.
    fn rebuild(&mut self, world: &World, output: &mut RelationBuffer);
}
