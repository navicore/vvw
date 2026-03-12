//! Lighting components

use bevy::prelude::*;

pub use vvw_core::lighting::LightingConfig;

/// A 2D point light that emits light in a circular radius or directional cone.
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
    /// Direction the light faces. `None` = omnidirectional (lantern).
    pub direction: Option<Vec2>,
    /// Cosine of the cone half-angle. Tiles with a dot product below this
    /// are outside the cone. `None` = omnidirectional.
    pub half_angle_cos: Option<f32>,
}

impl Default for PointLight2d {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            intensity: 1.0,
            radius: 200.0,
            falloff: 2.0,
            direction: None,
            half_angle_cos: None,
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

/// Tile-resolution grid of light occluders for shadow casting.
/// Populated by game code from the maze; consumed by the lighting renderer.
#[derive(Resource, Default, Clone)]
pub struct LightOccluderGrid {
    pub width: usize,
    pub height: usize,
    pub tile_size: f32,
    cells: Vec<bool>,
}

impl LightOccluderGrid {
    pub fn new(width: usize, height: usize, tile_size: f32) -> Self {
        Self {
            width,
            height,
            tile_size,
            cells: vec![false; width * height],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, blocked: bool) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = blocked;
        }
    }

    /// Returns true if the tile blocks light. Out-of-bounds = blocked.
    pub fn is_occluder(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 {
            return true;
        }
        let (ux, uy) = (x as usize, y as usize);
        if ux >= self.width || uy >= self.height {
            return true;
        }
        self.cells[uy * self.width + ux]
    }
}
