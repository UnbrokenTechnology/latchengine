// entity.rs
use crate::archetype::{Archetype, ArchetypeId};
use crate::component::{Component, ComponentId, ComponentMeta};
use std::collections::HashMap;

/// Compact entity handle: points into an archetype storage row.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Entity {
    pub id: u64,
    pub archetype: ArchetypeId,
    pub index: usize, // row index inside that archetype's SoA
}

/// Builder collects components (typed or raw) before spawning.
/// The entity's archetype will never change afterwards.
pub struct EntityBuilder {
    comps: HashMap<ComponentId, Vec<u8>>, // raw bytes per component id
    order: Vec<ComponentId>,              // keep deterministic order
}

impl EntityBuilder {
    pub fn new() -> Self {
        Self { comps: HashMap::new(), order: Vec::new() }
    }

    /// Add a Rust-defined component value (POD).
    pub fn with<T: Component>(mut self, value: T) -> Self {
        T::ensure_registered();
        let id = T::ID;
        let bytes = unsafe {
            let ptr = &value as *const T as *const u8;
            std::slice::from_raw_parts(ptr, std::mem::size_of::<T>()).to_vec()
        };
        if !self.comps.contains_key(&id) {
            self.order.push(id);
        }
        self.comps.insert(id, bytes);
        std::mem::forget(value); // value copied by bytes; avoid drop on moved POD
        self
    }

    /// Add a foreign/TS-defined component by raw bytes and expected size.
    pub fn with_raw(mut self, id: ComponentId, bytes: Vec<u8>, expected_size: usize) -> Self {
        assert_eq!(bytes.len(), expected_size, "raw component size mismatch for id={id}");
        if !self.comps.contains_key(&id) {
            self.order.push(id);
        }
        self.comps.insert(id, bytes);
        self
    }

    pub fn archetype(&self) -> Archetype {
        let mut ids = self.order.clone();
        ids.sort_unstable();
        Archetype::from_components(ids)
    }

    /// Consume into a stable list of (id, bytes).
    pub fn into_components(self) -> Vec<(ComponentId, Vec<u8>)> {
        let mut v: Vec<_> = self.comps.into_iter().collect();
        v.sort_by_key(|(id, _)| *id);
        v
    }
}