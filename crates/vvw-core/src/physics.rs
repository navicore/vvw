//! Physics configuration — platform-independent tunable parameters

use serde::{Deserialize, Serialize};

/// Tunable physics parameters for the player and wall colliders.
/// Stored in `ProjectManifest` alongside `LightingConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy-ecs", derive(bevy::prelude::Resource))]
pub struct PhysicsConfig {
    /// Player movement impulse magnitude (terminal velocity ≈ speed / `linear_damping`)
    pub player_speed: f32,
    /// Player body friction
    pub player_friction: f32,
    /// Player body bounciness (0 = no bounce, 1 = full bounce).
    /// Note: values above ~0.7 with low damping can cause sustained bounce
    /// cycles in corridors narrower than 2 tiles.
    pub player_restitution: f32,
    /// Linear velocity damping — higher values stop the player faster
    pub player_linear_damping: f32,
    /// Angular velocity damping — higher values stop spinning faster
    pub player_angular_damping: f32,
    /// Wall collider friction
    pub wall_friction: f32,
    /// Wall collider bounciness
    pub wall_restitution: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            player_speed: 600.0,
            player_friction: 0.3,
            player_restitution: 0.7,
            player_linear_damping: 2.5,
            player_angular_damping: 5.0,
            wall_friction: 0.3,
            wall_restitution: 0.6,
        }
    }
}
