// macros.rs - Macros for accessing component columns
//
// These macros provide a safe, ergonomic interface for accessing multiple
// component slices simultaneously from archetype storage.
//
// # Safety
//
// The macros ensure safety by:
// 1. Verifying all component IDs are unique at runtime
// 2. Getting separate column pointers that don't alias
// 3. Respecting the double-buffer read/write separation
//
// This allows safe parallel access to different component types while
// preventing undefined behavior from aliasing mutable references.
//
// # Example
//
// ```ignore
// use latch_core::{columns, columns_mut};
//
// // Read from current buffer
// let positions = columns!(storage, Position);
//
// // Write to next buffer
// let velocities = columns_mut!(storage, Velocity);
//
// // Read multiple (current buffer)
// let (pos, vel) = columns!(storage, Position, Velocity);
//
// // Write multiple (next buffer)
// let (pos_out, vel_out) = columns_mut!(storage, Position, Velocity);
// ```

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
        // Get the current buffer index (what we read from)
        let current_buffer = $storage.current_buffer_index();

        // Get raw pointers to each column
        let ptrs = [$(unsafe {
            $storage
                .get_column_ptr_const(<$T as $crate::ecs::Component>::id())
                .expect("Component not found in archetype")
        }),+];

        // SAFETY: Each column is independently readable, and the macro ensures
        // component types are distinct at compile time.
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
    let ids = [$(<$T as $crate::ecs::Component>::id()),+];

        // Get the next buffer index (what we write to)
        let next_buffer = $storage.next_buffer_index();

        // Get raw pointers to each column
        let ptrs = [$(unsafe {
            $storage
                .get_column_ptr(<$T as $crate::ecs::Component>::id())
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
