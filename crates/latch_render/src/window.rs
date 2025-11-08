//! Window management
//!
//! Cross-platform window creation via winit

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
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

/// Create window attributes from config
pub fn window_attributes(config: WindowConfig) -> winit::window::WindowAttributes {
    Window::default_attributes()
        .with_title(config.title)
        .with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height))
}

/// Helper to create a window within the event loop (winit 0.30+ API)
/// 
/// Note: In winit 0.30+, windows must be created inside the `resumed` event.
/// This is a helper that provides the old-style API for simple cases.
/// For production use, implement ApplicationHandler trait directly.
pub fn create_event_loop() -> EventLoop<()> {
    EventLoop::new().expect("Failed to create event loop")
}

/// Simple application handler for basic window creation
pub struct SimpleWindowApp {
    config: WindowConfig,
    window: Option<Window>,
}

impl SimpleWindowApp {
    pub fn new(config: WindowConfig) -> Self {
        Self {
            config,
            window: None,
        }
    }

    pub fn window(&self) -> Option<&Window> {
        self.window.as_ref()
    }
}

impl ApplicationHandler for SimpleWindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = window_attributes(WindowConfig {
                title: self.config.title.clone(),
                width: self.config.width,
                height: self.config.height,
            });
            self.window = Some(
                event_loop
                    .create_window(attrs)
                    .expect("Failed to create window"),
            );
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
        // Will be implemented in PoC 1
    }
}
