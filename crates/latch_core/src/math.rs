//! Deterministic math utilities
//!
//! Re-exports glam with additional deterministic utilities

pub use glam::*;

/// Deterministic random number generator (placeholder)
pub struct DeterministicRng {
    seed: u64,
    state: u64,
}

impl DeterministicRng {
    pub fn new(seed: u64) -> Self {
        Self { seed, state: seed }
    }

    /// Simple deterministic pseudo-random (use better algorithm in production)
    pub fn next_u32(&mut self) -> u32 {
        // LCG constants
        const A: u64 = 1664525;
        const C: u64 = 1013904223;
        const M: u64 = 1u64 << 32;

        self.state = (A.wrapping_mul(self.state).wrapping_add(C)) % M;
        self.state as u32
    }

    pub fn next_f32(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }
}
