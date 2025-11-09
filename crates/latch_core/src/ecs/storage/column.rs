// column.rs - Component column storage with double-buffering
//
// Each column stores all instances of a single component type in a
// Structure-of-Arrays layout for cache efficiency.
//
// # Double-Buffering
//
// Each column maintains two buffers to enable deterministic parallel updates:
// - Systems read from the "current" buffer (stable state from last tick)
// - Systems write to the "next" buffer (new state for next tick)
// - After all systems complete, buffers are swapped
//
// This ensures that processing order doesn't affect results - all systems
// see the same input state regardless of execution order.

use crate::ecs::{Component, ComponentMeta, meta_of};

/// A single component column (raw byte storage with double-buffering).
/// 
/// Uses two buffers for deterministic parallel updates:
/// - Buffer 0 and Buffer 1
/// - One is "current" (read), one is "next" (write)
/// - Buffers swap each physics tick
pub(crate) struct Column {
    pub(crate) elem_size: usize,
    pub(crate) elem_align: usize,
    pub(crate) buffers: [Vec<u8>; 2],
}

impl Column {
    /// Create a new column with proper alignment and double-buffering.
    pub(crate) fn new(meta: ComponentMeta) -> Self {
        Self {
            elem_size: meta.size,
            elem_align: meta.align,
            buffers: [Vec::new(), Vec::new()],
        }
    }

    /// Get the current read buffer.
    pub(crate) fn current_bytes(&self, current_buffer: usize) -> &[u8] {
        &self.buffers[current_buffer]
    }

    /// Get the next write buffer (mutable).
    pub(crate) fn next_bytes_mut(&mut self, next_buffer: usize) -> &mut [u8] {
        &mut self.buffers[next_buffer]
    }

    /// Grow both buffers by one element.
    pub(crate) fn grow_one(&mut self) {
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
    pub(crate) fn write_row(&mut self, buffer_idx: usize, row: usize, src: &[u8]) {
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
