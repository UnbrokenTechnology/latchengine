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
/// // Works with any number of components
/// for_each_entity!(world, [A, B, C], |
///     (a, b, c),
///     (a_next, b_next, c_next)
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
            
            // Parallel iteration using rayon's multi-zip
            $crate::for_each_entity!(@par_multizip [$($curr),+] [$($next),+] $body);
        });
    }};
    
    // 1 component
    (@par_multizip [$c1:ident] [$n1:ident] $body:expr) => {
        $c1.par_iter().zip($n1.par_iter_mut())
            .for_each(|($c1, $n1)| {
                let ($c1) = ($c1);
                let ($n1) = ($n1);
                $body
            });
    };
    
    // 2 components
    (@par_multizip [$c1:ident, $c2:ident] [$n1:ident, $n2:ident] $body:expr) => {
        $c1.par_iter().zip($n1.par_iter_mut())
            .zip($c2.par_iter().zip($n2.par_iter_mut()))
            .for_each(|(($c1, $n1), ($c2, $n2))| {
                let ($c1, $c2) = ($c1, $c2);
                let ($n1, $n2) = ($n1, $n2);
                $body
            });
    };
    
    // 3 components
    (@par_multizip [$c1:ident, $c2:ident, $c3:ident] [$n1:ident, $n2:ident, $n3:ident] $body:expr) => {
        $c1.par_iter().zip($n1.par_iter_mut())
            .zip($c2.par_iter().zip($n2.par_iter_mut()))
            .zip($c3.par_iter().zip($n3.par_iter_mut()))
            .for_each(|((($c1, $n1), ($c2, $n2)), ($c3, $n3))| {
                let ($c1, $c2, $c3) = ($c1, $c2, $c3);
                let ($n1, $n2, $n3) = ($n1, $n2, $n3);
                $body
            });
    };
    
    // 4 components
    (@par_multizip [$c1:ident, $c2:ident, $c3:ident, $c4:ident] [$n1:ident, $n2:ident, $n3:ident, $n4:ident] $body:expr) => {
        $c1.par_iter().zip($n1.par_iter_mut())
            .zip($c2.par_iter().zip($n2.par_iter_mut()))
            .zip($c3.par_iter().zip($n3.par_iter_mut()))
            .zip($c4.par_iter().zip($n4.par_iter_mut()))
            .for_each(|(((($c1, $n1), ($c2, $n2)), ($c3, $n3)), ($c4, $n4))| {
                let ($c1, $c2, $c3, $c4) = ($c1, $c2, $c3, $c4);
                let ($n1, $n2, $n3, $n4) = ($n1, $n2, $n3, $n4);
                $body
            });
    };
    
    // 5 components
    (@par_multizip [$c1:ident, $c2:ident, $c3:ident, $c4:ident, $c5:ident] [$n1:ident, $n2:ident, $n3:ident, $n4:ident, $n5:ident] $body:expr) => {
        $c1.par_iter().zip($n1.par_iter_mut())
            .zip($c2.par_iter().zip($n2.par_iter_mut()))
            .zip($c3.par_iter().zip($n3.par_iter_mut()))
            .zip($c4.par_iter().zip($n4.par_iter_mut()))
            .zip($c5.par_iter().zip($n5.par_iter_mut()))
            .for_each(|((((($c1, $n1), ($c2, $n2)), ($c3, $n3)), ($c4, $n4)), ($c5, $n5))| {
                let ($c1, $c2, $c3, $c4, $c5) = ($c1, $c2, $c3, $c4, $c5);
                let ($n1, $n2, $n3, $n4, $n5) = ($n1, $n2, $n3, $n4, $n5);
                $body
            });
    };
}
