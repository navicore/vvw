//! Maze data structure — platform-independent grid storage and queries

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::tiles::{TileKind, TilePos};

/// Maze resource containing the grid layout
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy-ecs", derive(bevy::prelude::Resource))]
pub struct Maze {
    /// Width of the maze in tiles
    pub width: usize,
    /// Height of the maze in tiles
    pub height: usize,
    /// Grid data stored row-major (y * width + x)
    tiles: Vec<TileKind>,
    /// Map from tile (x, y) to `track_id` (preserves insertion order across expansions)
    #[serde(default)]
    pub track_ids: HashMap<(usize, usize), usize>,
}

impl Maze {
    /// Create a new maze with all wall tiles
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: vec![TileKind::Wall; width * height],
            track_ids: HashMap::new(),
        }
    }

    /// Create a new maze filled with floor tiles
    pub fn new_floor(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: vec![TileKind::Floor; width * height],
            track_ids: HashMap::new(),
        }
    }

    /// Create a simple test maze with walls around the border
    pub fn simple_test_maze() -> Self {
        let width = 15;
        let height = 11;
        let mut maze = Self::new_floor(width, height);

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
        for x in 2..8 {
            maze.set(x, 4, TileKind::Wall);
        }
        for y in 2..6 {
            maze.set(10, y, TileKind::Wall);
        }
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

    /// Re-stamp all `TrackIcon` tiles from the `track_ids` map.
    /// Call after carving corridors/rooms that may have overwritten existing icons.
    pub fn restore_track_icons(&mut self) {
        let positions: Vec<(usize, usize)> = self.track_ids.keys().copied().collect();
        for (x, y) in positions {
            self.set(x, y, TileKind::TrackIcon);
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

    /// Expand the maze grid in all four directions.
    /// Returns the offset (dx, dy) applied to the origin.
    /// All existing coordinates shift by this offset.
    pub fn expand(&mut self, left: usize, right: usize, bottom: usize, top: usize) -> (i32, i32) {
        let new_width = self.width + left + right;
        let new_height = self.height + bottom + top;
        let mut new_tiles = vec![TileKind::Wall; new_width * new_height];

        // Copy existing tiles to new positions (shifted by left, bottom)
        for y in 0..self.height {
            for x in 0..self.width {
                let old_idx = y * self.width + x;
                let new_x = x + left;
                let new_y = y + bottom;
                let new_idx = new_y * new_width + new_x;
                new_tiles[new_idx] = self.tiles[old_idx];
            }
        }

        self.tiles = new_tiles;
        self.width = new_width;
        self.height = new_height;

        // Shift track_id keys by the expansion offset
        if left > 0 || bottom > 0 {
            self.track_ids = self
                .track_ids
                .drain()
                .map(|((x, y), id)| ((x + left, y + bottom), id))
                .collect();
        }

        (left as i32, bottom as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_preserves_tiles() {
        let mut maze = Maze::new_floor(3, 3);
        maze.set(1, 1, TileKind::PlayerStart);

        let (dx, dy) = maze.expand(2, 2, 2, 2);
        assert_eq!(dx, 2);
        assert_eq!(dy, 2);
        assert_eq!(maze.width, 7);
        assert_eq!(maze.height, 7);

        // Original (1,1) is now at (3,3)
        assert_eq!(maze.get(3, 3), Some(TileKind::PlayerStart));
        // New border tiles should be walls
        assert_eq!(maze.get(0, 0), Some(TileKind::Wall));
    }
}
