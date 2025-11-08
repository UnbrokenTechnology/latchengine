//! Memory management utilities
//!
//! Arena allocators, tracking, and budgets

/// Per-frame allocation tracker (placeholder)
pub struct AllocationTracker {
    frame_allocations: usize,
}

impl AllocationTracker {
    pub fn new() -> Self {
        Self {
            frame_allocations: 0,
        }
    }

    pub fn record_allocation(&mut self, size: usize) {
        self.frame_allocations += size;
    }

    pub fn reset_frame(&mut self) {
        self.frame_allocations = 0;
    }

    pub fn frame_allocations(&self) -> usize {
        self.frame_allocations
    }
}

impl Default for AllocationTracker {
    fn default() -> Self {
        Self::new()
    }
}
