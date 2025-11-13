// component.rs
use once_cell::sync::{Lazy, OnceCell};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::RwLock,
};

pub type ComponentId = u32;

/// Runtime metadata for a component layout (works for Rust or external/scripted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentMeta {
    pub id: ComponentId,
    pub name: String,
    pub size: usize,      // bytes of the logical element
    pub align: usize,     // required alignment (power-of-two)
    pub stride: usize,    // bytes between consecutive elements (>= size, multiple of align)
    pub pod: bool,        // Plain-Old-Data (no Drop) hint for fast paths
}

/// Lightweight, copyable handle you pass around at runtime.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ComponentHandle {
    pub id: ComponentId,
    pub size: usize,
    pub align: usize,
    pub stride: usize,
}

impl ComponentHandle {
    #[inline] pub fn bytes_per_elem(&self) -> usize { self.stride }
}

/// Global registry: name<->id maps + id->meta + next_id counter.
struct Registry {
    by_id: HashMap<ComponentId, ComponentMeta>,
    by_name: HashMap<String, ComponentId>,
    next_id: ComponentId,
}

static REG: Lazy<RwLock<Registry>> = Lazy::new(|| {
    RwLock::new(Registry { by_id: HashMap::new(), by_name: HashMap::new(), next_id: 1 })
});

/// Internal: insert-or-verify by name, returning (id, meta).
fn ensure_component_inner(name: &str, size: usize, align: usize, stride: usize, pod: bool) -> ComponentMeta {
    assert!(align.is_power_of_two(), "align must be a power of two");
    assert!(stride >= size && stride % align == 0, "stride must be >= size and a multiple of align");

    let mut reg = REG.write().unwrap();

    if let Some(&id) = reg.by_name.get(name) {
        let m = reg.by_id.get(&id).unwrap();
        // Re-registration must match prior layout
        assert_eq!(m.size,   size,   "component '{}' size changed ({} -> {})",   name, m.size, size);
        assert_eq!(m.align,  align,  "component '{}' align changed ({} -> {})",  name, m.align, align);
        assert_eq!(m.stride, stride, "component '{}' stride changed ({} -> {})", name, m.stride, stride);
        assert_eq!(m.pod,    pod,    "component '{}' POD flag changed",          name);
        return m.clone();
    }

    let id = {
        let id = reg.next_id;
        reg.next_id = id.checked_add(1).expect("component id overflow");
        id
    };

    let meta = ComponentMeta {
        id,
        name: name.to_string(),
        size,
        align,
        stride,
        pod,
    };
    reg.by_name.insert(meta.name.clone(), id);
    reg.by_id.insert(id, meta.clone());
    meta
}

/// Register or fetch a Rust-defined POD component by name/layout.
/// Returns a handle you can store or pass to systems.
pub fn ensure_component_by_layout(name: &str, size: usize, align: usize, stride: usize, pod: bool) -> ComponentHandle {
    let m = ensure_component_inner(name, size, align, stride, pod);
    ComponentHandle { id: m.id, size: m.size, align: m.align, stride: m.stride }
}

/// External/script registration (e.g., from Lua/JS/C#/C++).
pub fn register_external_component(name: &str, size: usize, align: usize, stride: usize) -> ComponentHandle {
    ensure_component_by_layout(name, size, align, stride, true /* or false if you support Drop via callbacks */)
}

/// Lookup by id or name.
pub fn meta_of_id(id: ComponentId) -> Option<ComponentMeta> {
    REG.read().unwrap().by_id.get(&id).cloned()
}
pub fn meta_of_name(name: &str) -> Option<ComponentMeta> {
    REG.read().unwrap().by_name.get(name).and_then(|id| REG.read().unwrap().by_id.get(id).cloned())
}

/// Fast path: get a handle by name (must already be registered).
pub fn handle_of_name(name: &str) -> ComponentHandle {
    let m = meta_of_name(name).unwrap_or_else(|| panic!("component '{}' not registered", name));
    ComponentHandle { id: m.id, size: m.size, align: m.align, stride: m.stride }
}

/// Trait for Rust-defined components (no manual ID typing).
/// ID is assigned on first use and cached in a per-type OnceCell.
pub trait Component: 'static + Sized + Send + Sync {
    const NAME: &'static str;
    /// Is this POD (no Drop, relocatable)? Defaults to true.
    fn is_pod() -> bool { true }

    /// Per-type cached handle (assigned at first call).
    fn handle() -> ComponentHandle where Self: Sized {
        static HANDLE: OnceCell<ComponentHandle> = OnceCell::new();
        *HANDLE.get_or_init(|| {
            let size = std::mem::size_of::<Self>();
            let align = std::mem::align_of::<Self>();
            let stride = size.next_multiple_of(align); // usually == size
            ensure_component_by_layout(Self::NAME, size, align, stride, Self::is_pod())
        })
    }

    /// Convenience: component id.
    #[inline] fn id() -> ComponentId { Self::handle().id }
}

/// Helper macro: implement `Component` for a Rust struct with a given name.
/// Usage:
///   #[derive(Clone, Copy)] struct Position { x: f32, y: f32 }
///   define_component!(Position, "Position");
#[macro_export]
macro_rules! define_component {
    ($ty:ty, $name:expr) => {
        impl $crate::component::Component for $ty {
            const NAME: &'static str = $name;
            // If your type has Drop or interior refs, override `fn is_pod() -> bool { false }`.
        }
    };
}

/// Macro to fetch a runtime handle by name (for dynamic systems).
/// Usage: let pos = component!("Position");
#[macro_export]
macro_rules! component {
    ($name:expr) => {
        $crate::component::handle_of_name($name)
    };
}