// component.rs
use once_cell::sync::Lazy;
use std::any::TypeId;
use std::collections::HashMap;
use std::mem::{align_of, size_of};
use std::sync::RwLock;

pub type ComponentId = u32;

/// Minimal metadata describing a POD component's layout.
#[derive(Clone, Debug)]
pub struct ComponentMeta {
    pub id: ComponentId,
    pub name: String,
    pub size: usize,
    pub align: usize,
}

/// Global registry for both Rust-defined and TS-defined components.
static REGISTRY: Lazy<RwLock<HashMap<ComponentId, ComponentMeta>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Register a component metadata (used by Rust or TS definitions).
/// Safe if IDs are unique and `size/align` match the real type when used as typed.
pub fn register_component(meta: ComponentMeta) {
    let mut map = REGISTRY.write().unwrap();
    if let Some(prev) = map.insert(meta.id, meta.clone()) {
        // Optional: sanity check re-registration matches layout
        assert_eq!(prev.size, meta.size, "Component size mismatch for id {}", meta.id);
        assert_eq!(prev.align, meta.align, "Component align mismatch for id {}", meta.id);
    }
}

/// Lookup metadata by id.
pub fn meta_of(id: ComponentId) -> Option<ComponentMeta> {
    REGISTRY.read().unwrap().get(&id).cloned()
}

/// Trait for Rust components (POD).
pub trait Component: 'static + Sized + Send + Sync {
    /// Globally unique id for this component.
    const ID: ComponentId;
    /// Human-readable name, used for debugging.
    const NAME: &'static str;

    /// Called once somewhere during startup to ensure registration.
    fn ensure_registered() {
        register_component(ComponentMeta {
            id: Self::ID,
            name: Self::NAME.to_string(),
            size: size_of::<Self>(),
            align: align_of::<Self>(),
        });
    }
}

/// Helper macro to implement `Component` and auto-register layout at use-site.
#[macro_export]
macro_rules! define_component {
    ($ty:ty, $id:expr, $name:expr) => {
        impl $crate::component::Component for $ty {
            const ID: $crate::component::ComponentId = $id;
            const NAME: &'static str = $name;
            fn ensure_registered() {
                $crate::component::register_component($crate::component::ComponentMeta{
                    id: Self::ID,
                    name: Self::NAME.to_string(),
                    size: core::mem::size_of::<Self>(),
                    align: core::mem::align_of::<Self>(),
                });
            }
        }
    };
}