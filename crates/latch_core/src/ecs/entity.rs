//! Entity handles with generational indexing

use super::archetype::ArchetypeId;

/// Entity handle (generation-indexed for safety)
///
/// Format: [32-bit index | 32-bit generation]
/// - Index: Position in entity metadata array
/// - Generation: Incremented on entity destruction (prevents use-after-free)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Entity {
    pub(crate) index: u32,
    pub(crate) generation: u32,
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

/// Entity metadata tracked by the World
#[derive(Clone)]
pub(crate) struct EntityMetadata {
    pub generation: u32,
    pub alive: bool,
    pub archetype_id: ArchetypeId,
    pub archetype_index: usize,
}
