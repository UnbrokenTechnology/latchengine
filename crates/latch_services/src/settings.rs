//! Settings management

use serde::{Deserialize, Serialize};

/// Engine settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub graphics: GraphicsSettings,
    pub audio: AudioSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicsSettings {
    pub resolution_width: u32,
    pub resolution_height: u32,
    pub fullscreen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    pub master_volume: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            graphics: GraphicsSettings {
                resolution_width: 1280,
                resolution_height: 720,
                fullscreen: false,
            },
            audio: AudioSettings { master_volume: 1.0 },
        }
    }
}
