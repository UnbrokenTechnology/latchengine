//! Archetype storage for entities with identical component sets

use std::any::TypeId;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::entity::Entity;
use super::component::{Component, ComponentStorage, ComponentVec};

/// Unique identifier for an archetype
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ArchetypeId(pub u64);

impl ArchetypeId {
    pub fn from_types(component_types: &[TypeId]) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for &type_id in component_types {
            type_id.hash(&mut hasher);
        }
        Self(hasher.finish())
    }
}

/// Archetype: Collection of entities with identical component signatures
///
/// All entities in an archetype have the exact same set of components.
/// Components are stored in separate contiguous arrays (SoA layout) for cache efficiency.
pub(crate) struct Archetype {
    #[allow(dead_code)] // Used for debugging and archetype queries
    pub id: ArchetypeId,
    #[allow(dead_code)] // Used for archetype migrations (though we don't support them at runtime)
    pub component_types: Vec<TypeId>,
    pub entities: Vec<Entity>,
    pub components: HashMap<TypeId, Box<dyn ComponentStorage>>,
}

impl Archetype {
    pub fn new(id: ArchetypeId, component_types: Vec<TypeId>) -> Self {
        Self {
            id,
            component_types,
            entities: Vec::new(),
            components: HashMap::new(),
        }
    }

    pub fn add_storage<T: Component + Clone>(&mut self) {
        self.components
            .insert(TypeId::of::<T>(), Box::new(ComponentVec::<T>::new()));
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn has_component(&self, type_id: TypeId) -> bool {
        self.components.contains_key(&type_id)
    }

    pub fn get_storage<T: Component>(&self) -> Option<&ComponentVec<T>> {
        self.components
            .get(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any().downcast_ref::<ComponentVec<T>>())
    }

    pub fn get_storage_mut<T: Component>(&mut self) -> Option<&mut ComponentVec<T>> {
        self.components
            .get_mut(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any_mut().downcast_mut::<ComponentVec<T>>())
    }

    #[allow(dead_code)]
    pub fn add_entity<T: Component + Clone>(&mut self, entity: Entity, component: T) {
        if !self.has_component(TypeId::of::<T>()) {
            self.add_storage::<T>();
        }

        let storage = self.get_storage_mut::<T>().unwrap();
        storage.data.push(component);
        self.entities.push(entity);
    }

    pub fn remove_entity(&mut self, index: usize) -> Entity {
        let entity = self.entities.swap_remove(index);
        for storage in self.components.values_mut() {
            storage.swap_remove(index);
        }
        entity
    }

    /// Swap-remove entity and return it (alias for remove_entity)
    pub fn swap_remove(&mut self, index: usize) -> Entity {
        self.remove_entity(index)
    }

    /// Add entity without component (for internal use)
    pub fn push_entity(&mut self, entity: Entity) -> usize {
        let index = self.entities.len();
        self.entities.push(entity);
        index
    }
}
