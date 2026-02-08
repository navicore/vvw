//! Lighting components

use bevy::prelude::*;

/// A 2D point light that emits light in a circular radius.
/// Attach as a child entity of the light source.
#[derive(Component, Debug, Clone)]
pub struct PointLight2d {
    /// Light color
    pub color: Color,
    /// Intensity multiplier
    pub intensity: f32,
    /// Maximum radius in world units
    pub radius: f32,
    /// Falloff exponent (higher = sharper edges)
    pub falloff: f32,
}

impl Default for PointLight2d {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            intensity: 1.0,
            radius: 200.0,
            falloff: 2.0,
        }
    }
}

/// Global ambient light level. Insert as a resource.
#[derive(Resource, Debug, Clone)]
pub struct AmbientLight2d {
    /// Ambient color (typically a dim blue/purple)
    pub color: Color,
    /// Brightness from 0.0 (pitch black) to 1.0 (fully lit)
    pub brightness: f32,
}

impl Default for AmbientLight2d {
    fn default() -> Self {
        Self {
            color: Color::srgb(0.1, 0.1, 0.2),
            brightness: 0.15,
        }
    }
}

/// Marks an entity as a light occluder (blocks light).
/// The actual occlusion is handled by game-level LOS checks;
/// this component is a marker for the lighting system.
#[derive(Component, Debug, Clone)]
pub struct LightOccluder2d {
    /// Rectangle half-extents for the occluder
    pub half_size: Vec2,
}
