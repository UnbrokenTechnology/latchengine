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
/// // Works with any number of components (1, 2, 3, 4, 5, ...)
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
            let ($($curr),+) = $crate::columns!(storage, $($Component),+);
            
            // Extract all write slices at once (next buffer)
            let ($($next),+) = $crate::columns_mut!(storage, $($Component),+);
            
            // Zip curr and next for each component in parallel
            #[allow(non_snake_case)]
            {
                $(
                    let $curr = $curr.par_iter();
                    let $next = $next.par_iter_mut();
                )+
                
                // Use multizip from itertools pattern manually
                // Zip the first two, then zip with the rest iteratively
                $crate::for_each_entity!(@zip_components [$($curr, $next),+] $body);
            }
        });
    }};
    
    // Base case: single component
    (@zip_components [$curr:ident, $next:ident] $body:expr) => {
        $curr.zip($next).for_each(|($curr, $next)| $body);
    };
    
    // Recursive case: multiple components
    (@zip_components [$curr1:ident, $next1:ident, $($curr_rest:ident, $next_rest:ident),+] $body:expr) => {
        $curr1.zip($next1).zip(
            $crate::for_each_entity!(@build_rest_zip $($curr_rest, $next_rest),+)
        ).for_each(|(($curr1, $next1), rest)| {
            $crate::for_each_entity!(@unpack_rest rest [$($curr_rest, $next_rest),+] $body);
        });
    };
    
    // Build iterator for the rest of the components
    (@build_rest_zip $curr:ident, $next:ident) => {
        $curr.zip($next)
    };
    
    (@build_rest_zip $curr1:ident, $next1:ident, $($curr_rest:ident, $next_rest:ident),+) => {
        $curr1.zip($next1).zip(
            $crate::for_each_entity!(@build_rest_zip $($curr_rest, $next_rest),+)
        )
    };
    
    // Unpack nested tuple structure
    (@unpack_rest $rest:ident [$curr:ident, $next:ident] $body:expr) => {
        let ($curr, $next) = $rest;
        $body
    };
    
    (@unpack_rest $rest:ident [$curr1:ident, $next1:ident, $($curr_rest:ident, $next_rest:ident),+] $body:expr) => {
        let (($curr1, $next1), rest) = $rest;
        $crate::for_each_entity!(@unpack_rest rest [$($curr_rest, $next_rest),+] $body);
    };
}
