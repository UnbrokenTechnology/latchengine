//! Latch Asset Pipeline
//!
//! Asset loading, conversion, and management

/// Asset handle (opaque ID)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct AssetHandle(u64);

/// Asset registry (placeholder)
pub struct AssetRegistry {
    next_id: u64,
}

impl AssetRegistry {
    pub fn new() -> Self {
        Self { next_id: 1 }
    }

    pub fn register(&mut self) -> AssetHandle {
        let handle = AssetHandle(self.next_id);
        self.next_id += 1;
        handle
    }
}

impl Default for AssetRegistry {
    fn default() -> Self {
        Self::new()
    }
}
