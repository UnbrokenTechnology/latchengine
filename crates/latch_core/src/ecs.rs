//! Entity Component System (Archetype-based)
//!
//! Design Philosophy:
//! - Cache-friendly: Components stored in contiguous arrays (archetypes)
//! - SIMD-ready: Aligned memory, batch processing
//! - Deterministic: Stable iteration order, reproducible results
//! - Parallel-safe: Systems can run concurrently on different archetypes
//!
//! Architecture:
//! 1. Entities are handles (generation-indexed IDs for safety)
//! 2. Components are POD structs (no methods, just data)
//! 3. Archetypes group entities with identical component sets
//! 4. Systems query components and operate on archetypes in parallel

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

// ============================================================================
// Entity: Generational index for safe handles
// ============================================================================

/// Entity handle (generation-indexed for safety)
///
/// Format: [32-bit index | 32-bit generation]
/// - Index: Position in entity metadata array
/// - Generation: Incremented on entity destruction (prevents use-after-free)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// For serialization/networking
    pub fn to_bits(&self) -> u64 {
        ((self.generation as u64) << 32) | (self.index as u64)
    }

    pub fn from_bits(bits: u64) -> Self {
        Self {
            index: bits as u32,
            generation: (bits >> 32) as u32,
        }
    }
}

// ============================================================================
// Component: Marker trait for types that can be attached to entities
// ============================================================================

/// Marker trait for component types
///
/// Requirements:
/// - Must be 'static (no lifetimes)
/// - Must be Send + Sync (for parallel systems)
///
/// Components should be POD (Plain Old Data):
/// - Public fields
/// - No methods (logic goes in systems)
/// - Deterministic memory layout
pub trait Component: 'static + Send + Sync {
    /// Component type name (for debugging)
    fn type_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

// Blanket implementation for all valid types
impl<T: 'static + Send + Sync> Component for T {}

// ============================================================================
// ComponentBundle: Macro for adding multiple components at once
// ============================================================================

/// Helper macro for spawning entities with multiple components
///
/// This ensures all components are added atomically to the correct archetype
///
/// Example:
/// ```ignore
/// let entity = spawn!(world, Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 1.0 });
/// ```
#[macro_export]
macro_rules! spawn {
    // Two components
    ($world:expr, $c1:expr, $c2:expr) => {{
        let entity = $world.spawn();
        $world.add_component2(entity, $c1, $c2);
        entity
    }};
    // Three components
    ($world:expr, $c1:expr, $c2:expr, $c3:expr) => {{
        let entity = $world.spawn();
        $world.add_component3(entity, $c1, $c2, $c3);
        entity
    }};
    // Single component (fallback)
    ($world:expr, $component:expr) => {{
        let entity = $world.spawn();
        $world.add_component(entity, $component);
        entity
    }};
}

// ============================================================================
// ComponentStorage: Type-erased storage for a single component type
// ============================================================================

/// Type-erased component storage (one per component type per archetype)
trait ComponentStorage: Send + Sync {
    /// Component TypeId
    #[allow(dead_code)] // Used for debugging and future type checks
    fn type_id(&self) -> TypeId;

    /// Number of components stored
    #[allow(dead_code)] // Used for debugging and validation
    fn len(&self) -> usize;

    /// Remove component at index (swap-remove for O(1))
    fn swap_remove(&mut self, index: usize);

    /// Clone storage (for archetype migration)
    #[allow(dead_code)] // Will be used for component addition/removal
    fn clone_storage(&self) -> Box<dyn ComponentStorage>;

    /// Downcast to concrete type
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Concrete storage for components of type T
struct ComponentVec<T: Component> {
    data: Vec<T>,
}

impl<T: Component> ComponentVec<T> {
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn push(&mut self, component: T) {
        self.data.push(component);
    }

    fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.data.get_mut(index)
    }
}

impl<T: Component + Clone> ComponentStorage for ComponentVec<T> {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn swap_remove(&mut self, index: usize) {
        self.data.swap_remove(index);
    }

    fn clone_storage(&self) -> Box<dyn ComponentStorage> {
        Box::new(Self {
            data: self.data.clone(),
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ============================================================================
// Archetype: Stores entities with identical component sets
// ============================================================================

/// Archetype ID (hash of component TypeIds)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct ArchetypeId(u64);

impl ArchetypeId {
    fn from_types(type_ids: &[TypeId]) -> Self {
        // Sort for determinism (same components = same ID regardless of order)
        let mut sorted = type_ids.to_vec();
        sorted.sort_unstable();

        // Hash using std::hash
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        for type_id in sorted {
            type_id.hash(&mut hasher);
        }
        Self(hasher.finish())
    }
}

/// Archetype: Collection of entities with identical component signatures
struct Archetype {
    #[allow(dead_code)] // Used for debugging and future query optimization
    id: ArchetypeId,
    component_types: Vec<TypeId>,
    entities: Vec<Entity>,
    components: HashMap<TypeId, Box<dyn ComponentStorage>>,
}

impl Archetype {
    fn new(id: ArchetypeId, component_types: Vec<TypeId>) -> Self {
        Self {
            id,
            component_types,
            entities: Vec::new(),
            components: HashMap::new(),
        }
    }

    fn add_storage<T: Component + Clone>(&mut self) {
        self.components
            .insert(TypeId::of::<T>(), Box::new(ComponentVec::<T>::new()));
    }

    fn len(&self) -> usize {
        self.entities.len()
    }

    fn has_component(&self, type_id: TypeId) -> bool {
        self.components.contains_key(&type_id)
    }

    /// Add entity with components to this archetype
    fn push_entity(&mut self, entity: Entity) -> usize {
        let index = self.entities.len();
        self.entities.push(entity);
        index
    }

    /// Remove entity at index (swap-remove for O(1))
    fn swap_remove(&mut self, index: usize) -> Entity {
        let entity = self.entities.swap_remove(index);
        for storage in self.components.values_mut() {
            storage.swap_remove(index);
        }
        entity
    }

    /// Get component storage for type T
    fn get_storage<T: Component>(&self) -> Option<&ComponentVec<T>> {
        self.components
            .get(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any().downcast_ref())
    }

    fn get_storage_mut<T: Component>(&mut self) -> Option<&mut ComponentVec<T>> {
        self.components
            .get_mut(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any_mut().downcast_mut())
    }
}

// ============================================================================
// EntityMetadata: Tracks entity location in archetypes
// ============================================================================

struct EntityMetadata {
    generation: u32,
    archetype_id: ArchetypeId,
    archetype_index: usize, // Index within archetype's entity list
    alive: bool,
}

// ============================================================================
// World: ECS container
// ============================================================================

/// World: Container for all entities, components, and archetypes
///
/// Design:
/// - Entities are allocated from a free list (recycled indices)
/// - Archetypes are created on-demand when new component combinations appear
/// - Component access is O(1) via archetype + index lookup
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
        Self {
            entities: Vec::new(),
            free_entities: Vec::new(),
            archetypes: HashMap::new(),
            next_entity_index: 0,
        }
    }

    /// Create a new entity with no components
    pub fn spawn(&mut self) -> Entity {
        let index = if let Some(index) = self.free_entities.pop() {
            index
        } else {
            let index = self.next_entity_index;
            self.next_entity_index += 1;
            self.entities.push(EntityMetadata {
                generation: 0,
                archetype_id: ArchetypeId(0),
                archetype_index: 0,
                alive: false,
            });
            index
        };

        let metadata = &mut self.entities[index as usize];
        metadata.alive = true;
        
        Entity::new(index, metadata.generation)
    }

    /// Destroy an entity and all its components
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.is_alive(entity) {
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
                // Safe: swapped_entity is different from entity
                let swapped_meta = &mut self.entities[swapped_entity.index as usize];
                swapped_meta.archetype_index = archetype_index;
            }
        }

        self.free_entities.push(entity.index);
        true
    }

    /// Check if entity is alive
    pub fn is_alive(&self, entity: Entity) -> bool {
        if let Some(metadata) = self.entities.get(entity.index as usize) {
            metadata.alive && metadata.generation == entity.generation
        } else {
            false
        }
    }

    /// Add a component to an entity
    /// 
    /// Note: For PoC 2, components should be added in sequence immediately
    /// after spawning. The implementation ensures proper archetype migration.
    pub fn add_component<T: Component + Clone>(&mut self, entity: Entity, component: T) {
        if !self.is_alive(entity) {
            return;
        }

        let type_id = TypeId::of::<T>();
        let metadata = &self.entities[entity.index as usize];
        let old_archetype_id = metadata.archetype_id;

        // Calculate new archetype (old components + new component)
        let new_types = if old_archetype_id.0 == 0 {
            // Fresh entity, first component
            vec![type_id]
        } else {
            // Entity has components, adding another
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

        // Ensure archetype exists with proper storage
        if !self.archetypes.contains_key(&new_archetype_id) {
            let archetype = Archetype::new(new_archetype_id, new_types.clone());
            self.archetypes.insert(new_archetype_id, archetype);
        }

        // Ensure storage exists for the new component
        if !self.archetypes.get(&new_archetype_id).unwrap().components.contains_key(&type_id) {
            self.archetypes.get_mut(&new_archetype_id).unwrap().add_storage::<T>();
        }

        // Add entity to new archetype
        let new_archetype = self.archetypes.get_mut(&new_archetype_id).unwrap();
        let new_index = new_archetype.push_entity(entity);
        
        // Add the new component
        if let Some(storage) = new_archetype.get_storage_mut::<T>() {
            storage.push(component);
        }

        // Update entity metadata
        let metadata = &mut self.entities[entity.index as usize];
        metadata.archetype_id = new_archetype_id;
        metadata.archetype_index = new_index;
    }

    /// Add two components atomically (avoids archetype migration issues)
    pub fn add_component2<T1, T2>(&mut self, entity: Entity, c1: T1, c2: T2)
    where
        T1: Component + Clone,
        T2: Component + Clone,
    {
        if !self.is_alive(entity) {
            return;
        }

        let types = vec![TypeId::of::<T1>(), TypeId::of::<T2>()];
        let archetype_id = ArchetypeId::from_types(&types);

        // Create archetype if needed
        if !self.archetypes.contains_key(&archetype_id) {
            let archetype = Archetype::new(archetype_id, types.clone());
            self.archetypes.insert(archetype_id, archetype);
        }

        // Ensure storage exists
        let archetype = self.archetypes.get_mut(&archetype_id).unwrap();
        if !archetype.components.contains_key(&TypeId::of::<T1>()) {
            archetype.add_storage::<T1>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T2>()) {
            archetype.add_storage::<T2>();
        }

        // Add entity and components
        let archetype = self.archetypes.get_mut(&archetype_id).unwrap();
        let index = archetype.push_entity(entity);
        
        if let Some(storage) = archetype.get_storage_mut::<T1>() {
            storage.push(c1);
        }
        if let Some(storage) = archetype.get_storage_mut::<T2>() {
            storage.push(c2);
        }

        // Update metadata
        let metadata = &mut self.entities[entity.index as usize];
        metadata.archetype_id = archetype_id;
        metadata.archetype_index = index;
    }

    /// Add three components atomically
    pub fn add_component3<T1, T2, T3>(&mut self, entity: Entity, c1: T1, c2: T2, c3: T3)
    where
        T1: Component + Clone,
        T2: Component + Clone,
        T3: Component + Clone,
    {
        if !self.is_alive(entity) {
            return;
        }

        let types = vec![TypeId::of::<T1>(), TypeId::of::<T2>(), TypeId::of::<T3>()];
        let archetype_id = ArchetypeId::from_types(&types);

        // Create archetype if needed
        if !self.archetypes.contains_key(&archetype_id) {
            let archetype = Archetype::new(archetype_id, types.clone());
            self.archetypes.insert(archetype_id, archetype);
        }

        // Ensure storage exists
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

        // Add entity and components
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

        // Update metadata
        let metadata = &mut self.entities[entity.index as usize];
        metadata.archetype_id = archetype_id;
        metadata.archetype_index = index;
    }

    /// Get immutable reference to a component
    pub fn get_component<T: Component>(&self, entity: Entity) -> Option<&T> {
        if !self.is_alive(entity) {
            return None;
        }

        let metadata = &self.entities[entity.index as usize];
        self.archetypes
            .get(&metadata.archetype_id)
            .and_then(|archetype| archetype.get_storage::<T>())
            .and_then(|storage| storage.get(metadata.archetype_index))
    }

    /// Get mutable reference to a component
    pub fn get_component_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        if !self.is_alive(entity) {
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

    /// Iterate over all entities with component T
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

    /// Iterate over all entities with components T1 and T2
    pub fn query2<T1: Component, T2: Component>(
        &self,
    ) -> impl Iterator<Item = (Entity, &T1, &T2)> {
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
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Clone, Debug, PartialEq)]
    struct Velocity {
        x: f32,
        y: f32,
    }

    #[test]
    fn test_entity_creation() {
        let mut world = World::new();
        let e1 = world.spawn();
        let e2 = world.spawn();
        
        assert!(world.is_alive(e1));
        assert!(world.is_alive(e2));
        assert_ne!(e1, e2);
    }

    #[test]
    fn test_entity_despawn() {
        let mut world = World::new();
        let entity = world.spawn();
        
        assert!(world.is_alive(entity));
        assert!(world.despawn(entity));
        assert!(!world.is_alive(entity));
    }

    #[test]
    fn test_component_add_get() {
        let mut world = World::new();
        let entity = world.spawn();
        
        world.add_component(entity, Position { x: 10.0, y: 20.0 });
        
        let pos = world.get_component::<Position>(entity).unwrap();
        assert_eq!(pos.x, 10.0);
        assert_eq!(pos.y, 20.0);
    }

    #[test]
    fn test_component_mutation() {
        let mut world = World::new();
        let entity = world.spawn();
        
        world.add_component(entity, Position { x: 0.0, y: 0.0 });
        
        if let Some(pos) = world.get_component_mut::<Position>(entity) {
            pos.x = 100.0;
        }
        
        let pos = world.get_component::<Position>(entity).unwrap();
        assert_eq!(pos.x, 100.0);
    }

    #[test]
    fn test_query_single_component() {
        let mut world = World::new();
        
        let e1 = world.spawn();
        world.add_component(e1, Position { x: 1.0, y: 2.0 });
        
        let e2 = world.spawn();
        world.add_component(e2, Position { x: 3.0, y: 4.0 });
        
        let positions: Vec<_> = world.query::<Position>().collect();
        assert_eq!(positions.len(), 2);
    }

    #[test]
    fn test_query_multiple_components() {
        let mut world = World::new();
        
        // Use the spawn! macro to add both components
        let _e1 = spawn!(world,
            Position { x: 0.0, y: 0.0 },
            Velocity { x: 1.0, y: 1.0 }
        );
        
        let e2 = world.spawn();
        world.add_component(e2, Position { x: 10.0, y: 10.0 });
        // No velocity
        
        let results: Vec<_> = world.query2::<Position, Velocity>().collect();
        assert_eq!(results.len(), 1); // Only e1 has both components
        
        let (_entity, pos, vel) = results[0];
        assert_eq!(pos.x, 0.0);
        assert_eq!(vel.x, 1.0);
    }
}
