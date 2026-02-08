//! Tile types and grid position utilities

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Size of each tile in world units
pub const TILE_SIZE: f32 = 32.0;

/// The kind of tile at a grid position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TileKind {
    /// Empty floor tile - player can walk here
    #[default]
    Floor,
    /// Wall tile - blocks movement and line of sight
    Wall,
    /// Player starting position (treated as floor after spawn)
    PlayerStart,
    /// Audio track location
    TrackIcon,
}

impl TileKind {
    /// Returns true if this tile blocks movement
    pub const fn is_solid(&self) -> bool {
        matches!(self, Self::Wall)
    }

    /// Returns true if this tile blocks line of sight
    pub const fn blocks_sight(&self) -> bool {
        matches!(self, Self::Wall)
    }
}

/// Grid position component (discrete tile coordinates)
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TilePos {
    pub x: i32,
    pub y: i32,
}

impl TilePos {
    /// Create a new tile position
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Convert tile position to world position (center of tile)
    pub fn to_world(self) -> Vec2 {
        Vec2::new(
            (self.x as f32).mul_add(TILE_SIZE, TILE_SIZE / 2.0),
            (self.y as f32).mul_add(TILE_SIZE, TILE_SIZE / 2.0),
        )
    }

    /// Convert world position to tile position
    pub fn from_world(world: Vec2) -> Self {
        Self {
            x: (world.x / TILE_SIZE).floor() as i32,
            y: (world.y / TILE_SIZE).floor() as i32,
        }
    }

    /// Calculate Manhattan distance to another tile
    pub fn manhattan_distance(self, other: Self) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }

    /// Calculate Euclidean distance to another tile
    pub fn distance(self, other: Self) -> f32 {
        let dx = (self.x - other.x) as f32;
        let dy = (self.y - other.y) as f32;
        dx.hypot(dy)
    }

    /// Get adjacent tile in a direction
    pub fn neighbor(self, direction: Direction) -> Self {
        match direction {
            Direction::Up => Self::new(self.x, self.y + 1),
            Direction::Down => Self::new(self.x, self.y - 1),
            Direction::Left => Self::new(self.x - 1, self.y),
            Direction::Right => Self::new(self.x + 1, self.y),
        }
    }
}

impl From<(i32, i32)> for TilePos {
    fn from((x, y): (i32, i32)) -> Self {
        Self { x, y }
    }
}

/// Cardinal direction for movement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    /// Get the opposite direction
    pub const fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// Convert to a unit vector
    pub const fn as_vec2(self) -> Vec2 {
        match self {
            Self::Up => Vec2::new(0.0, 1.0),
            Self::Down => Vec2::new(0.0, -1.0),
            Self::Left => Vec2::new(-1.0, 0.0),
            Self::Right => Vec2::new(1.0, 0.0),
        }
    }
}
