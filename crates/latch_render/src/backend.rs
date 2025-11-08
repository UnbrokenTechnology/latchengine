//! Rendering backend abstraction
//!
//! Phase 0: wgpu-based prototype
//! Phase 1: Custom backends for D3D9, GL2.1, software rasterizer

use crate::{BackendType, DeviceCapabilities};

/// Probe available rendering capabilities
pub fn probe_capabilities() -> DeviceCapabilities {
    // Placeholder: wgpu will handle backend selection initially
    DeviceCapabilities {
        backend: BackendType::Metal, // Will detect automatically
        max_texture_size: 8192,
        supports_compute: true,
        supports_instancing: true,
    }
}
