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

    /// Single spawn method that handles any number of components
    /// 
    /// The macro generates the type-specific storage pushes, but this method
    /// handles the archetype calculation and entity placement.
    #[doc(hidden)]
    pub fn spawn_with_components(
        &mut self,
        type_ids: Vec<TypeId>,
        insert_components: impl FnOnce(&mut Archetype),
    ) -> Entity {
        let entity = self.spawn();
        
        // Calculate archetype from component types
        let archetype_id = ArchetypeId::from_types(&type_ids);
        
        // Create archetype if it doesn't exist
        if !self.archetypes.contains_key(&archetype_id) {
            let archetype = Archetype::new(archetype_id, type_ids.clone());
            self.archetypes.insert(archetype_id, archetype);
        }
        
        // Let the macro-generated closure insert components into storage
        let archetype = self.archetypes.get_mut(&archetype_id).unwrap();
        insert_components(archetype);
        
        // Add entity to archetype (AFTER components are inserted)
        let index = archetype.entities.len();
        archetype.entities.push(entity);
        
        // Update entity metadata
        let metadata = &mut self.entities[entity.index as usize];
        metadata.archetype_id = archetype_id;
        metadata.archetype_index = index;
        
        entity
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
