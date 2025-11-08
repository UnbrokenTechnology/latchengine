//! Convenience macros for entity spawning

/// Spawn an entity with components atomically
///
/// Each component provides its own ID via component_id(), allowing
/// components defined in TypeScript or other languages to work seamlessly.
///
/// # Examples
///
/// ```ignore
/// let entity = spawn!(world, Position { x: 0.0, y: 0.0 });
/// let entity = spawn!(world, 
///     Position { x: 0.0, y: 0.0 },
///     Velocity { x: 1.0, y: 1.0 }
/// );
/// ```
#[macro_export]
macro_rules! spawn {
    ($world:expr, $($component:expr),+ $(,)?) => {{
        // Collect component IDs  
        let mut component_ids = ::std::vec::Vec::new();
        $(
            let comp = $component;
            component_ids.push($crate::ecs::Component::component_id(&comp));
        )+
        
        // Create entity with archetype calculated from IDs
        let entity = $world.create_entity_for_archetype(&component_ids);
        
        // Insert each component into archetype storage
        $(
            let comp = $component;
            $world.insert_component_for_entity(entity, comp);
        )+
        
        entity
    }};
}
