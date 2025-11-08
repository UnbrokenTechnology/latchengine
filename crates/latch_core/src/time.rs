//! Deterministic time system
//!
//! Fixed 60Hz tick rate with interpolation for rendering

use std::time::Duration;

/// Fixed simulation tick rate (60 Hz = 16.666ms per tick)
pub const TICK_RATE_HZ: u32 = 60;
pub const TICK_DURATION: Duration = Duration::from_micros(16_666); // ~16.666ms

/// Simulation time tracker
pub struct SimulationTime {
    tick_count: u64,
    accumulated_time: Duration,
}

impl SimulationTime {
    pub fn new() -> Self {
        Self {
            tick_count: 0,
            accumulated_time: Duration::ZERO,
        }
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    pub fn advance_tick(&mut self) {
        self.tick_count += 1;
        self.accumulated_time += TICK_DURATION;
    }

    pub fn total_time(&self) -> Duration {
        self.accumulated_time
    }
}

impl Default for SimulationTime {
    fn default() -> Self {
        Self::new()
    }
}
