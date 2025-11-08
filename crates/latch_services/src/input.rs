//! Input abstraction and recording for replays

/// Input state (placeholder)
#[derive(Debug, Clone, Copy)]
pub struct InputState {
    pub move_x: f32,
    pub move_y: f32,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            move_x: 0.0,
            move_y: 0.0,
        }
    }
}
