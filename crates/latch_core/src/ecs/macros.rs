//! Convenience macros for entity spawning

/// Spawn an entity with components atomically
///
/// This macro ensures all components are added to the correct archetype
/// in a single operation, avoiding archetype migrations.
///
/// # Examples
///
/// ```ignore
/// // Single component
/// let entity = spawn!(world, Position { x: 0.0, y: 0.0 });
///
/// // Multiple components
/// let entity = spawn!(world, 
///     Position { x: 0.0, y: 0.0 }, 
///     Velocity { x: 1.0, y: 1.0 },
///     Health { current: 100, max: 100 }
/// );
/// ```
#[macro_export]
macro_rules! spawn {
    // Single component
    ($world:expr, $c1:expr) => {{
        let entity = $world.spawn();
        $world.add_component(entity, $c1);
        entity
    }};
    // Two components (uses optimized add_component2)
    ($world:expr, $c1:expr, $c2:expr) => {{
        let entity = $world.spawn();
        $world.add_component2(entity, $c1, $c2);
        entity
    }};
    // Three components (uses optimized add_component3)
    ($world:expr, $c1:expr, $c2:expr, $c3:expr) => {{
        let entity = $world.spawn();
        $world.add_component3(entity, $c1, $c2, $c3);
        entity
    }};
    // Four or more: fallback to sequential adds
    ($world:expr, $($component:expr),+ $(,)?) => {{
        let entity = $world.spawn();
        $(
            $world.add_component(entity, $component);
        )+
        entity
    }};
}
