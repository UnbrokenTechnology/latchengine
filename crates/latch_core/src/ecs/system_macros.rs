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
/// - Iterating over entities in parallel (if using rayon)
/// 
/// The closure receives pairs of `(&current, &mut next)` references for each component:
/// - `current`: Read from the stable state (previous tick)
/// - `next`: Write to the new state (next tick)
/// 
/// # Example
/// 
/// ```ignore
/// use latch_core::for_each_entity;
/// 
/// // Simple physics update
/// for_each_entity!(world, [Position, Velocity], |
///     (pos_curr, pos_next),
///     (vel_curr, vel_next)
/// | {
///     pos_next.x = pos_curr.x + vel_curr.x as i32;
///     pos_next.y = pos_curr.y + vel_curr.y as i32;
///     *vel_next = *vel_curr;
/// });
/// ```
/// 
/// # Parallel Iteration
/// 
/// For parallel iteration with rayon, use the `parallel` variant:
/// 
/// ```ignore
/// use rayon::prelude::*;
/// 
/// for_each_entity!(world, [Position, Velocity], |
///     (pos_curr, pos_next),
///     (vel_curr, vel_next)
/// | {
///     // This closure will be called for each entity in parallel
///     pos_next.x = pos_curr.x + vel_curr.x as i32;
///     pos_next.y = pos_curr.y + vel_curr.y as i32;
/// }, parallel);
/// ```
#[macro_export]
macro_rules! for_each_entity {
    // Sequential iteration: for_each_entity!(world, [C1, C2, ...], |args| { body })
    ($world:expr, [$($Component:ty),+ $(,)?], |$(($curr:ident, $next:ident)),+ $(,)?| $body:expr) => {{
        use $crate::ecs::Component;
        
        // Collect component IDs
        let component_ids = [$(<$Component as Component>::ID),+];
        
        // Iterate over all archetypes with these components
        $world.for_each_archetype_with_components(&component_ids, |storage| {
            // Extract all read slices at once (current buffer)
            let ($($curr),+) = $crate::columns!(storage, $($Component),+);
            
            // Extract all write slices at once (next buffer)
            let ($($next),+) = $crate::columns_mut!(storage, $($Component),+);
            
            // Create iterator that zips all read and write slices together
            let iter = $crate::for_each_entity!(@build_iter $($curr, $next),+);
            
            // Execute the user's closure for each entity
            iter.for_each(|$crate::for_each_entity!(@pattern $($curr, $next),+)| $body);
        });
    }};
    
    // Parallel iteration: for_each_entity!(world, [C1, C2, ...], |args| { body }, parallel)
    ($world:expr, [$($Component:ty),+ $(,)?], |$(($curr:ident, $next:ident)),+ $(,)?| $body:expr, parallel) => {{
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
            
            // Create parallel iterator that zips all read and write slices together
            let iter = $crate::for_each_entity!(@build_par_iter $($curr, $next),+);
            
            // Execute the user's closure for each entity in parallel
            iter.for_each(|$crate::for_each_entity!(@pattern $($curr, $next),+)| $body);
        });
    }};
    
    // Helper: Build pattern for 1 component
    (@pattern $curr1:ident, $next1:ident) => {
        ($curr1, $next1)
    };
    
    // Helper: Build pattern for 2 components
    (@pattern $curr1:ident, $next1:ident, $curr2:ident, $next2:ident) => {
        (($curr1, $next1), ($curr2, $next2))
    };
    
    // Helper: Build pattern for 3 components
    (@pattern $curr1:ident, $next1:ident, $curr2:ident, $next2:ident, $curr3:ident, $next3:ident) => {
        (($curr1, $next1), ($curr2, $next2), ($curr3, $next3))
    };
    
    // Helper: Build pattern for 4 components
    (@pattern $curr1:ident, $next1:ident, $curr2:ident, $next2:ident, $curr3:ident, $next3:ident, $curr4:ident, $next4:ident) => {
        (($curr1, $next1), ($curr2, $next2), ($curr3, $next3), ($curr4, $next4))
    };
    
    // Helper: Build sequential iterator for 1 component
    (@build_iter $curr1:ident, $next1:ident) => {
        $curr1.iter().zip($next1.iter_mut())
    };
    
    // Helper: Build sequential iterator for 2 components
    (@build_iter $curr1:ident, $next1:ident, $curr2:ident, $next2:ident) => {
        $curr1.iter().zip($next1.iter_mut())
            .zip($curr2.iter().zip($next2.iter_mut()))
            .map(|(($curr1, $next1), ($curr2, $next2))| (($curr1, $next1), ($curr2, $next2)))
    };
    
    // Helper: Build sequential iterator for 3 components
    (@build_iter $curr1:ident, $next1:ident, $curr2:ident, $next2:ident, $curr3:ident, $next3:ident) => {
        $curr1.iter().zip($next1.iter_mut())
            .zip($curr2.iter().zip($next2.iter_mut()))
            .zip($curr3.iter().zip($next3.iter_mut()))
            .map(|((($curr1, $next1), ($curr2, $next2)), ($curr3, $next3))| 
                (($curr1, $next1), ($curr2, $next2), ($curr3, $next3)))
    };
    
    // Helper: Build sequential iterator for 4 components
    (@build_iter $curr1:ident, $next1:ident, $curr2:ident, $next2:ident, $curr3:ident, $next3:ident, $curr4:ident, $next4:ident) => {
        $curr1.iter().zip($next1.iter_mut())
            .zip($curr2.iter().zip($next2.iter_mut()))
            .zip($curr3.iter().zip($next3.iter_mut()))
            .zip($curr4.iter().zip($next4.iter_mut()))
            .map(|(((($curr1, $next1), ($curr2, $next2)), ($curr3, $next3)), ($curr4, $next4))| 
                (($curr1, $next1), ($curr2, $next2), ($curr3, $next3), ($curr4, $next4)))
    };
    
    // Helper: Build parallel iterator for 1 component
    (@build_par_iter $curr1:ident, $next1:ident) => {
        $curr1.par_iter().zip($next1.par_iter_mut())
    };
    
    // Helper: Build parallel iterator for 2 components
    (@build_par_iter $curr1:ident, $next1:ident, $curr2:ident, $next2:ident) => {
        $curr1.par_iter().zip($next1.par_iter_mut())
            .zip($curr2.par_iter().zip($next2.par_iter_mut()))
            .map(|(($curr1, $next1), ($curr2, $next2))| (($curr1, $next1), ($curr2, $next2)))
    };
    
    // Helper: Build parallel iterator for 3 components
    (@build_par_iter $curr1:ident, $next1:ident, $curr2:ident, $next2:ident, $curr3:ident, $next3:ident) => {
        $curr1.par_iter().zip($next1.par_iter_mut())
            .zip($curr2.par_iter().zip($next2.par_iter_mut()))
            .zip($curr3.par_iter().zip($next3.par_iter_mut()))
            .map(|((($curr1, $next1), ($curr2, $next2)), ($curr3, $next3))| 
                (($curr1, $next1), ($curr2, $next2), ($curr3, $next3)))
    };
    
    // Helper: Build parallel iterator for 4 components
    (@build_par_iter $curr1:ident, $next1:ident, $curr2:ident, $next2:ident, $curr3:ident, $next3:ident, $curr4:ident, $next4:ident) => {
        $curr1.par_iter().zip($next1.par_iter_mut())
            .zip($curr2.par_iter().zip($next2.par_iter_mut()))
            .zip($curr3.par_iter().zip($next3.par_iter_mut()))
            .zip($curr4.par_iter().zip($next4.par_iter_mut()))
            .map(|(((($curr1, $next1), ($curr2, $next2)), ($curr3, $next3)), ($curr4, $next4))| 
                (($curr1, $next1), ($curr2, $next2), ($curr3, $next3), ($curr4, $next4)))
    };
}
