//! Frame timing utilities

use super::ring_buffer::RingBuffer;
use std::time::{Duration, Instant};

pub struct FrameTimer {
    frame_start: Instant,
    frame_times: RingBuffer<Duration>,
}

impl FrameTimer {
    pub fn new(capacity: usize) -> Self {
        Self {
            frame_start: Instant::now(),
            frame_times: RingBuffer::new(capacity),
        }
    }

    pub fn begin(&mut self) {
        self.frame_start = Instant::now();
    }

    pub fn end(&mut self) {
        let elapsed = self.frame_start.elapsed();
        self.frame_times.push(elapsed);
    }

    pub fn fps(&self) -> f64 {
        let avg = self.frame_times.average();
        if avg.as_secs_f64() > 0.0 {
            1.0 / avg.as_secs_f64()
        } else {
            0.0
        }
    }

    pub fn frame_time_ms(&self) -> f64 {
        self.frame_times.average().as_secs_f64() * 1000.0
    }

    pub fn frame_time_range_ms(&self) -> (f64, f64) {
        let (min, max) = self.frame_times.min_max();
        (min.as_secs_f64() * 1000.0, max.as_secs_f64() * 1000.0)
    }
}
