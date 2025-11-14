//! Archetype identification helpers.
//!
//! Archetypes group entities that share an identical set of component
//! types. We compute a stable 64-bit identifier by hashing the sorted
//! component IDs. This allows cheap equality checks and convenient use
//! as keys in hash maps.

use crate::ecs::ComponentId;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub type ArchetypeId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchetypeLayout {
    id: ArchetypeId,
    components: Box<[ComponentId]>,
}

impl ArchetypeLayout {
    pub fn new(mut components: Vec<ComponentId>) -> Self {
        components.sort_unstable();
        components.dedup();
        let id = hash_components(&components);
        Self {
            id,
            components: components.into_boxed_slice(),
        }
    }

    #[inline]
    pub fn id(&self) -> ArchetypeId {
        self.id
    }

    #[inline]
    pub fn components(&self) -> &[ComponentId] {
        &self.components
    }

    #[inline]
    pub fn contains(&self, id: ComponentId) -> bool {
        self.components.binary_search(&id).is_ok()
    }
}

fn hash_components(components: &[ComponentId]) -> ArchetypeId {
    let mut hasher = DefaultHasher::new();
    components.iter().for_each(|c| c.hash(&mut hasher));
    hasher.finish()
}
