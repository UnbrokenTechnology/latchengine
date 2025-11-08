// builder.rs - EntityBuilder for constructing entities
//
// Collects components before spawning to calculate archetype once.

use crate::ecs::{Archetype, Component, ComponentId};
use std::collections::HashMap;

/// Builder for constructing entities with components.
/// 
/// Components are collected first, then the archetype is calculated,
/// and finally the entity is spawned. This ensures entities are placed
/// in the correct archetype from the start (no migration needed).
pub struct EntityBuilder {
    components: HashMap<ComponentId, Vec<u8>>,
    order: Vec<ComponentId>, // Maintain insertion order for determinism
}

impl EntityBuilder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// Add a Rust-typed component.
    /// 
    /// The component is copied to raw bytes and the original is forgotten
    /// to avoid double-drop.
    pub fn with<T: Component>(mut self, value: T) -> Self {
        T::ensure_registered();
        let id = T::ID;
        
        // Convert component to bytes
        let bytes = unsafe {
            let ptr = &value as *const T as *const u8;
            std::slice::from_raw_parts(ptr, std::mem::size_of::<T>()).to_vec()
        };
        
        // Track insertion order
        if !self.components.contains_key(&id) {
            self.order.push(id);
        }
        
        self.components.insert(id, bytes);
        std::mem::forget(value); // Prevent double-drop
        self
    }

    /// Add a component by raw bytes (for TypeScript/JSON components).
    /// 
    /// # Panics
    /// Panics if bytes.len() != expected_size.
    pub fn with_raw(mut self, id: ComponentId, bytes: Vec<u8>, expected_size: usize) -> Self {
        assert_eq!(
            bytes.len(),
            expected_size,
            "Raw component size mismatch for id={}: expected {}, got {}",
            id,
            expected_size,
            bytes.len()
        );
        
        if !self.components.contains_key(&id) {
            self.order.push(id);
        }
        
        self.components.insert(id, bytes);
        self
    }

    /// Calculate the archetype for this entity.
    pub fn archetype(&self) -> Archetype {
        let mut ids: Vec<ComponentId> = self.components.keys().copied().collect();
        ids.sort_unstable();
        Archetype::from_components(ids)
    }

    /// Consume the builder and return components as a sorted list.
    pub fn into_components(self) -> Vec<(ComponentId, Vec<u8>)> {
        let mut v: Vec<_> = self.components.into_iter().collect();
        v.sort_by_key(|(id, _)| *id);
        v
    }
}

impl Default for EntityBuilder {
    fn default() -> Self {
        Self::new()
    }
}
