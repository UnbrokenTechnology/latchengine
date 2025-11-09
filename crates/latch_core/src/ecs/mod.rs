// mod.rs - ECS module exports
//
// Runtime-defined component system with TypeScript compatibility.
// See docs/ecs-runtime-components.md for architectural details.
//
// # Architecture Overview
//
// The ECS is built around these core concepts:
//
// - **Entity**: A unique identifier (ID + generation + archetype + row)
// - **Component**: POD data associated with entities (registered by ID, not Rust type)
// - **Archetype**: A unique combination of component types
// - **World**: Container for all entities and their components
// - **Storage**: SoA (Structure-of-Arrays) storage per archetype
//
// # Usage Patterns
//
// ## Spawning Entities
//
// ```ignore
// use latch_core::spawn;
//
// let entity = spawn!(world,
//     Position { x: 1.0, y: 2.0 },
//     Velocity { x: 0.5, y: 0.0 }
// );
// ```
//
// ## Single-Component Queries
//
// ```ignore
// // Iterate over all entities with Position
// for (entity, position) in world.query::<Position>() {
//     println!("Entity {:?} at {:?}", entity, position);
// }
// ```
//
// ## Multi-Component Iteration
//
// ```ignore
// use latch_core::{columns, columns_mut};
//
// // Process multiple components in parallel
// world.par_for_each(&[Position::ID, Velocity::ID], |storage| {
//     let positions = columns!(storage, Position);
//     let velocities = columns_mut!(storage, Velocity);
//     
//     velocities.par_iter_mut().zip(positions.par_iter())
//         .for_each(|(vel, pos)| {
//             vel.x += pos.x * dt;
//         });
// });
// ```
//
// ## Direct Storage Access (Advanced)
//
// ```ignore
// // Find archetypes with specific components
// let archetypes = world.archetypes_with_all(&[Position::ID, Velocity::ID]);
//
// for arch_id in archetypes {
//     if let Some(storage) = world.archetype_storage(arch_id) {
//         // Direct access to component slices
//         let positions = storage.column_as_slice::<Position>().unwrap();
//         // ... custom processing
//     }
// }
// ```

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
