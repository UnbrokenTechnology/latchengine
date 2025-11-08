//! Deterministic time system
//!
//! Fixed 60Hz tick rate with interpolation for rendering
//! Supports input recording/replay for determinism validation

use std::time::{Duration, Instant};

/// Fixed simulation tick rate (60 Hz = 16.666ms per tick)
pub const TICK_RATE_HZ: u32 = 60;
pub const TICK_DURATION_SECS: f32 = 1.0 / 60.0; // 0.01666...
pub const TICK_DURATION: Duration = Duration::from_micros(16_666); // ~16.666ms

/// Simulation time tracker with fixed timestep
pub struct SimulationTime {
    tick_count: u64,
    accumulated_time: Duration,
    last_update: Instant,
    lag: Duration,
}

impl SimulationTime {
    pub fn new() -> Self {
        Self {
            tick_count: 0,
            accumulated_time: Duration::ZERO,
            last_update: Instant::now(),
            lag: Duration::ZERO,
        }
    }

    /// Get current tick number
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Get total simulation time elapsed
    pub fn total_time(&self) -> Duration {
        self.accumulated_time
    }

    /// Get delta time for this tick (always fixed)
    pub fn delta_time(&self) -> f32 {
        TICK_DURATION_SECS
    }

    /// Update with elapsed wall-clock time
    /// Returns number of ticks to simulate this frame
    pub fn update(&mut self) -> u32 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        self.last_update = now;

        self.lag += elapsed;

        let mut ticks = 0;
        while self.lag >= TICK_DURATION && ticks < 4 {
            // Max 4 ticks per frame to avoid spiral of death
            self.lag -= TICK_DURATION;
            self.tick_count += 1;
            self.accumulated_time += TICK_DURATION;
            ticks += 1;
        }

        ticks
    }

    /// Get interpolation alpha for smooth rendering between ticks
    pub fn interpolation_alpha(&self) -> f32 {
        self.lag.as_secs_f32() / TICK_DURATION_SECS
    }

    /// Reset time (for replay)
    pub fn reset(&mut self) {
        self.tick_count = 0;
        self.accumulated_time = Duration::ZERO;
        self.lag = Duration::ZERO;
        self.last_update = Instant::now();
    }
}

impl Default for SimulationTime {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Input Recording for Determinism
// ============================================================================

/// Input event for a single tick
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickInput {
    pub tick: u64,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub mouse_pressed: bool,
}

/// Input recorder for replay validation
pub struct InputRecorder {
    inputs: Vec<TickInput>,
    recording: bool,
    playback_index: usize,
}

impl InputRecorder {
    pub fn new() -> Self {
        Self {
            inputs: Vec::new(),
            recording: false,
            playback_index: 0,
        }
    }

    /// Start recording inputs
    pub fn start_recording(&mut self) {
        self.inputs.clear();
        self.recording = true;
        self.playback_index = 0;
    }

    /// Stop recording
    pub fn stop_recording(&mut self) {
        self.recording = false;
    }

    /// Record input for this tick
    pub fn record(&mut self, input: TickInput) {
        if self.recording {
            self.inputs.push(input);
        }
    }

    /// Get recorded input for playback (returns None if replay finished)
    pub fn playback(&mut self, tick: u64) -> Option<TickInput> {
        if self.playback_index < self.inputs.len() {
            let input = self.inputs[self.playback_index];
            if input.tick == tick {
                self.playback_index += 1;
                return Some(input);
            }
        }
        None
    }

    /// Start replay from beginning
    pub fn start_playback(&mut self) {
        self.playback_index = 0;
    }

    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Get number of recorded inputs
    pub fn input_count(&self) -> usize {
        self.inputs.len()
    }

    /// Export recorded inputs (for saving to file)
    pub fn export(&self) -> &[TickInput] {
        &self.inputs
    }

    /// Import recorded inputs (from file)
    pub fn import(&mut self, inputs: Vec<TickInput>) {
        self.inputs = inputs;
        self.playback_index = 0;
    }
}

impl Default for InputRecorder {
    fn default() -> Self {
        Self::new()
    }
}
