//! VVW Game Plugin — platform-independent game logic
//!
//! Provides the core game functionality for the Visual Virtual World:
//! - 2D maze rendering with physics colliders and light occluders
//! - Physics-based player movement (avian2d)
//! - Spatial audio with line-of-sight gain/pan interpolation
//! - Custom 2D lighting (vvw-light)
//!
//! Platform-specific code (audio backends, UI, file I/O) lives in the app layer.
//! The platform inserts the `Maze` resource; this plugin handles everything else.

mod audio;
mod camera;
mod maze;
pub mod mazegen;
mod player;
mod spatial;
mod tiles;
mod touch;

pub use audio::{SpatialAudioPlugin, SpatialAudioSet, TrackAudioState, TrackIdCounter};
pub use camera::{CameraPlugin, GameCamera};
pub use maze::{
    Maze, MazeChanged, MazePlugin, MazeTile, TrackIcon, TrackLight, colors, spawn_maze_tiles,
};
pub use player::{Player, PlayerLight, PlayerMovement, PlayerPlugin};
pub use tiles::{TILE_SIZE, TileKind, TilePos};

use avian2d::PhysicsPlugins;
use avian2d::prelude::Gravity;
use bevy::prelude::*;

/// Main game plugin — platform-independent core.
///
/// The platform layer must:
/// 1. Insert a `Maze` resource before `PostStartup`
/// 2. Call `spawn_maze_tiles` in a `Startup` system
/// 3. Read `TrackAudioState` after `SpatialAudioSet` and push to its audio backend
pub struct VvwGamePlugin;

impl Plugin for VvwGamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Gravity(Vec2::ZERO)).add_plugins((
            PhysicsPlugins::default(),
            MazePlugin,
            PlayerPlugin,
            SpatialAudioPlugin,
            CameraPlugin,
            touch::TouchControlsPlugin,
        ));
    }
}
