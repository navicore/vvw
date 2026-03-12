//! Lighting configuration — platform-independent tunable parameters

use serde::{Deserialize, Serialize};

/// Player light mode — omnidirectional lantern or directional flashlight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LightMode {
    /// 360° radial light (original behavior)
    #[default]
    Lantern,
    /// Narrow cone in the player's facing direction
    Flashlight,
}

/// Tunable lighting parameters. Mutated by UI sliders; applied to
/// actual light components each frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy-ecs", derive(bevy::prelude::Resource))]
pub struct LightingConfig {
    pub ambient_brightness: f32,
    pub player_intensity: f32,
    pub player_radius: f32,
    pub player_falloff: f32,
    pub track_intensity: f32,
    pub track_radius: f32,
    pub track_falloff: f32,
    /// Whether to spawn point lights on track icons. When false, the maze is
    /// lit only by the player's lantern and ambient light.
    #[serde(default = "default_true")]
    pub track_lights_enabled: bool,
    /// Player light mode: Lantern (default) or Flashlight
    #[serde(default)]
    pub player_light_mode: LightMode,
    /// Half-angle of the flashlight cone in degrees (default 15° = 30° total spread)
    #[serde(default = "default_flashlight_half_angle")]
    pub flashlight_half_angle: f32,
}

fn default_true() -> bool {
    true
}

fn default_flashlight_half_angle() -> f32 {
    15.0
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            ambient_brightness: 0.09,
            player_intensity: 0.9,
            player_radius: 325.0,
            player_falloff: 0.06,
            track_intensity: 0.4,
            track_radius: 100.0,
            track_falloff: 0.6,
            track_lights_enabled: false,
            player_light_mode: LightMode::default(),
            flashlight_half_angle: default_flashlight_half_angle(),
        }
    }
}
