//! Component trait and blanket implementation
//!
//! Components are POD (Plain Old Data) types attached to entities.

/// Marker trait for component types
///
/// Requirements:
/// - Must be 'static (no borrowed references)
/// - Must be Send + Sync (for parallel systems)
///
/// Components should be POD (Plain Old Data):
/// - Public fields
/// - No methods (logic goes in systems)
/// - Deterministic memory layout
///
/// Note: `'static` means "no references", NOT "lives forever"!
/// Components are deallocated when their entity is destroyed.
///
/// Example:
/// ```ignore
/// #[derive(Clone)]
/// struct Position { x: f32, y: f32 }
/// // Automatically implements Component!
/// ```
pub trait Component: 'static + Send + Sync {
    /// Component type name (for debugging)
    fn type_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

// Blanket implementation for all valid types
impl<T: 'static + Send + Sync> Component for T {}
