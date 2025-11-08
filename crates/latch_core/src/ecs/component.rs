//! Component trait and storage

use std::any::Any;

/// Component ID - unique identifier for a component type
///
/// This can be:
/// - Rust types: Hash of type name
/// - TypeScript: Hash of class name  
/// - Manual: Any chosen u64
pub type ComponentId = u64;

/// Base trait for all components
///
/// Requirements:
/// - Must be 'static (no lifetimes)
/// - Must be Send + Sync (for parallel systems)
///
/// Components should be POD (Plain Old Data):
/// - Public fields
/// - No methods (logic goes in systems)
/// - Deterministic memory layout
pub trait Component: 'static + Send + Sync {
    /// Get the unique ID for this component type
    fn id() -> ComponentId where Self: Sized;
}

/// Helper macro to implement Component with a derived ID
#[macro_export]
macro_rules! component {
    ($type:ty) => {
        impl $crate::ecs::Component for $type {
            fn id() -> $crate::ecs::ComponentId {
                // Simple FNV-1a hash of type name
                const fn hash_type_name() -> u64 {
                    let bytes = ::core::any::type_name::<$type>().as_bytes();
                    let mut hash: u64 = 0xcbf29ce484222325;
                    let mut i = 0;
                    while i < bytes.len() {
                        hash ^= bytes[i] as u64;
                        hash = hash.wrapping_mul(0x100000001b3);
                        i += 1;
                    }
                    hash
                }
                hash_type_name()
            }
        }
    };
}

/// Internal trait for type-erased component storage
#[allow(dead_code)]
pub(crate) trait ComponentStorage: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn len(&self) -> usize;
    fn swap_remove(&mut self, index: usize);
    fn clone_box(&self) -> Box<dyn ComponentStorage>;
}

/// Type-specific component storage
pub(crate) struct ComponentVec<T: Component> {
    pub data: Vec<T>,
}

impl<T: Component> ComponentVec<T> {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn push(&mut self, value: T) {
        self.data.push(value);
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.data.get_mut(index)
    }
}

impl<T: Component + Clone> ComponentStorage for ComponentVec<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn swap_remove(&mut self, index: usize) {
        self.data.swap_remove(index);
    }

    fn clone_box(&self) -> Box<dyn ComponentStorage> {
        Box::new(Self {
            data: self.data.clone(),
        })
    }
}
