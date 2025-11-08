//! Entity Component System
//!
//! Modular ECS implementation organized into focused submodules.

mod entity;
mod component;
mod archetype;
mod world;
mod macros;

pub use entity::Entity;
pub use component::Component;
pub use world::{World, EntityBuilder};
