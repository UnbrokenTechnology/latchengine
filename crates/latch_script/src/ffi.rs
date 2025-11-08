//! FFI layer between Rust and scripts
//!
//! Handle-based access to engine systems

use latch_core::ecs::Entity;

/// Opaque handle for script access
#[derive(Debug, Copy, Clone)]
pub struct ScriptHandle(pub u64);

impl From<Entity> for ScriptHandle {
    fn from(entity: Entity) -> Self {
        ScriptHandle(entity.to_bits())
    }
}

impl From<ScriptHandle> for Entity {
    fn from(handle: ScriptHandle) -> Self {
        Entity::from_bits(handle.0)
    }
}
