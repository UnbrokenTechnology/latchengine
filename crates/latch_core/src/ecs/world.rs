//! World: ECS container and entity management

use std::any::TypeId;
use std::collections::HashMap;

use super::archetype::{Archetype, ArchetypeId};
use super::component::Component;
use super::entity::{Entity, EntityMetadata};

/// The ECS World - owns all entities, components, and archetypes
pub struct World {
    /// Entity metadata (sparse array indexed by Entity.index)
    entities: Vec<EntityMetadata>,
    
    /// Free list for recycled entity indices
    free_entities: Vec<u32>,
    
    /// All archetypes (indexed by ArchetypeId)
    archetypes: HashMap<ArchetypeId, Archetype>,
    
    /// Next entity index to allocate
    next_entity_index: u32,
}

impl World {
    pub fn new() -> Self {
        Self::with_capacity(1024) // Preallocate 1024 entities
    }

    /// Create a world with preallocated entity capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let mut entities = Vec::with_capacity(capacity);
        let mut free_entities = Vec::with_capacity(capacity);
        
        // Preallocate entity metadata (all marked as dead initially)
        for i in 0..capacity {
            entities.push(EntityMetadata {
                generation: 0,
                archetype_id: ArchetypeId(0),
                archetype_index: 0,
                alive: false,
            });
            free_entities.push(i as u32);
        }
        
        // Reverse so we allocate in order (pop from end)
        free_entities.reverse();
        
        Self {
            entities,
            free_entities,
            archetypes: HashMap::new(),
            next_entity_index: capacity as u32,
        }
    }

    /// Create a new entity with no components
    pub fn spawn(&mut self) -> Entity {
        let index = if let Some(index) = self.free_entities.pop() {
            index
        } else {
            // Out of preallocated entities - grow by doubling
            let old_capacity = self.entities.len();
            let new_capacity = old_capacity * 2;
            
            self.entities.reserve(new_capacity - old_capacity);
            self.free_entities.reserve(new_capacity - old_capacity);
            
            for i in old_capacity..new_capacity {
                self.entities.push(EntityMetadata {
                    generation: 0,
                    archetype_id: ArchetypeId(0),
                    archetype_index: 0,
                    alive: false,
                });
                self.free_entities.push(i as u32);
            }
            
            self.free_entities.reverse(); // Maintain allocation order
            self.next_entity_index = new_capacity as u32;
            
            self.free_entities.pop().unwrap()
        };

        let metadata = &mut self.entities[index as usize];
        metadata.alive = true;
        
        Entity::new(index, metadata.generation)
    }

    /// Destroy an entity and all its components
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.is_valid(entity) {
            return false;
        }

        let metadata = &mut self.entities[entity.index as usize];
        let archetype_id = metadata.archetype_id;
        let archetype_index = metadata.archetype_index;
        
        metadata.alive = false;
        metadata.generation = metadata.generation.wrapping_add(1);

        // Remove from archetype
        if let Some(archetype) = self.archetypes.get_mut(&archetype_id) {
            let swapped_entity = archetype.swap_remove(archetype_index);
            
            // Update swapped entity's index (if not the last element)
            if archetype_index < archetype.len() {
                let swapped_meta = &mut self.entities[swapped_entity.index as usize];
                swapped_meta.archetype_index = archetype_index;
            }
        }

        self.free_entities.push(entity.index);
        true
    }

    /// Check if entity handle is valid
    pub fn is_valid(&self, entity: Entity) -> bool {
        if let Some(metadata) = self.entities.get(entity.index as usize) {
            metadata.alive && metadata.generation == entity.generation
        } else {
            false
        }
    }

    /// Add a component to an entity
    /// 
    /// **⚠️ INTERNAL USE ONLY - DO NOT CALL DIRECTLY ⚠️**
    #[doc(hidden)]
    pub fn add_component<T: Component + Clone>(&mut self, entity: Entity, component: T) {
        if !self.is_valid(entity) {
            return;
        }

        let type_id = TypeId::of::<T>();
        let metadata = &self.entities[entity.index as usize];
        let old_archetype_id = metadata.archetype_id;

        // Calculate new archetype
        let new_types = if old_archetype_id.0 == 0 {
            vec![type_id]
        } else {
            let mut types = self.archetypes
                .get(&old_archetype_id)
                .map(|a| a.component_types.clone())
                .unwrap_or_default();
            
            if types.contains(&type_id) {
                // Component already exists, update in place
                if let Some(archetype) = self.archetypes.get_mut(&old_archetype_id) {
                    if let Some(storage) = archetype.get_storage_mut::<T>() {
                        if let Some(existing) = storage.get_mut(metadata.archetype_index) {
                            *existing = component;
                        }
                    }
                }
                return;
            }
            
            types.push(type_id);
            types
        };

        let new_archetype_id = ArchetypeId::from_types(&new_types);

        // Ensure archetype exists
        if !self.archetypes.contains_key(&new_archetype_id) {
            let archetype = Archetype::new(new_archetype_id, new_types.clone());
            self.archetypes.insert(new_archetype_id, archetype);
        }

        // Ensure storage exists
        if !self.archetypes.get(&new_archetype_id).unwrap().components.contains_key(&type_id) {
            self.archetypes.get_mut(&new_archetype_id).unwrap().add_storage::<T>();
        }

        // Add to new archetype
        let new_archetype = self.archetypes.get_mut(&new_archetype_id).unwrap();
        let new_index = new_archetype.push_entity(entity);
        
        if let Some(storage) = new_archetype.get_storage_mut::<T>() {
            storage.push(component);
        }

        // Update metadata
        let metadata = &mut self.entities[entity.index as usize];
        metadata.archetype_id = new_archetype_id;
        metadata.archetype_index = new_index;
    }

    /// Add two components atomically
    #[doc(hidden)]
    pub fn add_component2<T1, T2>(&mut self, entity: Entity, c1: T1, c2: T2)
    where
        T1: Component + Clone,
        T2: Component + Clone,
    {
        if !self.is_valid(entity) {
            return;
        }

        let types = vec![TypeId::of::<T1>(), TypeId::of::<T2>()];
        let archetype_id = ArchetypeId::from_types(&types);

        if !self.archetypes.contains_key(&archetype_id) {
            let archetype = Archetype::new(archetype_id, types.clone());
            self.archetypes.insert(archetype_id, archetype);
        }

        let archetype = self.archetypes.get_mut(&archetype_id).unwrap();
        if !archetype.components.contains_key(&TypeId::of::<T1>()) {
            archetype.add_storage::<T1>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T2>()) {
            archetype.add_storage::<T2>();
        }

        let archetype = self.archetypes.get_mut(&archetype_id).unwrap();
        let index = archetype.push_entity(entity);
        
        if let Some(storage) = archetype.get_storage_mut::<T1>() {
            storage.push(c1);
        }
        if let Some(storage) = archetype.get_storage_mut::<T2>() {
            storage.push(c2);
        }

        let metadata = &mut self.entities[entity.index as usize];
        metadata.archetype_id = archetype_id;
        metadata.archetype_index = index;
    }

    /// Add three components atomically
    #[doc(hidden)]
    pub fn add_component3<T1, T2, T3>(&mut self, entity: Entity, c1: T1, c2: T2, c3: T3)
    where
        T1: Component + Clone,
        T2: Component + Clone,
        T3: Component + Clone,
    {
        if !self.is_valid(entity) {
            return;
        }

        let types = vec![TypeId::of::<T1>(), TypeId::of::<T2>(), TypeId::of::<T3>()];
        let archetype_id = ArchetypeId::from_types(&types);

        if !self.archetypes.contains_key(&archetype_id) {
            let archetype = Archetype::new(archetype_id, types.clone());
            self.archetypes.insert(archetype_id, archetype);
        }

        let archetype = self.archetypes.get_mut(&archetype_id).unwrap();
        if !archetype.components.contains_key(&TypeId::of::<T1>()) {
            archetype.add_storage::<T1>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T2>()) {
            archetype.add_storage::<T2>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T3>()) {
            archetype.add_storage::<T3>();
        }

        let archetype = self.archetypes.get_mut(&archetype_id).unwrap();
        let index = archetype.push_entity(entity);
        
        if let Some(storage) = archetype.get_storage_mut::<T1>() {
            storage.push(c1);
        }
        if let Some(storage) = archetype.get_storage_mut::<T2>() {
            storage.push(c2);
        }
        if let Some(storage) = archetype.get_storage_mut::<T3>() {
            storage.push(c3);
        }

        let metadata = &mut self.entities[entity.index as usize];
        metadata.archetype_id = archetype_id;
        metadata.archetype_index = index;
    }

    /// Get component reference
    pub fn get_component<T: Component>(&self, entity: Entity) -> Option<&T> {
        if !self.is_valid(entity) {
            return None;
        }

        let metadata = &self.entities[entity.index as usize];
        self.archetypes
            .get(&metadata.archetype_id)
            .and_then(|archetype| archetype.get_storage::<T>())
            .and_then(|storage| storage.get(metadata.archetype_index))
    }

    /// Get mutable component reference
    pub fn get_component_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        if !self.is_valid(entity) {
            return None;
        }

        let metadata = &self.entities[entity.index as usize];
        let archetype_id = metadata.archetype_id;
        let archetype_index = metadata.archetype_index;

        self.archetypes
            .get_mut(&archetype_id)
            .and_then(|archetype| archetype.get_storage_mut::<T>())
            .and_then(|storage| storage.get_mut(archetype_index))
    }

    /// Query all entities with component T
    pub fn query<T: Component>(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.archetypes
            .values()
            .filter(|archetype| archetype.has_component(TypeId::of::<T>()))
            .flat_map(|archetype| {
                let storage = archetype.get_storage::<T>().unwrap();
                archetype
                    .entities
                    .iter()
                    .zip(storage.data.iter())
                    .map(|(entity, component)| (*entity, component))
            })
    }

    /// Query all entities with components T1 and T2
    pub fn query2<T1: Component, T2: Component>(&self) -> impl Iterator<Item = (Entity, &T1, &T2)> {
        self.archetypes
            .values()
            .filter(|archetype| {
                archetype.has_component(TypeId::of::<T1>())
                    && archetype.has_component(TypeId::of::<T2>())
            })
            .flat_map(|archetype| {
                let storage1 = archetype.get_storage::<T1>().unwrap();
                let storage2 = archetype.get_storage::<T2>().unwrap();
                archetype
                    .entities
                    .iter()
                    .zip(storage1.data.iter())
                    .zip(storage2.data.iter())
                    .map(|((entity, c1), c2)| (*entity, c1, c2))
            })
    }

    /// Start building an entity (fluent API)
    pub fn entity(&mut self) -> EntityBuilder<'_> {
        let entity = self.spawn();
        EntityBuilder::new(self, entity)
    }
    
    /// Entity count
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }
    
    /// Live entity count
    pub fn live_entity_count(&self) -> usize {
        self.entities.iter().filter(|e| e.alive).count()
    }
    
    /// Archetype count
    pub fn archetype_count(&self) -> usize {
        self.archetypes.len()
    }
    
    /// Estimate component memory usage
    pub fn component_memory_bytes(&self) -> usize {
        let mut total = 0;
        total += self.entities.len() * std::mem::size_of::<EntityMetadata>();
        
        for archetype in self.archetypes.values() {
            total += archetype.entities.capacity() * std::mem::size_of::<Entity>();
            for _storage in archetype.components.values() {
                total += archetype.entities.len() * 32; // Conservative estimate
            }
        }
        
        total
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for adding multiple components before finalizing
pub struct EntityBuilder<'w> {
    world: &'w mut World,
    entity: Entity,
    component_types: Vec<TypeId>,
}

impl<'w> EntityBuilder<'w> {
    fn new(world: &'w mut World, entity: Entity) -> Self {
        Self {
            world,
            entity,
            component_types: Vec::new(),
        }
    }

    /// Add a component
    pub fn with<T: Component + Clone>(mut self, component: T) -> Self {
        let type_id = TypeId::of::<T>();
        self.component_types.push(type_id);
        self.world.add_component(self.entity, component);
        self
    }

    /// Finalize entity
    pub fn build(self) -> Entity {
        self.entity
    }
}
