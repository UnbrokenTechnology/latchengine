//! Named counters for tracking events

use std::collections::HashMap;

pub struct Counter {
    counters: HashMap<String, usize>,
}

impl Counter {
    pub fn new() -> Self {
        Self {
            counters: HashMap::new(),
        }
    }
    
    pub fn increment(&mut self, name: &str, value: usize) {
        *self.counters.entry(name.to_string()).or_insert(0) += value;
    }
    
    pub fn set(&mut self, name: &str, value: usize) {
        self.counters.insert(name.to_string(), value);
    }
    
    pub fn get(&self, name: &str) -> usize {
        self.counters.get(name).copied().unwrap_or(0)
    }
    
    pub fn reset(&mut self, name: &str) {
        self.counters.insert(name.to_string(), 0);
    }
    
    pub fn reset_all(&mut self) {
        self.counters.clear();
    }
    
    pub fn iter(&self) -> impl Iterator<Item = (&String, &usize)> {
        self.counters.iter()
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}
