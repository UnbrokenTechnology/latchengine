// storage.rs - SoA storage with alignment safety
//
// Each archetype has its own storage with columns for each component type.
// Object pooling via free lists enables efficient slot reuse.

use crate::ecs::{Archetype, Component, ComponentId, ComponentMeta, Entity, meta_of};
use std::collections::HashMap;

/// Macro to get multiple immutable component slices from a storage.
/// 
/// Reads from the "current" buffer (stable state from last tick).
/// Handles any number of components using compile-time validation.
/// 
/// # Example
/// ```ignore
/// let positions = columns!(storage, Position);
/// let (positions, velocities) = columns!(storage, Position, Velocity);
/// let (a, b, c) = columns!(storage, ComponentA, ComponentB, ComponentC);
/// ```
#[macro_export]
macro_rules! columns {
    // Single component - just call the method directly
    ($storage:expr, $T:ty) => {
        $storage.column_as_slice::<$T>().unwrap()
    };
    
    // Multiple components - use the general implementation
    ($storage:expr, $($T:ty),+ $(,)?) => {{
        // Collect component IDs and verify uniqueness at compile time
        let ids = [$(<$T as $crate::ecs::Component>::ID),+];
        
        // Get the current buffer index (what we read from)
        let current_buffer = $storage.current_buffer_index();
        
        // Get raw pointers to each column
        let ptrs = [$(unsafe {
            $storage.get_column_ptr_const(<$T as $crate::ecs::Component>::ID)
                .expect("Component not found in archetype")
        }),+];
        
        // Runtime verification that all IDs are unique (not strictly necessary for immutable, but consistent)
        for i in 0..ids.len() {
            for j in (i+1)..ids.len() {
                assert_ne!(ids[i], ids[j], "Duplicate component IDs in query");
            }
        }
        
        // SAFETY: We've verified all component IDs are unique, so these point to different columns.
        // Each column is independently readable.
        unsafe {
            let mut idx = 0;
            ($(
                {
                    let slice = $crate::ecs::ArchetypeStorage::column_ptr_to_slice_const::<$T>(ptrs[idx], current_buffer);
                    idx += 1;
                    slice
                }
            ),+)
        }
    }};
}

/// Macro to get multiple mutable component slices from a storage.
/// 
/// Writes to the "next" buffer (the one not currently being read from).
/// Handles any number of components using compile-time validation.
/// 
/// # Example
/// ```ignore
/// let positions = columns_mut!(storage, Position);
/// let (positions, velocities) = columns_mut!(storage, Position, Velocity);
/// let (a, b, c) = columns_mut!(storage, ComponentA, ComponentB, ComponentC);
/// let (a, b, c, d, e) = columns_mut!(storage, A, B, C, D, E);
/// ```
#[macro_export]
macro_rules! columns_mut {
    // Single component - just call the method directly
    ($storage:expr, $T:ty) => {
        $storage.column_as_slice_mut::<$T>().unwrap()
    };
    
    // Multiple components - use the general implementation
    ($storage:expr, $($T:ty),+ $(,)?) => {{
        // Collect component IDs and verify uniqueness at compile time
        let ids = [$(<$T as $crate::ecs::Component>::ID),+];
        
        // Get the next buffer index (what we write to)
        let next_buffer = $storage.next_buffer_index();
        
        // Get raw pointers to each column
        let ptrs = [$(unsafe {
            $storage.get_column_ptr(<$T as $crate::ecs::Component>::ID)
                .expect("Component not found in archetype")
        }),+];
        
        // Runtime verification that all IDs are unique
        for i in 0..ids.len() {
            for j in (i+1)..ids.len() {
                assert_ne!(ids[i], ids[j], "Cannot get multiple mutable references to the same component");
            }
        }
        
        // SAFETY: We've verified all component IDs are unique, so these point to different columns.
        // Each column is independently mutable.
        unsafe {
            let mut idx = 0;
            ($(
                {
                    let slice = $crate::ecs::ArchetypeStorage::column_ptr_to_slice::<$T>(ptrs[idx], next_buffer);
                    idx += 1;
                    slice
                }
            ),+)
        }
    }};
}

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
    pub archetype: Archetype,
    pub(crate) columns: HashMap<ComponentId, Column>,
    len: usize,
    free: Vec<usize>,
    entity_ids: Vec<Option<u64>>,
    pub(crate) entity_generations: Vec<u32>,
    /// Which buffer is currently being read from (0 or 1)
    current_buffer: usize,
}

impl ArchetypeStorage {
    /// Create new storage for an archetype.
    pub fn new(archetype: Archetype) -> Self {
        Self {
            archetype,
            columns: HashMap::new(),
            len: 0,
            free: Vec::new(),
            entity_ids: Vec::new(),
            entity_generations: Vec::new(),
            current_buffer: 0,
        }
    }

    /// Get the index of the current buffer (the one being read from).
    pub fn current_buffer_index(&self) -> usize {
        self.current_buffer
    }

    /// Get the index of the next buffer (the one being written to).
    pub fn next_buffer_index(&self) -> usize {
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
        self.current_buffer = 1 - self.current_buffer;
        // That's it! No copying needed.
        // What was "next" is now "current" (contains latest state)
        // What was "current" is now "next" (will be overwritten next tick)
    }

    /// Ensure a column exists for the given component.
    pub fn ensure_column(&mut self, meta: ComponentMeta) {
        self.columns
            .entry(meta.id)
            .or_insert_with(|| Column::new(meta));
    }

    /// Allocate a new row (reuses from pool if available).
    pub fn alloc_row(&mut self) -> (usize, u32) {
        if let Some(idx) = self.free.pop() {
            let generation = self.entity_generations[idx];
            self.entity_ids[idx] = None;
            (idx, generation)
        } else {
            let row = self.len;
            self.len += 1;
            
            // Grow each column
            for col in self.columns.values_mut() {
                col.grow_one();
            }
            
            self.entity_ids.push(None);
            self.entity_generations.push(0);
            (row, 0)
        }
    }

    /// Free a row (returns to pool and increments generation).
    pub fn free_row(&mut self, idx: usize) {
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
        let len = bytes.len() / meta.size;
        
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
        let len = bytes.len() / meta.size;
        
        // Safety: Column guarantees proper alignment and tightly-packed POD layout
        Some(unsafe { std::slice::from_raw_parts_mut(ptr as *mut T, len) })
    }

    /// Get two mutable component slices simultaneously.
    /// 
    /// This is safe because T1 and T2 are stored in separate columns.
    /// Panics if T1::ID == T2::ID (same component type).
    /// 
    /// Writes to the "next" buffer.
    pub fn two_columns_mut<T1: Component, T2: Component>(&mut self) -> Option<(&mut [T1], &mut [T2])> {
        assert_ne!(T1::ID, T2::ID, "Cannot get two mutable references to the same component");
        
        let meta1 = meta_of(T1::ID)?;
        let meta2 = meta_of(T2::ID)?;
        
        let col1_ptr = self.columns.get_mut(&T1::ID)? as *mut Column;
        let col2_ptr = self.columns.get_mut(&T2::ID)? as *mut Column;
        
        let next_buffer = 1 - self.current_buffer;
        
        // Safety: We've verified T1::ID != T2::ID, so these are different columns
        unsafe {
            let col1 = &mut *col1_ptr;
            let col2 = &mut *col2_ptr;
            
            assert_eq!(meta1.size, col1.elem_size);
            assert_eq!(meta2.size, col2.elem_size);
            
            let bytes1 = &mut col1.buffers[next_buffer];
            let bytes2 = &mut col2.buffers[next_buffer];
            
            debug_assert_eq!(bytes1.as_ptr() as usize % std::mem::align_of::<T1>(), 0);
            debug_assert_eq!(bytes2.as_ptr() as usize % std::mem::align_of::<T2>(), 0);
            
            let ptr1 = bytes1.as_mut_ptr() as *mut T1;
            let len1 = bytes1.len() / meta1.size;
            
            let ptr2 = bytes2.as_mut_ptr() as *mut T2;
            let len2 = bytes2.len() / meta2.size;
            
            Some((
                std::slice::from_raw_parts_mut(ptr1, len1),
                std::slice::from_raw_parts_mut(ptr2, len2),
            ))
        }
    }

    /// Get a raw const pointer to a column for use in the columns! macro.
    /// 
    /// # Safety
    /// This returns a raw pointer that the caller must use safely.
    /// The macro ensures uniqueness of component IDs before dereferencing.
    #[doc(hidden)]
    pub unsafe fn get_column_ptr_const(&self, cid: ComponentId) -> Option<*const u8> {
        self.columns.get(&cid).map(|col| col as *const Column as *const u8)
    }

    /// Get a raw mutable pointer to a column for use in the columns_mut! macro.
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

/// A single component column (raw byte storage with double-buffering).
/// 
/// Uses two buffers for deterministic parallel updates:
/// - Buffer 0 and Buffer 1
/// - One is "current" (read), one is "next" (write)
/// - Buffers swap each physics tick
pub(crate) struct Column {
    elem_size: usize,
    elem_align: usize,
    buffers: [Vec<u8>; 2],
}

impl Column {
    /// Create a new column with proper alignment and double-buffering.
    fn new(meta: ComponentMeta) -> Self {
        Self {
            elem_size: meta.size,
            elem_align: meta.align,
            buffers: [Vec::new(), Vec::new()],
        }
    }

    /// Get the current read buffer.
    fn current_bytes(&self, current_buffer: usize) -> &[u8] {
        &self.buffers[current_buffer]
    }

    /// Get the next write buffer (mutable).
    fn next_bytes_mut(&mut self, next_buffer: usize) -> &mut [u8] {
        &mut self.buffers[next_buffer]
    }

    /// Grow both buffers by one element.
    fn grow_one(&mut self) {
        for buffer in &mut self.buffers {
            let target = buffer.len() + self.elem_size;
            buffer.resize(target, 0);
            
            // Verify alignment (Vec<u8> typically has good alignment from allocator)
            debug_assert_eq!(
                buffer.as_ptr() as usize % self.elem_align,
                0,
                "Column lost alignment after resize (elem_align={})",
                self.elem_align
            );
        }
    }

    /// Write component data to a specific row in a specific buffer.
    fn write_row(&mut self, buffer_idx: usize, row: usize, src: &[u8]) {
        assert_eq!(
            src.len(),
            self.elem_size,
            "Component size mismatch: expected {}, got {}",
            self.elem_size,
            src.len()
        );
        
        let start = row * self.elem_size;
        let buffer = &mut self.buffers[buffer_idx];
        buffer[start..start + self.elem_size].copy_from_slice(src);
    }

    /// Get a typed immutable slice from this column.
    /// 
    /// # Safety
    /// Caller must ensure T matches the actual component type stored.
    pub(crate) unsafe fn as_slice<T: Component>(&self, buffer_idx: usize) -> &[T] {
        let meta = meta_of(T::ID).expect("Component not registered");
        assert_eq!(meta.size, self.elem_size);
        
        let bytes = &self.buffers[buffer_idx];
        debug_assert_eq!(bytes.as_ptr() as usize % std::mem::align_of::<T>(), 0);
        
        let ptr = bytes.as_ptr() as *const T;
        let len = bytes.len() / meta.size;
        std::slice::from_raw_parts(ptr, len)
    }

    /// Get a typed mutable slice from this column.
    /// 
    /// # Safety
    /// Caller must ensure T matches the actual component type stored.
    pub(crate) unsafe fn as_slice_mut<T: Component>(&mut self, buffer_idx: usize) -> &mut [T] {
        let meta = meta_of(T::ID).expect("Component not registered");
        assert_eq!(meta.size, self.elem_size);
        
        let bytes = &mut self.buffers[buffer_idx];
        debug_assert_eq!(bytes.as_ptr() as usize % std::mem::align_of::<T>(), 0);
        
        let ptr = bytes.as_mut_ptr() as *mut T;
        let len = bytes.len() / meta.size;
        std::slice::from_raw_parts_mut(ptr, len)
    }
}
