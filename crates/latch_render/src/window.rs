//! Window management
//!
//! Cross-platform window creation via winit

use winit::{
    event_loop::EventLoop,
    window::{Window, WindowAttributes},
};

pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Latch Engine".to_string(),
            width: 1280,
            height: 720,
        }
    }
}

pub fn create_window(event_loop: &EventLoop<()>, config: WindowConfig) -> Window {
    let window_attributes = Window::default_attributes()
        .with_title(config.title)
        .with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height));
    
    event_loop
        .create_window(window_attributes)
        .expect("Failed to create window")
}
