//! Latch Render System
//!
//! Cross-platform rendering with automatic backend selection and fallbacks

pub mod backend;
pub mod window;

pub use wgpu;
pub use winit;

/// Rendering backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    /// Metal (macOS, iOS)
    Metal,
    /// DirectX 11 (Windows)
    DirectX11,
    /// DirectX 12 (Windows)
    DirectX12,
    /// Vulkan (cross-platform)
    Vulkan,
    /// OpenGL (cross-platform, fallback)
    OpenGL,
    /// WebGL (web)
    WebGL,
    /// Software rasterizer (ultimate fallback)
    Software,
}

/// Capability probe result
#[derive(Debug, Clone)]
pub struct DeviceCapabilities {
    pub backend: BackendType,
    pub max_texture_size: u32,
    pub supports_compute: bool,
    pub supports_instancing: bool,
}
