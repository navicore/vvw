//! VVW Game Plugin
//!
//! Provides the core game functionality for the Visual Virtual World:
//! - 2D maze rendering and navigation
//! - Physics-based player movement (avian2d)
//! - Spatial audio with line-of-sight

mod audio;
mod camera;
mod maze;
mod mazegen;
mod player;
pub mod project;
mod spatial;
mod tiles;

pub use audio::AudioPlugin;
pub use camera::CameraPlugin;
pub use maze::{Maze, MazePlugin};
pub use player::{Player, PlayerPlugin};
pub use project::StartupProject;
pub use tiles::{TileKind, TilePos};

use avian2d::PhysicsPlugins;
use avian2d::prelude::Gravity;
use bevy::prelude::*;
use bevy_egui::EguiPlugin;

/// Main game plugin that bundles all VVW game systems
pub struct VvwGamePlugin;

impl Plugin for VvwGamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Gravity(Vec2::ZERO)).add_plugins((
            PhysicsPlugins::default(),
            EguiPlugin::default(),
            MazePlugin,
            PlayerPlugin,
            AudioPlugin,
            CameraPlugin,
        ));
    }
}
