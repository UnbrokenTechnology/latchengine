//! Ring buffer for rolling averages

use std::time::Duration;

pub struct RingBuffer<T> {
    samples: Vec<T>,
    capacity: usize,
    index: usize,
}

impl<T: Clone + Default> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: Vec::with_capacity(capacity),
            capacity,
            index: 0,
        }
    }
    
    pub fn push(&mut self, sample: T) {
        if self.samples.len() < self.capacity {
            self.samples.push(sample);
        } else {
            self.samples[self.index] = sample;
        }
        self.index = (self.index + 1) % self.capacity;
    }
    
    pub fn len(&self) -> usize {
        self.samples.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

// Specialize for Duration (common case)
impl RingBuffer<Duration> {
    pub fn average(&self) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }
        
        let sum: Duration = self.samples.iter().sum();
        sum / self.samples.len() as u32
    }
    
    pub fn min_max(&self) -> (Duration, Duration) {
        if self.samples.is_empty() {
            return (Duration::ZERO, Duration::ZERO);
        }
        
        let min = *self.samples.iter().min().unwrap();
        let max = *self.samples.iter().max().unwrap();
        (min, max)
    }
}

// Specialize for f64
impl RingBuffer<f64> {
    pub fn average(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        
        let sum: f64 = self.samples.iter().sum();
        sum / self.samples.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ring_buffer() {
        let mut buffer = RingBuffer::new(3);
        
        buffer.push(Duration::from_millis(10));
        assert_eq!(buffer.average(), Duration::from_millis(10));
        
        buffer.push(Duration::from_millis(20));
        assert_eq!(buffer.average(), Duration::from_millis(15));
        
        buffer.push(Duration::from_millis(30));
        assert_eq!(buffer.average(), Duration::from_millis(20));
        
        // Should wrap around
        buffer.push(Duration::from_millis(40));
        assert_eq!(buffer.average(), Duration::from_millis(30)); // (20 + 30 + 40) / 3
    }
}
