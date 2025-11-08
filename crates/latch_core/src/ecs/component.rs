//! Component trait and storage

use std::any::Any;

/// Marker trait for component types
///
/// Requirements:
/// - Must be 'static (no lifetimes)
/// - Must be Send + Sync (for parallel systems)
///
/// Components should be POD (Plain Old Data):
/// - Public fields
/// - No methods (logic goes in systems)
/// - Deterministic memory layout
///
/// # Example
///
/// ```ignore
/// #[derive(Clone, Copy)]
/// struct Position {
///     x: f32,
///     y: f32,
/// }
/// // Automatically implements Component via blanket impl
/// ```
pub trait Component: 'static + Send + Sync {
    /// Component type name (for debugging)
    fn type_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

// Blanket implementation for all valid types
impl<T: 'static + Send + Sync> Component for T {}

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
