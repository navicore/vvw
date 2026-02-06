//! Maze data structure and rendering

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::tiles::{TILE_SIZE, TileKind, TilePos};

/// Maze resource containing the grid layout
#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct Maze {
    /// Width of the maze in tiles
    pub width: usize,
    /// Height of the maze in tiles
    pub height: usize,
    /// Grid data stored row-major (y * width + x)
    tiles: Vec<TileKind>,
}

impl Maze {
    /// Create a new maze with all floor tiles
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: vec![TileKind::Floor; width * height],
        }
    }

    /// Create a simple test maze with walls around the border
    pub fn simple_test_maze() -> Self {
        let width = 15;
        let height = 11;
        let mut maze = Self::new(width, height);

        // Add border walls
        for x in 0..width {
            maze.set(x, 0, TileKind::Wall);
            maze.set(x, height - 1, TileKind::Wall);
        }
        for y in 0..height {
            maze.set(0, y, TileKind::Wall);
            maze.set(width - 1, y, TileKind::Wall);
        }

        // Add some internal walls to make it interesting
        // Horizontal wall with gap
        for x in 2..8 {
            maze.set(x, 4, TileKind::Wall);
        }
        // Vertical wall with gap
        for y in 2..6 {
            maze.set(10, y, TileKind::Wall);
        }
        // Another horizontal wall
        for x in 6..13 {
            maze.set(x, 7, TileKind::Wall);
        }

        // Player start position
        maze.set(2, 2, TileKind::PlayerStart);

        // Track icon positions
        maze.set(7, 2, TileKind::TrackIcon);
        maze.set(12, 5, TileKind::TrackIcon);
        maze.set(3, 8, TileKind::TrackIcon);

        maze
    }

    /// Get the tile at a grid position
    pub fn get(&self, x: usize, y: usize) -> Option<TileKind> {
        if x < self.width && y < self.height {
            Some(self.tiles[y * self.width + x])
        } else {
            None
        }
    }

    /// Get the tile at a `TilePos`
    pub fn get_tile(&self, pos: &TilePos) -> Option<TileKind> {
        if pos.x >= 0 && pos.y >= 0 {
            self.get(pos.x as usize, pos.y as usize)
        } else {
            None
        }
    }

    /// Set the tile at a grid position
    pub fn set(&mut self, x: usize, y: usize, kind: TileKind) {
        if x < self.width && y < self.height {
            self.tiles[y * self.width + x] = kind;
        }
    }

    /// Check if a tile position is walkable
    pub fn is_walkable(&self, pos: &TilePos) -> bool {
        self.get_tile(pos).is_some_and(|tile| !tile.is_solid())
    }

    /// Check if a tile blocks line of sight
    pub fn is_wall(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 {
            return true;
        }
        self.get(x as usize, y as usize)
            .is_none_or(|tile| tile.blocks_sight())
    }

    /// Find the player start position
    pub fn find_player_start(&self) -> Option<TilePos> {
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) == Some(TileKind::PlayerStart) {
                    return Some(TilePos::new(x as i32, y as i32));
                }
            }
        }
        None
    }

    /// Find all track icon positions
    pub fn find_track_icons(&self) -> Vec<TilePos> {
        let mut positions = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) == Some(TileKind::TrackIcon) {
                    positions.push(TilePos::new(x as i32, y as i32));
                }
            }
        }
        positions
    }
}

/// Marker component for maze tiles (for cleanup)
#[derive(Component)]
pub struct MazeTile;

/// Marker component for track icons
#[derive(Component)]
pub struct TrackIcon {
    pub track_id: usize,
}

/// Plugin for maze loading and rendering
pub struct MazePlugin;

impl Plugin for MazePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_maze);
    }
}

/// Colors for different tile types
mod colors {
    use bevy::prelude::*;

    pub const FLOOR: Color = Color::srgb(0.15, 0.15, 0.2);
    pub const WALL: Color = Color::srgb(0.4, 0.35, 0.5);
    pub const PLAYER_START: Color = Color::srgb(0.2, 0.3, 0.2);
    pub const TRACK_ICON: Color = Color::srgb(0.8, 0.4, 0.2);
}

fn setup_maze(mut commands: Commands) {
    // Create and insert the test maze
    let maze = Maze::simple_test_maze();

    // Spawn tile sprites
    for y in 0..maze.height {
        for x in 0..maze.width {
            let tile = maze.get(x, y).unwrap_or_default();
            let pos = TilePos::new(x as i32, y as i32);
            let world_pos = pos.to_world();

            let color = match tile {
                TileKind::Wall => colors::WALL,
                TileKind::PlayerStart => colors::PLAYER_START,
                TileKind::Floor | TileKind::TrackIcon => colors::FLOOR,
            };

            // Spawn floor/wall tile
            commands.spawn((
                Sprite {
                    color,
                    custom_size: Some(Vec2::splat(TILE_SIZE - 1.0)), // Small gap between tiles
                    ..default()
                },
                Transform::from_xyz(world_pos.x, world_pos.y, 0.0),
                MazeTile,
            ));

            // Spawn track icon on top if this is a track position
            if tile == TileKind::TrackIcon {
                commands.spawn((
                    Sprite {
                        color: colors::TRACK_ICON,
                        custom_size: Some(Vec2::splat(TILE_SIZE * 0.6)),
                        ..default()
                    },
                    Transform::from_xyz(world_pos.x, world_pos.y, 1.0),
                    TrackIcon {
                        track_id: maze
                            .find_track_icons()
                            .iter()
                            .position(|p| *p == pos)
                            .unwrap_or(0),
                    },
                    pos,
                ));
            }
        }
    }

    // Insert maze as resource
    commands.insert_resource(maze);
}
