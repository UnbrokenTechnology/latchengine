// world.rs - ECS World with entity management and queries
//
// The World owns all archetypes and provides APIs for spawning, despawning,
// and querying entities.

use crate::ecs::{
    ArchetypeId, ArchetypeStorage, Component, ComponentId, Entity, EntityBuilder,
};
use rayon::prelude::*;
use std::collections::{hash_map::Entry, HashMap};

/// The main ECS world containing all entities and components.
pub struct World {
    next_entity_id: u64,
    storages: HashMap<ArchetypeId, ArchetypeStorage>,
    comp_index: HashMap<ComponentId, Vec<ArchetypeId>>,
}

impl World {
    /// Create a new empty world.
    pub fn new() -> Self {
        Self {
            next_entity_id: 1,
            storages: HashMap::new(),
            comp_index: HashMap::new(),
        }
    }

    /// Spawn an entity from a builder.
    /// 
    /// The entity's archetype is calculated from its components and never changes.
    /// Uses object pooling to reuse despawned entity slots.
    pub fn spawn(&mut self, builder: EntityBuilder) -> Entity {
        let archetype = builder.archetype();

        // Get or create storage for this archetype
        let storage = match self.storages.entry(archetype.id) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => {
                // Create new storage
                let s = v.insert(ArchetypeStorage::new(archetype.clone()));
                
                // Update reverse index
                for &cid in &archetype.components {
                    self.comp_index.entry(cid).or_default().push(archetype.id);
                }
                
                s
            }
        };

        // Ensure all columns exist
        for &cid in &archetype.components {
            let meta = crate::ecs::meta_of(cid).expect("component not registered");
            storage.ensure_column(meta);
        }

        // Allocate entity ID and row
        let id = self.next_entity_id;
        self.next_entity_id += 1;

        let (row, generation) = storage.alloc_row();

        // Write components
        for (cid, bytes) in builder.into_components() {
            storage.write_component(row, cid, &bytes);
        }
        
        storage.set_entity(row, id);

        Entity::new(id, generation, archetype.id, row)
    }

    /// Despawn an entity.
    /// 
    /// The entity's slot is returned to the pool and its generation is incremented.
    /// This invalidates any stale handles pointing to the old entity.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if let Some(storage) = self.storages.get_mut(&entity.archetype) {
            if storage.validate_generation(entity) {
                storage.free_row(entity.index);
                return true;
            }
        }
        false
    }

    /// Get an immutable reference to a component.
    /// 
    /// Returns None if the entity is invalid or doesn't have the component.
    pub fn get_component<T: Component>(&self, entity: Entity) -> Option<&T> {
        let storage = self.storages.get(&entity.archetype)?;
        if !storage.validate_generation(entity) {
            return None;
        }
        storage.row_typed::<T>(entity.index)
    }

    /// Get a mutable reference to a component.
    /// 
    /// Returns None if the entity is invalid or doesn't have the component.
    pub fn get_component_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        let storage = self.storages.get_mut(&entity.archetype)?;
        if !storage.validate_generation(entity) {
            return None;
        }
        storage.row_typed_mut::<T>(entity.index)
    }

    /// Get an immutable typed column for a component in a specific archetype.
    pub fn column<T: Component>(&self, archetype: ArchetypeId) -> Option<&[T]> {
        self.storages.get(&archetype)?.column_as_slice::<T>()
    }

    /// Get a mutable typed column for a component in a specific archetype.
    pub fn column_mut<T: Component>(&mut self, archetype: ArchetypeId) -> Option<&mut [T]> {
        self.storages.get_mut(&archetype)?.column_as_slice_mut::<T>()
    }

    /// Get all archetype IDs that contain a specific component.
    pub fn archetypes_with(&self, cid: ComponentId) -> &[ArchetypeId] {
        self.comp_index.get(&cid).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Iterate over entities with a single component type.
    /// 
    /// Returns (Entity, &Component) for each entity.
    pub fn query<T: Component>(&self) -> impl Iterator<Item = (Entity, &T)> + '_ {
        let archetypes = self.archetypes_with(T::ID);
        
        archetypes.iter().flat_map(move |&arch_id| {
            let storage = self.storages.get(&arch_id)?;
            let slice = storage.column_as_slice::<T>()?;
            
            Some((0..storage.len()).filter_map(move |row| {
                let eid = storage.entity_at(row)?;
                let generation = storage.entity_generations[row];
                let entity = Entity::new(eid, generation, arch_id, row);
                Some((entity, &slice[row]))
            }))
        })
        .flatten()
    }

    /// Iterate over entities with two component types.
    /// 
    /// Returns (Entity, &C1, &C2) for each entity.
    pub fn query2<T1: Component, T2: Component>(
        &self,
    ) -> impl Iterator<Item = (Entity, &T1, &T2)> + '_ {
        let archs1 = self.archetypes_with(T1::ID);
        let archs2 = self.archetypes_with(T2::ID);
        
        // Find intersection
        let intersection: Vec<ArchetypeId> = archs1
            .iter()
            .filter(|a| archs2.contains(a))
            .copied()
            .collect();
        
        intersection.into_iter().flat_map(move |arch_id| {
            let storage = self.storages.get(&arch_id)?;
            let slice1 = storage.column_as_slice::<T1>()?;
            let slice2 = storage.column_as_slice::<T2>()?;
            
            Some((0..storage.len()).filter_map(move |row| {
                let eid = storage.entity_at(row)?;
                let generation = storage.entity_generations[row];
                let entity = Entity::new(eid, generation, arch_id, row);
                Some((entity, &slice1[row], &slice2[row]))
            }))
        })
        .flatten()
    }

    /// Parallel iteration over all components of a type.
    /// 
    /// Processes components in parallel chunks for efficiency.
    pub fn for_each<T, F>(&mut self, f: F)
    where
        T: Component + Send,
        F: Fn(&mut T) + Sync + Send,
    {
        let Some(ids) = self.comp_index.get(&T::ID) else {
            return;
        };

        for &arch_id in ids {
            if let Some(storage) = self.storages.get_mut(&arch_id) {
                if let Some(slice) = storage.column_as_slice_mut::<T>() {
                    slice.par_chunks_mut(1024).for_each(|chunk| {
                        for x in chunk {
                            f(x);
                        }
                    });
                }
            }
        }
    }

    /// Iterate over archetypes that have ALL of the specified components.
    /// 
    /// This is the low-level primitive for zero-cost multi-component iteration.
    /// Returns archetype IDs - use `archetype_storage()` to get the actual data.
    pub fn archetypes_with_all(&self, component_ids: &[ComponentId]) -> Vec<ArchetypeId> {
        if component_ids.is_empty() {
            return Vec::new();
        }
        
        // Start with archetypes that have the first component
        let mut result: Vec<ArchetypeId> = self.archetypes_with(component_ids[0]).to_vec();
        
        // Filter to only archetypes that have ALL components
        for &cid in &component_ids[1..] {
            let archs = self.archetypes_with(cid);
            result.retain(|a| archs.contains(a));
        }
        
        result
    }

    /// Get direct access to an archetype's storage.
    /// 
    /// This allows zero-cost access to component slices.
    pub fn archetype_storage(&self, archetype: ArchetypeId) -> Option<&ArchetypeStorage> {
        self.storages.get(&archetype)
    }

    /// Get mutable access to an archetype's storage.
    /// 
    /// This allows zero-cost access to mutable component slices.
    pub fn archetype_storage_mut(&mut self, archetype: ArchetypeId) -> Option<&mut ArchetypeStorage> {
        self.storages.get_mut(&archetype)
    }

    /// Get the total number of entities (including despawned slots).
    pub fn entity_count(&self) -> usize {
        self.storages.values().map(|s| s.len()).sum()
    }

    /// Get the number of live entities (excluding free slots).
    pub fn live_entity_count(&self) -> usize {
        self.storages.values().map(|s| s.len()).sum()
    }

    /// Get all archetype storages.
    pub fn archetypes(&self) -> impl Iterator<Item = &ArchetypeStorage> {
        self.storages.values()
    }

    /// Execute a function on all entities with the specified components.
    /// 
    /// This is the high-level ergonomic API. Use `columns_mut!` macro to extract
    /// component slices, then iterate with rayon for parallelism.
    /// 
    /// # Example
    /// ```ignore
    /// world.par_for_each(&[Position::ID, Velocity::ID], |storage| {
    ///     let (positions, velocities) = columns_mut!(storage, Position, Velocity);
    ///     positions.par_iter_mut().zip(velocities.par_iter_mut())
    ///         .for_each(|(pos, vel)| {
    ///             pos.x += vel.x * dt;
    ///             pos.y += vel.y * dt;
    ///         });
    /// });
    /// ```
    pub fn par_for_each<F>(&mut self, component_ids: &[ComponentId], mut f: F)
    where
        F: FnMut(&mut ArchetypeStorage),
    {
        let archetypes = self.archetypes_with_all(component_ids);
        
        for &arch_id in &archetypes {
            if let Some(storage) = self.storages.get_mut(&arch_id) {
                f(storage);
            }
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

