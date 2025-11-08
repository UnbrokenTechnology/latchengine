// component.rs - Runtime component registration
//
// Components are identified by u32 IDs, not Rust TypeIds.
// This enables TypeScript-defined components to coexist with Rust components.

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::mem::{align_of, size_of};
use std::sync::RwLock;

pub type ComponentId = u32;

/// Metadata describing a component's memory layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentMeta {
    pub id: ComponentId,
    pub name: String,
    pub size: usize,
    pub align: usize,
}

/// Global registry for both Rust-defined and TypeScript-defined components.
static REGISTRY: Lazy<RwLock<HashMap<ComponentId, ComponentMeta>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Register a component's metadata.
/// 
/// # Safety
/// Callers must ensure that `size` and `align` accurately describe the component's layout
/// when used with typed access methods.
pub fn register_component(meta: ComponentMeta) {
    let mut map = REGISTRY.write().unwrap();
    if let Some(prev) = map.insert(meta.id, meta.clone()) {
        // Sanity check: re-registration must match previous layout
        assert_eq!(
            prev.size, meta.size,
            "Component size mismatch for id {}: was {}, now {}",
            meta.id, prev.size, meta.size
        );
        assert_eq!(
            prev.align, meta.align,
            "Component align mismatch for id {}: was {}, now {}",
            meta.id, prev.align, meta.align
        );
    }
}

/// Look up component metadata by ID.
pub fn meta_of(id: ComponentId) -> Option<ComponentMeta> {
    REGISTRY.read().unwrap().get(&id).cloned()
}

/// Trait for Rust-defined POD components.
/// 
/// # Safety
/// Implementors must:
/// - Be POD (Plain Old Data): no Drop, no internal references
/// - Have stable layout (no padding variations)
/// - Be Send + Sync for parallel iteration
pub trait Component: 'static + Sized + Send + Sync {
    /// Globally unique component ID.
    const ID: ComponentId;
    
    /// Human-readable name for debugging.
    const NAME: &'static str;

    /// Register this component's layout with the global registry.
    /// Should be called once during startup.
    fn ensure_registered() {
        register_component(ComponentMeta {
            id: Self::ID,
            name: Self::NAME.to_string(),
            size: size_of::<Self>(),
            align: align_of::<Self>(),
        });
    }
}

/// Helper macro to implement Component trait.
/// 
/// # Example
/// ```ignore
/// #[derive(Clone, Copy)]
/// struct Position { x: f32, y: f32 }
/// 
/// define_component!(Position, 1, "Position");
/// ```
#[macro_export]
macro_rules! define_component {
    ($ty:ty, $id:expr, $name:expr) => {
        impl $crate::ecs::Component for $ty {
            const ID: $crate::ecs::ComponentId = $id;
            const NAME: &'static str = $name;
        }
    };
}
