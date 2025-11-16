use crate::ecs::{ComponentId, SystemHandle};
use thiserror::Error;

/// Errors that can occur while registering a system with the world.
#[derive(Debug, Error)]
pub enum SystemRegistrationError {
    #[error("system '{name}' is already registered")]
    DuplicateName { name: String },

    #[error("system '{name}' does not access any components")]
    EmptyAccess { name: String },

    #[error("component {component} already has a writer registered by system '{existing}'")]
    ComponentWriteConflict {
        component: ComponentId,
        existing: String,
        requested: String,
        existing_handle: SystemHandle,
    },
}
