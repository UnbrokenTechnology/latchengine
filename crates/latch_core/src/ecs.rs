//! Entity Component System
//!
//! Phase 0: Stub for initial prototyping
//! Phase 1: Full archetype-based ECS with authority awareness

/// Entity handle (opaque ID)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Entity(u64);

impl Entity {
    pub fn from_raw(id: u64) -> Self {
        Self(id)
    }

    pub fn id(&self) -> u64 {
        self.0
    }
}

/// Placeholder for World/ECS implementation
pub struct World {
    next_entity_id: u64,
}

impl World {
    pub fn new() -> Self {
        Self { next_entity_id: 1 }
    }

    pub fn spawn(&mut self) -> Entity {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        Entity(id)
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
