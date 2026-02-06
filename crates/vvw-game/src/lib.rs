//! VVW Game Plugin
//!
//! Provides the core game functionality for the Visual Virtual World:
//! - 2D maze rendering and navigation
//! - Player movement with grid-snapping (Pacman-style)
//! - Tile-based collision detection

mod audio;
mod maze;
mod player;
mod tiles;

pub use audio::AudioPlugin;
pub use maze::{Maze, MazePlugin};
pub use player::{Player, PlayerPlugin};
pub use tiles::{TileKind, TilePos};

use bevy::prelude::*;

/// Main game plugin that bundles all VVW game systems
pub struct VvwGamePlugin;

impl Plugin for VvwGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((MazePlugin, PlayerPlugin, AudioPlugin));
    }
}
