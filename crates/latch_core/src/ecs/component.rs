//! Component metadata and registration infrastructure.
//!
//! This module underpins the dynamic ECS component model. It exposes
//! a global registry keyed by stable `ComponentId`s so that runtime
//! systems (Rust, scripting, tooling) can consistently reason about
//! component layouts.

use once_cell::sync::OnceCell;

pub use once_cell::sync::OnceCell as __ComponentOnceCell;
use std::{collections::HashMap, fmt, sync::RwLock};

/// Unique identifier assigned to each registered component.
pub type ComponentId = u32;

/// Metadata for a single field inside a component layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldMeta {
    pub name: Box<str>,
    pub offset: usize,
    pub size: usize,
}

impl FieldMeta {
    #[inline]
    pub fn new(name: impl Into<Box<str>>, offset: usize, size: usize) -> Self {
        Self {
            name: name.into(),
            offset,
            size,
        }
    }
}

/// Full runtime metadata for a component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentMeta {
    pub id: ComponentId,
    pub name: Box<str>,
    pub size: usize,
    pub align: usize,
    pub stride: usize,
    pub pod: bool,
    pub fields: Box<[FieldMeta]>,
}

impl ComponentMeta {
    #[inline]
    fn handle(&self) -> ComponentHandle {
        ComponentHandle {
            id: self.id,
            size: self.size,
            align: self.align,
            stride: self.stride,
            pod: self.pod,
        }
    }
}

/// Lightweight handle cached by systems once registration succeeds.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ComponentHandle {
    pub id: ComponentId,
    pub size: usize,
    pub align: usize,
    pub stride: usize,
    pub pod: bool,
}

impl ComponentHandle {
    #[inline]
    pub fn bytes_per_element(&self) -> usize {
        self.stride
    }
}

#[derive(Default)]
struct Registry {
    by_id: HashMap<ComponentId, ComponentMeta>,
    by_name: HashMap<Box<str>, ComponentId>,
    next_id: ComponentId,
}

static REGISTRY: OnceCell<RwLock<Registry>> = OnceCell::new();

fn registry_mut() -> std::sync::RwLockWriteGuard<'static, Registry> {
    REGISTRY
        .get_or_init(|| RwLock::new(Registry::default()))
        .write()
        .expect("component registry poisoned")
}

fn validate_layout(
    meta: &ComponentMeta,
    size: usize,
    align: usize,
    stride: usize,
    pod: bool,
    fields: &[FieldMeta],
) {
    if meta.size != size
        || meta.align != align
        || meta.stride != stride
        || meta.pod != pod
        || meta.fields.as_ref() != fields
    {
        panic!(
            "component '{}' registered with conflicting layout",
            meta.name
        );
    }
}

fn register_internal(
    name: &str,
    size: usize,
    align: usize,
    stride: usize,
    pod: bool,
    fields: Vec<FieldMeta>,
) -> ComponentHandle {
    assert!(
        align.is_power_of_two(),
        "component alignment must be power-of-two"
    );
    assert!(stride >= size, "stride must be >= size");
    assert!(
        stride % align == 0,
        "stride must be a multiple of alignment"
    );

    let mut reg = registry_mut();
    if let Some(&id) = reg.by_name.get(name) {
        let existing = reg
            .by_id
            .get(&id)
            .expect("registry missing component metadata");
        validate_layout(existing, size, align, stride, pod, &fields);
        return existing.handle();
    }

    let id = reg.next_id;
    reg.next_id = reg.next_id.checked_add(1).expect("component id overflow");

    let meta = ComponentMeta {
        id,
        name: name.into(),
        size,
        align,
        stride,
        pod,
        fields: fields.into_boxed_slice(),
    };

    reg.by_name.insert(meta.name.clone(), meta.id);
    reg.by_id.insert(meta.id, meta.clone());
    meta.handle()
}

/// Register a Rust-side component layout.
pub fn register_component(
    name: &str,
    size: usize,
    align: usize,
    stride: usize,
    pod: bool,
    fields: Vec<FieldMeta>,
) -> ComponentHandle {
    register_internal(name, size, align, stride, pod, fields)
}

/// Register an externally-described component (e.g. scripting, tooling).
pub fn register_external_component_with_fields(
    name: &str,
    size: usize,
    align: usize,
    stride: usize,
    fields: Vec<FieldMeta>,
    pod: bool,
) -> ComponentHandle {
    register_internal(name, size, align, stride, pod, fields)
}

/// Retrieve metadata by id.
pub fn meta_of(id: ComponentId) -> Option<ComponentMeta> {
    REGISTRY
        .get()
        .and_then(|lock| lock.read().ok())
        .and_then(|reg| reg.by_id.get(&id).cloned())
}

/// Retrieve metadata by name.
pub fn meta_of_name(name: &str) -> Option<ComponentMeta> {
    REGISTRY
        .get()
        .and_then(|lock| lock.read().ok())
        .and_then(|reg| {
            reg.by_name
                .get(name)
                .and_then(|id| reg.by_id.get(id))
                .cloned()
        })
}

/// Resolve a handle by name, panicking if not registered.
pub fn handle_of_name(name: &str) -> ComponentHandle {
    meta_of_name(name)
        .unwrap_or_else(|| panic!("component '{}' not registered", name))
        .handle()
}

/// Trait implemented by Rust-native component types.
pub trait Component: 'static + Send + Sync {
    const NAME: &'static str;

    /// Override to mark components that drop resources.
    fn is_pod() -> bool {
        true
    }

    /// Provide compile-time field metadata (optional).
    fn fields() -> Vec<FieldMeta> {
        Vec::new()
    }

    /// Register the component layout and return its handle.
    fn register_layout() -> ComponentHandle
    where
        Self: Sized,
    {
        let size = std::mem::size_of::<Self>();
        let align = std::mem::align_of::<Self>();
        let stride = size.next_multiple_of(align);
        register_component(
            Self::NAME,
            size,
            align,
            stride,
            Self::is_pod(),
            Self::fields(),
        )
    }

    /// Lazy component handle registration.
    fn handle() -> ComponentHandle
    where
        Self: Sized,
    {
        static HANDLE: OnceCell<ComponentHandle> = OnceCell::new();
        *HANDLE.get_or_init(Self::register_layout)
    }

    #[inline]
    fn id() -> ComponentId
    where
        Self: Sized,
    {
        Self::handle().id
    }
}

/// Helper macro for trivial POD components.
#[macro_export]
macro_rules! define_component {
    ($ty:ty, $name:expr) => {
        impl $crate::ecs::Component for $ty {
            const NAME: &'static str = $name;
        }
    };

    ($ty:ty, $id:expr, $name:expr) => {
        impl $crate::ecs::Component for $ty {
            const NAME: &'static str = $name;

            fn handle() -> $crate::ecs::ComponentHandle {
                static HANDLE: $crate::ecs::component::__ComponentOnceCell<
                    $crate::ecs::ComponentHandle,
                > = $crate::ecs::component::__ComponentOnceCell::new();
                *HANDLE.get_or_init(|| {
                    let size = std::mem::size_of::<$ty>();
                    let align = std::mem::align_of::<$ty>();
                    let stride = size.next_multiple_of(align);
                    let handle = $crate::ecs::register_component(
                        $name,
                        size,
                        align,
                        stride,
                        <$ty as $crate::ecs::Component>::is_pod(),
                        <$ty as $crate::ecs::Component>::fields(),
                    );
                    debug_assert_eq!(
                        handle.id, $id,
                        "component '{}' registered with id {} but expected {}",
                        $name, handle.id, $id,
                    );
                    handle
                })
            }
        }
    };
}

impl fmt::Display for ComponentMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ComponentMeta {{ id: {}, name: {}, size: {}, align: {}, stride: {}, pod: {} }}",
            self.id, self.name, self.size, self.align, self.stride, self.pod
        )
    }
}
