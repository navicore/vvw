//! Lighting configuration — platform-independent tunable parameters

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Tunable lighting parameters. Mutated by UI sliders; applied to
/// actual light components each frame.
#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct LightingConfig {
    pub ambient_brightness: f32,
    pub player_intensity: f32,
    pub player_radius: f32,
    pub player_falloff: f32,
    pub track_intensity: f32,
    pub track_radius: f32,
    pub track_falloff: f32,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            ambient_brightness: 0.15,
            player_intensity: 0.4,
            player_radius: 100.0,
            player_falloff: 0.6,
            track_intensity: 0.4,
            track_radius: 100.0,
            track_falloff: 0.6,
        }
    }
}
