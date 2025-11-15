use crate::ecs::{ComponentId, SystemDescriptor, SystemHandle, SystemRegistrationError};
use std::collections::HashMap;

pub(crate) struct SystemRegistry {
    systems: Vec<RegisteredSystem>,
    name_lookup: HashMap<String, SystemHandle>,
    component_writers: HashMap<ComponentId, SystemHandle>,
}

impl SystemRegistry {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
            name_lookup: HashMap::new(),
            component_writers: HashMap::new(),
        }
    }

    pub fn register(
        &mut self,
        descriptor: SystemDescriptor,
    ) -> Result<SystemHandle, SystemRegistrationError> {
        if descriptor.is_empty() {
            return Err(SystemRegistrationError::EmptyAccess {
                name: descriptor.name().to_string(),
            });
        }

        let name_key = descriptor.name().to_string();
        if self.name_lookup.contains_key(&name_key) {
            return Err(SystemRegistrationError::DuplicateName { name: name_key });
        }

        let writes = descriptor.write_components().to_vec();
        for component in &writes {
            if let Some(existing_handle) = self.component_writers.get(component) {
                let existing_name = self
                    .systems
                    .get(existing_handle.index() as usize)
                    .map(|sys| sys.descriptor.name().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                return Err(SystemRegistrationError::ComponentWriteConflict {
                    component: *component,
                    existing: existing_name,
                    requested: descriptor.name().to_string(),
                    existing_handle: *existing_handle,
                });
            }
        }

        let handle = SystemHandle::new(self.systems.len() as u32);
        let components = descriptor.all_components().to_vec();
        for component in writes {
            self.component_writers.insert(component, handle);
        }

        self.name_lookup.insert(name_key, handle);
        self.systems.push(RegisteredSystem {
            handle,
            descriptor,
            components,
        });

        Ok(handle)
    }

    pub fn descriptor(&self, handle: SystemHandle) -> Option<&SystemDescriptor> {
        self.systems
            .get(handle.index() as usize)
            .map(|system| &system.descriptor)
    }

    pub fn component_filter(&self, handle: SystemHandle) -> Option<&[ComponentId]> {
        self.systems
            .get(handle.index() as usize)
            .map(|system| system.components.as_slice())
    }

    pub fn read_components(&self, handle: SystemHandle) -> Option<&[ComponentId]> {
        self.descriptor(handle)
            .map(|descriptor| descriptor.read_components())
    }

    pub fn write_components(&self, handle: SystemHandle) -> Option<&[ComponentId]> {
        self.descriptor(handle)
            .map(|descriptor| descriptor.write_components())
    }

    pub fn iter(&self) -> impl Iterator<Item = (SystemHandle, &SystemDescriptor)> {
        self.systems
            .iter()
            .map(|system| (system.handle, &system.descriptor))
    }
}

struct RegisteredSystem {
    handle: SystemHandle,
    descriptor: SystemDescriptor,
    components: Vec<ComponentId>,
}
