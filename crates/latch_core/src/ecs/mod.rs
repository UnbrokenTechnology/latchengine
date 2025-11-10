// mod.rs - ECS module exports
//
// Runtime-defined component system with TypeScript compatibility.
// See docs/ecs-runtime-components.md for detailed usage and architecture.

mod archetype;
mod builder;
mod component;
mod entity;
mod storage;
mod world;

#[macro_use]
mod system_macros;

// Public exports
pub use archetype::{Archetype, ArchetypeId};
pub use builder::EntityBuilder;
pub use component::{meta_of, register_component, Component, ComponentId, ComponentMeta};
pub use entity::Entity;
pub use storage::ArchetypeStorage;
pub use world::World;
