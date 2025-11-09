// macros.rs - Macros for accessing component columns
//
// These macros provide a safe, ergonomic interface for accessing multiple
// component slices simultaneously from archetype storage.

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
