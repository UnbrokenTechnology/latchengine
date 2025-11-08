// mod.rs - ECS module exports
//
// Runtime-defined component system with TypeScript compatibility.
// See docs/ecs-runtime-components.md for architectural details.

mod archetype;
mod builder;
mod component;
mod entity;
mod storage;
mod world;

// Public exports
pub use archetype::{Archetype, ArchetypeId};
pub use builder::EntityBuilder;
pub use component::{meta_of, register_component, Component, ComponentId, ComponentMeta};
pub use entity::Entity;
pub use storage::ArchetypeStorage;
pub use world::World;

/// Convenience macro for spawning entities.
/// 
/// # Example
/// ```ignore
/// let entity = spawn!(world,
///     Position { x: 1.0, y: 2.0 },
///     Velocity { x: 0.5, y: 0.0 }
/// );
/// ```
#[macro_export]
macro_rules! spawn {
    ($world:expr, $($comp:expr),+ $(,)?) => {{
        let builder = $crate::ecs::EntityBuilder::new()
            $(.with($comp))+;
        $world.spawn(builder)
    }};
}
