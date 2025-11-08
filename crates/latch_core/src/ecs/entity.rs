//! Entity handle with generational index
//!
//! Entities are lightweight handles (8 bytes) that reference data in the World.
//! The generation counter prevents use-after-free bugs.

/// Entity handle (generation-indexed for safety)
///
/// Format: [32-bit index | 32-bit generation]
/// - Index: Position in entity metadata array
/// - Generation: Incremented on entity destruction (prevents use-after-free)
///
/// Example:
/// ```ignore
/// let entity = world.spawn();
/// world.despawn(entity);
/// // entity handle is now invalid (generation mismatch)
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    pub(crate) const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Serialize to 64-bit integer (for networking/save files)
    pub fn to_bits(&self) -> u64 {
        ((self.generation as u64) << 32) | (self.index as u64)
    }

    /// Deserialize from 64-bit integer
    pub fn from_bits(bits: u64) -> Self {
        Self {
            index: bits as u32,
            generation: (bits >> 32) as u32,
        }
    }
}
