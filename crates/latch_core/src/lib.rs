//! Latch Engine Core
//!
//! Contains the fundamental simulation systems:
//! - Entity Component System (ECS)
//! - Job scheduler
//! - Deterministic time and math
//! - Memory management

pub mod ecs;
pub mod time;
pub mod math;
pub mod memory;

// Re-export metrics from latch_metrics for convenience
#[cfg(feature = "metrics")]
pub use latch_metrics as metrics;

pub use glam;

/// Engine version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!VERSION.is_empty());
    }
}
