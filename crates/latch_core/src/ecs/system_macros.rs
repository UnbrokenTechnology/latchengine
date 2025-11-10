// system_macros.rs - Ergonomic macros for ECS system definition
//
// These macros provide a clean, user-friendly API for defining systems
// that iterate over entities with specific components, hiding the
// complexity of archetype iteration and double-buffering.

/// Execute a closure for each entity with the specified components.
/// 
/// This macro provides a clean API for iterating over entities without
/// exposing archetype implementation details. It automatically handles:
/// - Finding all archetypes with the required components
/// - Extracting component slices using double-buffering
/// - Iterating over entities in parallel using rayon
/// 
/// The closure receives two tuples:
/// - First tuple: References to current components `(&C1, &C2, ...)`
/// - Second tuple: Mutable references to next components `(&mut C1, &mut C2, ...)`
/// 
/// This structure allows for efficient parallel iteration while maintaining
/// the double-buffering pattern and supports arbitrary numbers of components.
/// 
/// # Example
/// 
/// ```ignore
/// use latch_core::for_each_entity;
/// 
/// // Simple physics update with 2 components
/// for_each_entity!(world, [Position, Velocity], |
///     (pos_curr, vel_curr),
///     (pos_next, vel_next)
/// | {
///     pos_next.x = pos_curr.x + vel_curr.x as i32;
///     pos_next.y = pos_curr.y + vel_curr.y as i32;
///     *vel_next = *vel_curr;
/// });
/// 
/// // Works with any number of components (1, 2, 3, 7, 10, ...)
/// for_each_entity!(world, [A, B, C, D, E, F, G], |
///     (a, b, c, d, e, f, g),
///     (a_next, b_next, c_next, d_next, e_next, f_next, g_next)
/// | {
///     // Update logic
/// });
/// ```
#[macro_export]
macro_rules! for_each_entity {
    ($world:expr, [$($Component:ty),+ $(,)?], |($($curr:ident),+ $(,)?), ($($next:ident),+ $(,)?)| $body:expr) => {{
        use $crate::ecs::Component;
        use rayon::prelude::*;
        
        // Collect component IDs
        let component_ids = [$(<$Component as Component>::ID),+];
        
        // Iterate over all archetypes with these components
        $world.for_each_archetype_with_components(&component_ids, |storage| {
            // Extract all read slices at once (current buffer)
            let ($ ($curr),+) = $crate::columns!(storage, $($Component),+);
            
            // Extract all write slices at once (next buffer)
            let ($(mut $next),+) = $crate::columns_mut!(storage, $($Component),+);
            
            // Verify all slices have the same length
            let len = {
                let curr_lens = [ $( $curr.len() ),+ ];
                let next_lens = [ $( $next.len() ),+ ];
                let len = curr_lens[0];
                assert!(curr_lens.iter().all(|&x| x == len), "curr columns differ in length");
                assert!(next_lens.iter().all(|&x| x == len), "next columns differ in length");
                len
            };
            
            // Get raw pointers to avoid borrowing issues in the parallel closure
            $(let $curr = $curr.as_ptr();)+
            $(let $next = $next.as_mut_ptr();)+
            
            // Parallel iteration using index-based approach
            // This avoids the nested tuple problem when zipping many iterators
            (0..len).into_par_iter().for_each(move |i| {
                // SAFETY: We've verified all slices have the same length `len`
                // and rayon ensures different threads access different indices.
                // The slices are obtained from different component columns so there's no aliasing.
                unsafe {
                    let ($($curr),+) = ( $( &*$curr.add(i) ),+ );
                    let ($($next),+) = ( $( &mut *$next.add(i) ),+ );
                    $body
                }
            });
        });
    }};
}
