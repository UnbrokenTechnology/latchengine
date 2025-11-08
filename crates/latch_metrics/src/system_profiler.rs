//! System profiler for timing named subsystems

use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct SystemProfiler {
    timings: HashMap<String, Duration>,
}

impl SystemProfiler {
    pub fn new() -> Self {
        Self {
            timings: HashMap::new(),
        }
    }
    
    pub fn time_system<F, R>(&mut self, name: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();
        
        *self.timings.entry(name.to_string()).or_insert(Duration::ZERO) += elapsed;
        result
    }
    
    pub fn get_timing(&self, name: &str) -> Duration {
        self.timings.get(name).copied().unwrap_or(Duration::ZERO)
    }
    
    pub fn reset(&mut self) {
        self.timings.clear();
    }
    
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Duration)> {
        self.timings.iter()
    }
}

impl Default for SystemProfiler {
    fn default() -> Self {
        Self::new()
    }
}
