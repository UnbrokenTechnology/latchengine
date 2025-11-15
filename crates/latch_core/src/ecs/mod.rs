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
mod builder;
mod component;
mod entity;
mod system_descriptor;
mod system_handle;
mod system_registration_error;
mod system_registry;
pub mod storage;
mod world;

pub use archetype::{ArchetypeId, ArchetypeLayout};
pub use builder::{ComponentBytes, EntityBlueprint, EntityBuilder, EntityBuilderError};
pub use component::{
    handle_of_name, meta_of, meta_of_name, register_component, register_component_with_id,
    register_external_component_with_fields, Component, ComponentHandle, ComponentId,
    ComponentMeta, FieldMeta, __ComponentOnceCell,
};
pub use entity::{Entity, EntityId, EntityLoc, Generation};
pub use system_descriptor::SystemDescriptor;
pub use system_handle::SystemHandle;
pub use system_registration_error::SystemRegistrationError;
pub(crate) use system_registry::SystemRegistry;
pub use storage::{
    plan_archetype, ArchetypePlan, ArchetypeStorage, ColumnError, PageBudget, PlanError,
    StorageError,
};
pub use world::{World, WorldError};

/// Spawn an entity into the world using builder-style component construction.
#[macro_export]
macro_rules! spawn {
    ($world:expr $(, $component:expr)+ $(,)?) => {{
        let builder = {
            let mut builder = $crate::ecs::EntityBuilder::new();
            $(
                builder = builder.with($component);
            )+
            builder
        };
        $world
            .spawn(builder)
            .expect("failed to spawn entity")
    }};
}
