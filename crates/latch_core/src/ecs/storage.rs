// storage.rs - SoA storage with alignment safety
//
// Each archetype has its own storage with columns for each component type.
// Object pooling via free lists enables efficient slot reuse.

use crate::ecs::{Archetype, Component, ComponentId, ComponentMeta, Entity, meta_of};
use std::collections::HashMap;

/// Storage for all entities of a single archetype.
/// 
/// Uses Structure-of-Arrays (SoA) layout for cache efficiency and
/// easy parallel iteration.
pub struct ArchetypeStorage {
    pub archetype: Archetype,
    columns: HashMap<ComponentId, Column>,
    len: usize,
    free: Vec<usize>,
    entity_ids: Vec<Option<u64>>,
    pub(crate) entity_generations: Vec<u32>,
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
        }
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
    pub fn write_component(&mut self, row: usize, cid: ComponentId, bytes: &[u8]) {
        let col = self
            .columns
            .get_mut(&cid)
            .expect("column not initialized");
        col.write_row(row, bytes);
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
    pub fn row_bytes(&self, cid: ComponentId, row: usize) -> Option<&[u8]> {
        let col = self.columns.get(&cid)?;
        let size = col.elem_size;
        let start = row * size;
        Some(&col.bytes[start..start + size])
    }

    /// Get typed reference to a component at a specific row.
    /// 
    /// # Safety
    /// Caller must ensure:
    /// - T matches the component type registered for T::ID
    /// - row is within bounds
    /// - The entity at row is alive
    pub fn row_typed<T: Component>(&self, row: usize) -> Option<&T> {
        let col = self.columns.get(&T::ID)?;
        let start = row * std::mem::size_of::<T>();
        
        // Safety: Column ensures proper alignment and layout
        Some(unsafe { &*(col.bytes[start..].as_ptr() as *const T) })
    }

    /// Get typed mutable reference to a component at a specific row.
    pub fn row_typed_mut<T: Component>(&mut self, row: usize) -> Option<&mut T> {
        let col = self.columns.get_mut(&T::ID)?;
        let start = row * std::mem::size_of::<T>();
        
        // Safety: Column ensures proper alignment and layout
        Some(unsafe { &mut *(col.bytes[start..].as_mut_ptr() as *mut T) })
    }

    /// Get an immutable typed slice of all components of type T.
    pub fn column_as_slice<T: Component>(&self) -> Option<&[T]> {
        let meta = meta_of(T::ID)?;
        let col = self.columns.get(&T::ID)?;
        
        assert_eq!(
            meta.size,
            col.elem_size,
            "Component size mismatch for {}",
            meta.name
        );
        
        // Alignment safety check
        debug_assert_eq!(
            col.bytes.as_ptr() as usize % std::mem::align_of::<T>(),
            0,
            "Column for {} is misaligned (expected align={})",
            meta.name,
            std::mem::align_of::<T>()
        );
        
        let ptr = col.bytes.as_ptr();
        let len = col.bytes.len() / meta.size;
        
        // Safety: Column guarantees proper alignment and tightly-packed POD layout
        Some(unsafe { std::slice::from_raw_parts(ptr as *const T, len) })
    }

    /// Get a mutable typed slice of all components of type T.
    pub fn column_as_slice_mut<T: Component>(&mut self) -> Option<&mut [T]> {
        let meta = meta_of(T::ID)?;
        let col = self.columns.get_mut(&T::ID)?;
        
        assert_eq!(
            meta.size,
            col.elem_size,
            "Component size mismatch for {}",
            meta.name
        );
        
        // Alignment safety check
        debug_assert_eq!(
            col.bytes.as_ptr() as usize % std::mem::align_of::<T>(),
            0,
            "Column for {} is misaligned (expected align={})",
            meta.name,
            std::mem::align_of::<T>()
        );
        
        let ptr = col.bytes.as_mut_ptr();
        let len = col.bytes.len() / meta.size;
        
        // Safety: Column guarantees proper alignment and tightly-packed POD layout
        Some(unsafe { std::slice::from_raw_parts_mut(ptr as *mut T, len) })
    }
}

/// A single component column (raw byte storage).
/// 
/// Uses aligned allocation to ensure safe transmutation to typed slices.
struct Column {
    elem_size: usize,
    elem_align: usize,
    bytes: Vec<u8>,
}

impl Column {
    /// Create a new column with proper alignment.
    fn new(meta: ComponentMeta) -> Self {
        Self {
            elem_size: meta.size,
            elem_align: meta.align,
            bytes: Vec::new(),
        }
    }

    /// Grow the column by one element.
    fn grow_one(&mut self) {
        let target = self.bytes.len() + self.elem_size;
        self.bytes.resize(target, 0);
        
        // Verify alignment (Vec<u8> typically has good alignment from allocator)
        debug_assert_eq!(
            self.bytes.as_ptr() as usize % self.elem_align,
            0,
            "Column lost alignment after resize (elem_align={})",
            self.elem_align
        );
    }

    /// Write component data to a specific row.
    fn write_row(&mut self, row: usize, src: &[u8]) {
        assert_eq!(
            src.len(),
            self.elem_size,
            "Component size mismatch: expected {}, got {}",
            self.elem_size,
            src.len()
        );
        
        let start = row * self.elem_size;
        self.bytes[start..start + self.elem_size].copy_from_slice(src);
    }
}
