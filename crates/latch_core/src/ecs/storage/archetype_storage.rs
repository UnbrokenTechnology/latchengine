// archetype_storage.rs - SoA storage for archetype entities
//
// Implements Structure-of-Arrays layout for cache-efficient entity storage.

use crate::ecs::{Archetype, Component, ComponentId, ComponentMeta, Entity, meta_of};
use std::collections::HashMap;

use super::column::Column;

/// Storage for all entities of a single archetype.
/// 
/// Uses Structure-of-Arrays (SoA) layout for cache efficiency and
/// easy parallel iteration.
/// 
/// Uses double-buffering for deterministic parallel updates:
/// - Read from "current" buffer (stable state)
/// - Write to "next" buffer (new state)
/// - Swap buffers each physics tick
pub struct ArchetypeStorage {
    pub archetype: Archetype, // The archetype this storage represents

    pub(crate) columns: HashMap<ComponentId, Column>, // Maps component ID to its column

    entity_ids: Vec<u64>, // Maps row index to entity ID

    current_buffer: u8, // Which buffer is currently being read from (0 or 1)

    len: usize, // Number of entities currently stored
    capacity: usize,  // Number of elements we have space for
}

impl ArchetypeStorage {
    /// Initial capacity for new vectors (1024 elements).
    const INITIAL_CAPACITY: usize = 1024;

    /// Create new storage for an archetype.
    pub fn new(archetype: Archetype) -> Self {
        let mut columns = HashMap::new();
        for &cid in &archetype.components {
            let meta = crate::ecs::meta_of(cid).expect("component not registered");
            columns.insert(meta.id, Column::new(meta, Self::INITIAL_CAPACITY));
        }

        Self {
            archetype,
            columns,
            entity_ids: Vec::with_capacity(Self::INITIAL_CAPACITY),
            current_buffer: 0,
            len: 0,
            capacity: Self::INITIAL_CAPACITY,
        }
    }

    /// Get the index of the current buffer (the one being read from).
    pub fn current_buffer_index(&self) -> u8 {
        self.current_buffer
    }

    /// Get the index of the next buffer (the one being written to).
    pub fn next_buffer_index(&self) -> u8 {
        1 - self.current_buffer
    }

    /// Swap read/write buffers.
    /// 
    /// Call this once per physics tick after all systems have executed.
    /// This makes the "write" buffer become the new "read" buffer.
    /// 
    /// **Performance**: This is just an index flip - O(1) operation, no memcpy!
    /// The buffers themselves stay in place; we just swap which one is "current".
    pub fn swap_buffers(&mut self) {
        self.current_buffer = self.current_buffer ^ 1;
    }

    /// Allocate a new row
    pub fn add_entity(&mut self, id: u64) -> (usize) {
        let row = self.len;
        self.len += 1;
        for col in self.columns.values_mut() {
            col.ensure_capacity(self.len);
        }
        self.entity_ids.push(id);
        row
    }

    /// Free a row (returns to pool and increments generation).
    pub fn remove_entity(&mut self, idx: usize) {
        self.entity_ids[idx] = None;
        self.entity_generations[idx] = self.entity_generations[idx].wrapping_add(1);
        self.free.push(idx);
    }

    /// Write component data to a specific row.
    /// 
    /// Writes to **both** buffers to ensure data is immediately available.
    /// This is necessary for entity spawning - we need the data to be readable
    /// on the current tick, not just after the next buffer swap.
    pub fn write_component(&mut self, row: usize, cid: ComponentId, bytes: &[u8]) {
        let col = self
            .columns
            .get_mut(&cid)
            .expect("column not initialized");
        
        // Write to BOTH buffers so data is immediately readable and writable
        col.write_row(0, row, bytes);
        col.write_row(1, row, bytes);
    }

    /// Record which entity is at a given row.
    pub fn set_entity(&mut self, row: usize, eid: u64) {
        self.entity_ids[row] = Some(eid);
    }

    /// Get the entity ID at a given row.
    pub fn entity_at(&self, row: usize) -> Option<u64> {
        self.entity_ids.get(row).and_then(|x| *x)
    }

    /// Validate that an entity handle's generation matches the current generation.
    pub fn validate_generation(&self, entity: Entity) -> bool {
        if entity.index >= self.entity_generations.len() {
            return false;
        }
        self.entity_generations[entity.index] == entity.generation
    }

    /// Get the number of live entities (total minus free slots).
    pub fn len(&self) -> usize {
        self.len - self.free.len()
    }

    /// Check if storage is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get raw bytes for a component at a specific row.
    /// 
    /// Reads from the "current" buffer (stable state).
    pub fn row_bytes(&self, cid: ComponentId, row: usize) -> Option<&[u8]> {
        let col = self.columns.get(&cid)?;
        let size = col.elem_size;
        let start = row * size;
        let bytes = col.current_bytes(self.current_buffer);
        Some(&bytes[start..start + size])
    }

    /// Get typed reference to a component at a specific row.
    /// 
    /// Reads from the "current" buffer (stable state).
    /// 
    /// # Safety
    /// Caller must ensure:
    /// - T matches the component type registered for T::ID
    /// - row is within bounds
    /// - The entity at row is alive
    pub fn row_typed<T: Component>(&self, row: usize) -> Option<&T> {
        let col = self.columns.get(&T::ID)?;
        let start = row * std::mem::size_of::<T>();
        let bytes = col.current_bytes(self.current_buffer);
        
        // Safety: Column ensures proper alignment and layout
        Some(unsafe { &*(bytes[start..].as_ptr() as *const T) })
    }

    /// Get typed mutable reference to a component at a specific row.
    /// 
    /// Writes to the "next" buffer (the one not currently being read from).
    pub fn row_typed_mut<T: Component>(&mut self, row: usize) -> Option<&mut T> {
        let col = self.columns.get_mut(&T::ID)?;
        let start = row * std::mem::size_of::<T>();
        let next_buffer = 1 - self.current_buffer;
        let bytes = col.next_bytes_mut(next_buffer);
        
        // Safety: Column ensures proper alignment and layout
        Some(unsafe { &mut *(bytes[start..].as_mut_ptr() as *mut T) })
    }

    /// Get an immutable typed slice of all components of type T.
    /// 
    /// Reads from the "current" buffer (stable state).
    pub fn column_as_slice<T: Component>(&self) -> Option<&[T]> {
        let meta = meta_of(T::ID)?;
        let col = self.columns.get(&T::ID)?;
        
        assert_eq!(
            meta.size,
            col.elem_size,
            "Component size mismatch for {}",
            meta.name
        );
        
        let bytes = col.current_bytes(self.current_buffer);
        
        // Alignment safety check
        debug_assert_eq!(
            bytes.as_ptr() as usize % std::mem::align_of::<T>(),
            0,
            "Column for {} is misaligned (expected align={})",
            meta.name,
            std::mem::align_of::<T>()
        );
        
        let ptr = bytes.as_ptr();
        let len = self.len;
        
        // Safety: Column guarantees proper alignment and tightly-packed POD layout
        Some(unsafe { std::slice::from_raw_parts(ptr as *const T, len) })
    }

    /// Get a mutable typed slice of all components of type T.
    /// 
    /// Writes to the "next" buffer (the one not currently being read from).
    pub fn column_as_slice_mut<T: Component>(&mut self) -> Option<&mut [T]> {
        let meta = meta_of(T::ID)?;
        let col = self.columns.get_mut(&T::ID)?;
        
        assert_eq!(
            meta.size,
            col.elem_size,
            "Component size mismatch for {}",
            meta.name
        );
        
        let next_buffer = 1 - self.current_buffer;
        let bytes = col.next_bytes_mut(next_buffer);
        
        // Alignment safety check
        debug_assert_eq!(
            bytes.as_ptr() as usize % std::mem::align_of::<T>(),
            0,
            "Column for {} is misaligned (expected align={})",
            meta.name,
            std::mem::align_of::<T>()
        );
        
        let ptr = bytes.as_mut_ptr();
        let len = self.len;
        
        // Safety: Column guarantees proper alignment and tightly-packed POD layout
        Some(unsafe { std::slice::from_raw_parts_mut(ptr as *mut T, len) })
    }

    /// Get raw const pointer to a column for use in the columns! macro.
    /// 
    /// # Safety
    /// This returns a raw pointer that the caller must use safely.
    /// The macro ensures uniqueness of component IDs before dereferencing.
    #[doc(hidden)]
    pub unsafe fn get_column_ptr_const(&self, cid: ComponentId) -> Option<*const u8> {
        self.columns.get(&cid).map(|col| col as *const Column as *const u8)
    }

    /// Get raw mutable pointer to a column for use in the columns_mut! macro.
    /// 
    /// # Safety
    /// This returns a raw pointer that the caller must use safely.
    /// The macro ensures uniqueness of component IDs before dereferencing.
    #[doc(hidden)]
    pub unsafe fn get_column_ptr(&mut self, cid: ComponentId) -> Option<*mut u8> {
        self.columns.get_mut(&cid).map(|col| col as *mut Column as *mut u8)
    }

    /// Convert a raw column pointer to a typed immutable slice.
    /// 
    /// # Safety
    /// - ptr must be a valid pointer returned from get_column_ptr_const
    /// - T must match the actual component type stored in that column
    /// - buffer_idx must be valid (0 or 1)
    #[doc(hidden)]
    pub unsafe fn column_ptr_to_slice_const<T: Component>(ptr: *const u8, buffer_idx: usize) -> &'static [T] {
        let col = &*(ptr as *const Column);
        col.as_slice::<T>(buffer_idx)
    }

    /// Convert a raw column pointer to a typed mutable slice.
    /// 
    /// # Safety
    /// - ptr must be a valid pointer returned from get_column_ptr
    /// - T must match the actual component type stored in that column
    /// - No other references to this column may exist
    /// - buffer_idx must be valid (0 or 1)
    #[doc(hidden)]
    pub unsafe fn column_ptr_to_slice<T: Component>(ptr: *mut u8, buffer_idx: usize) -> &'static mut [T] {
        let col = &mut *(ptr as *mut Column);
        col.as_slice_mut::<T>(buffer_idx)
    }
}
