//! VVW 2D Lighting
//!
//! Provides simple 2D point lighting with ambient darkness.
//! Uses sprite-based rendering for maximum Bevy version compatibility.
//!
//! Components:
//! - `PointLight2d` — attached to entities to emit light
//! - `AmbientLight2d` — controls global ambient brightness
//! - `LightOccluder2d` — marks entities that block light (used for LOS in game code)

mod components;
mod render;

pub use components::{
    AmbientLight2d, LightOccluder2d, LightOccluderGrid, LightingConfig, PointLight2d,
};
pub use render::Lighting2dPlugin;
