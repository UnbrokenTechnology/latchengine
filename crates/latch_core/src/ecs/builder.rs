use crate::ecs::{meta_of, ArchetypeLayout, Component, ComponentId};
use std::{collections::HashMap, mem, ptr};
use thiserror::Error;

/// Owned byte payload for a single component instance.
#[derive(Debug)]
pub struct ComponentBytes {
    component_id: ComponentId,
    bytes: Box<[u8]>,
}

impl ComponentBytes {
    #[inline]
    pub fn component_id(&self) -> ComponentId {
        self.component_id
    }

    #[inline]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Fully constructed entity blueprint used during spawning.
#[derive(Debug)]
pub struct EntityBlueprint {
    layout: ArchetypeLayout,
    components: Vec<ComponentBytes>,
}

impl EntityBlueprint {
    #[inline]
    pub fn layout(&self) -> &ArchetypeLayout {
        &self.layout
    }

    #[inline]
    pub fn components(&self) -> &[ComponentBytes] {
        &self.components
    }
}

#[derive(Debug, Error)]
pub enum EntityBuilderError {
    #[error("component id {component_id} is not registered")]
    ComponentNotRegistered { component_id: ComponentId },
    #[error(
        "component id {component_id} expects stride {expected} bytes but received {actual} bytes"
    )]
    StrideMismatch {
        component_id: ComponentId,
        expected: usize,
        actual: usize,
    },
}

/// Builder for constructing entity blueprints prior to spawning.
#[derive(Default)]
pub struct EntityBuilder {
    components: HashMap<ComponentId, Box<[u8]>>,
}

impl EntityBuilder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
        }
    }

    /// Add a Rust-typed component by value.
    pub fn with<T: Component>(mut self, value: T) -> Self {
        let handle = T::handle();
        let mut bytes = vec![0u8; handle.stride];
        unsafe {
            // SAFETY: value is still alive, so copying `size_of::<T>()` bytes is valid.
            ptr::copy_nonoverlapping(
                &value as *const T as *const u8,
                bytes.as_mut_ptr(),
                mem::size_of::<T>(),
            );
        }
        mem::forget(value);
        self.components.insert(handle.id, bytes.into_boxed_slice());
        self
    }

    /// Add a component by raw bytes (scripting, serialization, etc.).
    pub fn with_raw_bytes(
        mut self,
        component_id: ComponentId,
        bytes: Vec<u8>,
    ) -> Result<Self, EntityBuilderError> {
        let meta = meta_of(component_id)
            .ok_or(EntityBuilderError::ComponentNotRegistered { component_id })?;
        if bytes.len() != meta.stride {
            return Err(EntityBuilderError::StrideMismatch {
                component_id,
                expected: meta.stride,
                actual: bytes.len(),
            });
        }
        self.components
            .insert(component_id, bytes.into_boxed_slice());
        Ok(self)
    }

    /// Finalize the builder into an `EntityBlueprint` suitable for spawning.
    pub fn build(self) -> Result<EntityBlueprint, EntityBuilderError> {
        let mut components: Vec<(ComponentId, Box<[u8]>)> = self.components.into_iter().collect();
        components.sort_by_key(|(id, _)| *id);

        for (component_id, data) in &components {
            let meta =
                meta_of(*component_id).ok_or(EntityBuilderError::ComponentNotRegistered {
                    component_id: *component_id,
                })?;
            if data.len() != meta.stride {
                return Err(EntityBuilderError::StrideMismatch {
                    component_id: *component_id,
                    expected: meta.stride,
                    actual: data.len(),
                });
            }
        }

        let layout = ArchetypeLayout::new(components.iter().map(|(id, _)| *id).collect());
        let components = components
            .into_iter()
            .map(|(component_id, bytes)| ComponentBytes {
                component_id,
                bytes,
            })
            .collect();

        Ok(EntityBlueprint { layout, components })
    }
}
