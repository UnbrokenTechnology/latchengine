// entity.rs - Entity handles with generational indices
//
// Generational indices prevent stale entity handles from accessing wrong entities
// after despawn+respawn cycles reuse the same slot.

use crate::ecs::ArchetypeId;

/// Entity handle with generational index for safety.
/// 
/// The generation is incremented each time an entity slot is reused,
/// preventing stale handles from accessing the wrong entity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Entity {
    /// Globally unique entity ID (never reused).
    pub id: u64,
    
    /// Generation counter for this slot (increments on despawn).
    pub generation: u32,
    
    /// Archetype this entity belongs to.
    pub archetype: ArchetypeId,
    
    /// Row index within the archetype's storage.
    pub index: usize,
}

/// A dense array of used entities. When an entity is freed, we swap it with the last one and pop.
/// This allows O(1) allocation and deallocation.
static mut ENTITY_POOL: Vec<Entity> = Vec::new();
static mut NEXT_ENTITY: usize = 0;

impl Entity {

    const INITIAL_POOL_SIZE: usize = 4096;

    /// Create a new entity handle.
    pub(crate) fn new(id: u64, archetype: ArchetypeId, index: usize) -> Self {
        if ENTITY_POOL.capacity() == 0 {
            ENTITY_POOL.reserve(Self::INITIAL_POOL_SIZE);
        }
        Self {
            id,
            generation,
            archetype,
            index,
        }
    }

    /// Pack the entity into a single u64 for FFI/scripting.
    /// 
    /// This is a lossy conversion - only the entity ID is preserved.
    /// Use only for opaque handles in scripting contexts.
    pub fn to_bits(self) -> u64 {
        self.id
    }

    /// Unpack an entity from a u64.
    /// 
    /// This creates a partially invalid entity (generation, archetype, and index are zeroed).
    /// Only the ID is restored. Use only for opaque handles in scripting contexts
    /// where the World will validate and fill in the missing fields.
    pub fn from_bits(bits: u64) -> Self {
        Self {
            id: bits,
            generation: 0,
            archetype: 0,
            index: 0,
        }
    }
}
