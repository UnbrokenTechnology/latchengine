// archetype.rs
use crate::component::ComponentId;
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

pub type ArchetypeId = u64;

/// An archetype is a *set* of component types.
#[derive(Clone, Debug, Eq)]
pub struct Archetype {
    pub id: ArchetypeId,
    pub components: Vec<ComponentId>, // always sorted ascending
}

impl PartialEq for Archetype {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.components == other.components
    }
}

impl Archetype {
    pub fn from_components(mut comps: Vec<ComponentId>) -> Self {
        comps.sort_unstable();
        comps.dedup();
        let id = hash_components(&comps);
        Self { id, components: comps }
    }

    pub fn contains(&self, id: ComponentId) -> bool {
        self.components.binary_search(&id).is_ok()
    }
}

fn hash_components(comps: &[ComponentId]) -> ArchetypeId {
    // Simple stable hash for a sorted list of IDs
    use std::collections::hash_map::DefaultHasher;
    let mut h = DefaultHasher::new();
    for &c in comps {
        c.hash(&mut h);
    }
    h.finish()
}