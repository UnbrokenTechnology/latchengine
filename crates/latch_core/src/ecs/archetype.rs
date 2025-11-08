// archetype.rs - Archetype identification and management
//
// An archetype is a unique set of component types.
// Entities with the same component types share the same archetype.

use crate::ecs::ComponentId;
use std::hash::{Hash, Hasher};

pub type ArchetypeId = u64;

/// An archetype represents a unique combination of component types.
/// 
/// The component IDs are always stored in sorted order to ensure
/// deterministic archetype ID calculation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Archetype {
    /// Hash-based unique identifier for this archetype.
    pub id: ArchetypeId,
    
    /// Sorted list of component IDs in this archetype.
    pub components: Vec<ComponentId>,
}

impl Archetype {
    /// Create an archetype from a list of component IDs.
    /// 
    /// The input will be sorted and deduplicated.
    pub fn from_components(mut comps: Vec<ComponentId>) -> Self {
        comps.sort_unstable();
        comps.dedup();
        let id = hash_components(&comps);
        Self {
            id,
            components: comps,
        }
    }

    /// Check if this archetype contains a specific component.
    pub fn contains(&self, id: ComponentId) -> bool {
        self.components.binary_search(&id).is_ok()
    }
}

/// Compute a stable hash for a sorted list of component IDs.
fn hash_components(comps: &[ComponentId]) -> ArchetypeId {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    for &c in comps {
        c.hash(&mut hasher);
    }
    hasher.finish()
}
