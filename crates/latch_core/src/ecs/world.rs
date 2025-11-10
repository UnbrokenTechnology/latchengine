// world.rs - ECS World with entity management and iteration

use crate::ecs::{
    ArchetypeId, ArchetypeStorage, Component, ComponentId, Entity, EntityBuilder,
};
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
    /// 
    /// **Double-buffering note**: Component data is written to BOTH buffers during spawn
    /// to ensure the entity is immediately readable on the current tick and writable
    /// on the next tick. This prevents uninitialized data issues.
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

    /// Execute a function on all archetypes with the specified components.
    /// 
    /// **Internal API**: This is a low-level primitive for archetype iteration.
    /// Most users should use the higher-level `for_each_entity!` macro instead,
    /// which hides archetype details and provides a cleaner API.
    /// 
    /// Use `columns!` and `columns_mut!` macros to extract component slices.
    /// 
    /// # Example
    /// ```ignore
    /// use latch_core::{columns, columns_mut};
    /// 
    /// world.for_each_archetype_with_components(&[Position::ID, Velocity::ID], |storage| {
    ///     let positions = columns!(storage, Position);
    ///     let velocities = columns_mut!(storage, Velocity);
    ///     
    ///     // Iterate and update components
    ///     velocities.iter_mut().zip(positions.iter())
    ///         .for_each(|(vel, pos)| {
    ///             vel.x += pos.x * dt;
    ///             vel.y += pos.y * dt;
    ///         });
    /// });
    /// ```
    #[doc(hidden)]
    pub fn for_each_archetype_with_components<F>(&mut self, component_ids: &[ComponentId], mut f: F)
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

    /// Swap read/write buffers for all archetypes.
    /// 
    /// **Call this once per physics tick** after all systems have executed.
    /// This makes all writes from this tick visible for reading on the next tick,
    /// ensuring deterministic parallel updates.
    /// 
    /// # Determinism
    /// 
    /// Without double-buffering, the order in which entities are processed matters:
    /// - Entity A collides with Entity B
    /// - If A is processed first, it reads B's old velocity and writes A's new velocity
    /// - If B is processed first, it reads A's old velocity and writes B's new velocity
    /// - Different processing orders → different results (non-deterministic)
    /// 
    /// With double-buffering:
    /// - All entities read from the "current" buffer (stable state from last tick)
    /// - All entities write to the "next" buffer (new state for next tick)
    /// - Processing order doesn't matter because reads always see the same values
    /// - After all systems finish, swap buffers to make new state current
    /// 
    /// # Example
    /// ```ignore
    /// loop {
    ///     // Systems read from "current" buffer, write to "next" buffer
    ///     physics_system(&mut world, dt);
    ///     collision_system(&mut world);
    ///     
    ///     // Make next buffer current for the next tick
    ///     world.swap_buffers();
    /// }
    /// ```
    pub fn swap_buffers(&mut self) {
        for storage in self.storages.values_mut() {
            storage.swap_buffers();
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::Component;
    use crate::{define_component, spawn, for_each_entity};
    
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Position {
        x: i32,
        y: i32,
    }
    define_component!(Position, 100, "Position");
    
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Velocity {
        x: i16,
        y: i16,
    }
    define_component!(Velocity, 101, "Velocity");
    
    // Additional test components for testing arbitrary component counts
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Health { value: i32 }
    define_component!(Health, 102, "Health");
    
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Armor { value: i32 }
    define_component!(Armor, 103, "Armor");
    
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Damage { value: i32 }
    define_component!(Damage, 104, "Damage");
    
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Speed { value: i32 }
    define_component!(Speed, 105, "Speed");
    
    #[test]
    fn test_for_each_entity_macro() {
        Position::ensure_registered();
        Velocity::ensure_registered();
        
        let mut world = World::new();
        
        // Spawn some entities
        let e1 = spawn!(world, Position { x: 0, y: 0 }, Velocity { x: 1, y: 2 });
        let e2 = spawn!(world, Position { x: 10, y: 20 }, Velocity { x: 3, y: 4 });
        let e3 = spawn!(world, Position { x: 100, y: 200 }, Velocity { x: 5, y: 6 });
        
        // Use the new macro to update positions
        for_each_entity!(world, [Position, Velocity], |(pos_curr, vel_curr), (pos_next, vel_next)| {
            pos_next.x = pos_curr.x + vel_curr.x as i32;
            pos_next.y = pos_curr.y + vel_curr.y as i32;
            vel_next.x = vel_curr.x;
            vel_next.y = vel_curr.y;
        });
        
        // Swap buffers to make changes visible
        world.swap_buffers();
        
        // Check results using individual entity access
        assert_eq!(world.get_component::<Position>(e1), Some(&Position { x: 1, y: 2 }));
        assert_eq!(world.get_component::<Position>(e2), Some(&Position { x: 13, y: 24 }));
        assert_eq!(world.get_component::<Position>(e3), Some(&Position { x: 105, y: 206 }));
    }
    
    #[test]
    fn test_for_each_entity_macro_many_components() {
        // Test that the macro supports more than 5 components (arbitrary count)
        Position::ensure_registered();
        Velocity::ensure_registered();
        Health::ensure_registered();
        Armor::ensure_registered();
        Damage::ensure_registered();
        Speed::ensure_registered();
        
        let mut world = World::new();
        
        // Spawn an entity with 6 components
        let e1 = spawn!(
            world,
            Position { x: 10, y: 20 },
            Velocity { x: 1, y: 2 },
            Health { value: 100 },
            Armor { value: 50 },
            Damage { value: 25 },
            Speed { value: 5 }
        );
        
        // Use the macro with 6 components to verify arbitrary component support
        for_each_entity!(
            world,
            [Position, Velocity, Health, Armor, Damage, Speed],
            |(pos, vel, health, armor, damage, speed), (pos_n, vel_n, health_n, armor_n, damage_n, speed_n)| {
                // Simple update logic
                pos_n.x = pos.x + vel.x as i32;
                pos_n.y = pos.y + vel.y as i32;
                *vel_n = *vel;
                health_n.value = health.value + armor.value - damage.value;
                *armor_n = *armor;
                *damage_n = *damage;
                *speed_n = *speed;
            }
        );
        
        world.swap_buffers();
        
        // Verify the updates
        assert_eq!(world.get_component::<Position>(e1), Some(&Position { x: 11, y: 22 }));
        assert_eq!(world.get_component::<Health>(e1), Some(&Health { value: 125 })); // 100 + 50 - 25
    }
}

/// Convenience macro for spawning entities.
/// 
/// This provides a more ergonomic way to spawn entities with multiple components.
/// 
/// # Example
/// ```ignore
/// let entity = spawn!(world,
///     Position { x: 1.0, y: 2.0 },
///     Velocity { x: 0.5, y: 0.0 }
/// );
/// ```
#[macro_export]
macro_rules! spawn {
    ($world:expr, $($comp:expr),+ $(,)?) => {{
        let builder = $crate::ecs::EntityBuilder::new()
            $(.with($comp))+;
        $world.spawn(builder)
    }};
}

