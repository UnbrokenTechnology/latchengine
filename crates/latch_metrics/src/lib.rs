//! Latch Metrics - Common utilities for performance tracking
//!
//! Provides zero-cost abstractions for metrics collection that completely
//! vanish in production builds via feature flags.
//!
//! # Feature Flags
//!
//! - `metrics` - Enable metrics collection (default: disabled)
//!
//! # Usage
//!
//! ```ignore
//! use latch_metrics::{FrameTimer, RingBuffer};
//!
//! let mut timer = FrameTimer::new(60); // Track last 60 frames
//! timer.begin();
//! // ... do work ...
//! timer.end();
//! println!("FPS: {:.1}", timer.fps());
//! ```
//!
//! In production builds (without `metrics` feature), all instrumentation
//! is compiled out to zero overhead.

#[cfg(feature = "metrics")]
mod frame_timer;
#[cfg(feature = "metrics")]
mod ring_buffer;
#[cfg(feature = "metrics")]
mod counter;
#[cfg(feature = "metrics")]
mod system_profiler;

#[cfg(feature = "metrics")]
pub use frame_timer::FrameTimer;
#[cfg(feature = "metrics")]
pub use ring_buffer::RingBuffer;
#[cfg(feature = "metrics")]
pub use counter::Counter;
#[cfg(feature = "metrics")]
pub use system_profiler::SystemProfiler;

// ============================================================================
// Macros for conditional compilation
// ============================================================================

/// Execute code only when metrics are enabled
#[macro_export]
macro_rules! metrics {
    ($($tt:tt)*) => {
        #[cfg(feature = "metrics")]
        {
            $($tt)*
        }
    };
}

/// Begin timing a scope (zero-cost when metrics disabled)
#[macro_export]
macro_rules! time_scope {
    ($profiler:expr, $name:expr, $body:block) => {
        #[cfg(feature = "metrics")]
        {
            $profiler.time_system($name, || $body)
        }
        #[cfg(not(feature = "metrics"))]
        {
            $body
        }
    };
}

// ============================================================================
// No-op stubs when metrics disabled
// ============================================================================

#[cfg(not(feature = "metrics"))]
pub struct FrameTimer;

#[cfg(not(feature = "metrics"))]
impl FrameTimer {
    pub fn new(_capacity: usize) -> Self { Self }
    pub fn begin(&mut self) {}
    pub fn end(&mut self) {}
    pub fn fps(&self) -> f64 { 0.0 }
    pub fn frame_time_ms(&self) -> f64 { 0.0 }
}

#[cfg(not(feature = "metrics"))]
pub struct RingBuffer<T>(std::marker::PhantomData<T>);

#[cfg(not(feature = "metrics"))]
impl<T> RingBuffer<T> {
    pub fn new(_capacity: usize) -> Self { Self(std::marker::PhantomData) }
    pub fn push(&mut self, _value: T) {}
    pub fn average(&self) -> T where T: Default { T::default() }
}

#[cfg(not(feature = "metrics"))]
pub struct Counter;

#[cfg(not(feature = "metrics"))]
impl Counter {
    pub fn new() -> Self { Self }
    pub fn increment(&mut self, _name: &str, _value: usize) {}
    pub fn get(&self, _name: &str) -> usize { 0 }
}

#[cfg(not(feature = "metrics"))]
pub struct SystemProfiler;

#[cfg(not(feature = "metrics"))]
impl SystemProfiler {
    pub fn new() -> Self { Self }
    pub fn time_system<F, R>(&mut self, _name: &str, f: F) -> R where F: FnOnce() -> R { f() }
    pub fn get_timing(&self, _name: &str) -> std::time::Duration { std::time::Duration::ZERO }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_compiles_without_metrics() {
        // Ensure stubs compile when metrics feature is disabled
        let mut _timer = super::FrameTimer::new(60);
        let mut _buffer = super::RingBuffer::<f64>::new(10);
        let mut _counter = super::Counter::new();
        let mut _profiler = super::SystemProfiler::new();
    }
}
