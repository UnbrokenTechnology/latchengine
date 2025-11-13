//! Entity Component System core types.
//!
//! This module is being rewritten to follow the updated ECS design
//! documented in `.github/instructions/ecs.instructions.md`. The goal
//! is to provide cache-efficient, deterministic storage with rich
//! runtime metadata for both Rust and scripted components. The current
//! contents focus on foundational building blocks (components,
//! entities, archetype layout). Higher-level systems such as storage
//! and world management will be reintroduced in subsequent iterations.

mod archetype;
mod component;
mod entity;

pub use archetype::{ArchetypeId, ArchetypeLayout};
pub use component::{
	handle_of_name,
	meta_of,
	meta_of_name,
	register_component,
	register_external_component_with_fields,
	Component,
	ComponentHandle,
	ComponentId,
	ComponentMeta,
	FieldMeta,
	__ComponentOnceCell,
};
pub use entity::{Entity, EntityId, EntityLoc, Generation};
