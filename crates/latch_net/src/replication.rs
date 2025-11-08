//! State replication and rollback networking

/// Tick number for rollback
pub type Tick = u64;

/// Input buffer for rollback
pub struct InputBuffer {
    buffer_size: usize,
}

impl InputBuffer {
    pub fn new(buffer_size: usize) -> Self {
        Self { buffer_size }
    }
}

impl Default for InputBuffer {
    fn default() -> Self {
        Self::new(2) // 2-tick buffer (~33ms)
    }
}
