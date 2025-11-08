//! Save system abstraction

/// Save slot
pub struct SaveSlot {
    pub id: u32,
}

/// Save system (placeholder)
pub struct SaveSystem {
    _placeholder: (),
}

impl SaveSystem {
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for SaveSystem {
    fn default() -> Self {
        Self::new()
    }
}
