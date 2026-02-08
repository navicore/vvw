//! Maze data structure and rendering

use std::collections::HashMap;

use avian2d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vvw_light::{LightOccluder2d, LightOccluderGrid, PointLight2d};

use crate::audio::{TrackAudioFile, TrackAudioFiles, TrackAudioState, TrackIdCounter};
use crate::mazegen::{MazeGenConfig, MazeGenState, generate_initial_maze};
use crate::project::{self, StartupProject};
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

/// Marker component for maze tiles (for cleanup)
#[derive(Component)]
pub struct MazeTile;

/// Marker component for track icons
#[derive(Component)]
pub struct TrackIcon {
    pub track_id: usize,
}

/// Marker for track icon point lights
#[derive(Component)]
pub struct TrackLight;

/// Message fired when the maze changes and needs re-rendering
#[derive(Message)]
pub struct MazeChanged;

/// Plugin for maze loading and rendering
pub struct MazePlugin;

impl Plugin for MazePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MazeChanged>()
            .add_systems(Startup, (setup_maze, sync_occluder_grid).chain())
            .add_systems(Update, (respawn_maze_tiles, sync_occluder_grid).chain());
    }
}

/// Colors for different tile types
pub mod colors {
    use bevy::prelude::*;

    pub const FLOOR: Color = Color::srgb(0.15, 0.15, 0.2);
    pub const WALL: Color = Color::srgb(0.4, 0.35, 0.5);
    pub const PLAYER_START: Color = Color::srgb(0.2, 0.3, 0.2);
    pub const TRACK_ICON: Color = Color::srgb(0.8, 0.4, 0.2);
}

#[allow(clippy::needless_pass_by_value)]
fn setup_maze(mut commands: Commands, startup_project: Option<Res<StartupProject>>) {
    if let Some(name) = startup_project.as_ref().and_then(|p| p.0.as_deref()) {
        let path = project::project_dir(name);
        match project::load_project(&path) {
            Ok((manifest, audio_bytes)) => {
                tracing::info!("Loading project '{}' from {}", name, path.display());
                spawn_maze_tiles(&mut commands, &manifest.maze);

                // Set track counter to max id + 1
                let next_id = manifest
                    .tracks
                    .iter()
                    .map(|t| t.track_id + 1)
                    .max()
                    .unwrap_or(0);
                commands.insert_resource(TrackIdCounter(next_id));

                // Store audio bytes for later replay (in load_project_audio)
                let mut track_files = TrackAudioFiles::default();
                for entry in &manifest.tracks {
                    if let Some(bytes) = audio_bytes.get(&entry.track_id) {
                        track_files.files.insert(
                            entry.track_id,
                            TrackAudioFile {
                                original_filename: entry.original_filename.clone(),
                                bytes: bytes.clone(),
                            },
                        );
                    }
                }
                commands.insert_resource(track_files);

                let state = MazeGenState {
                    rooms: manifest.rooms,
                    config: manifest.maze_config,
                };
                commands.insert_resource(manifest.lighting);
                commands.insert_resource(manifest.maze);
                commands.insert_resource(state);
                return;
            }
            Err(e) => {
                tracing::error!("Failed to load project from {}: {e}", path.display());
                tracing::info!("Falling back to fresh maze");
            }
        }
    }

    // Default: generate fresh maze
    let config = MazeGenConfig::default();
    let (maze, state) = generate_initial_maze(&config);
    spawn_maze_tiles(&mut commands, &maze);
    commands.insert_resource(maze);
    commands.insert_resource(state);
}

/// Spawn all tile sprites for the current maze state
pub fn spawn_maze_tiles(commands: &mut Commands, maze: &Maze) {
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

            let mut entity = commands.spawn((
                Sprite {
                    color,
                    custom_size: Some(Vec2::splat(TILE_SIZE - 1.0)),
                    ..default()
                },
                Transform::from_xyz(world_pos.x, world_pos.y, 0.0),
                MazeTile,
            ));

            if tile == TileKind::Wall {
                entity.insert((
                    RigidBody::Static,
                    Collider::rectangle(TILE_SIZE, TILE_SIZE),
                    LightOccluder2d {
                        half_size: Vec2::splat(TILE_SIZE / 2.0),
                    },
                ));
            }

            if tile == TileKind::TrackIcon {
                let track_id = maze.track_ids.get(&(x, y)).copied().unwrap_or(0);
                commands
                    .spawn((
                        Sprite {
                            color: colors::TRACK_ICON,
                            custom_size: Some(Vec2::splat(TILE_SIZE * 0.6)),
                            ..default()
                        },
                        Transform::from_xyz(world_pos.x, world_pos.y, 1.0),
                        TrackIcon { track_id },
                        pos,
                        TrackAudioState::default(),
                    ))
                    .with_child((
                        PointLight2d {
                            color: Color::srgb(0.8, 0.4, 0.2), // Orange glow
                            intensity: 0.4,
                            radius: 100.0,
                            falloff: 0.6,
                        },
                        TrackLight,
                    ));
            }
        }
    }
}

/// Populate the light occluder grid from maze wall data.
#[allow(clippy::needless_pass_by_value)]
fn sync_occluder_grid(maze: Res<Maze>, mut grid: ResMut<LightOccluderGrid>) {
    if grid.width != maze.width || grid.height != maze.height {
        *grid = LightOccluderGrid::new(maze.width, maze.height, TILE_SIZE);
    }
    for y in 0..maze.height {
        for x in 0..maze.width {
            grid.set(x, y, maze.is_wall(x as i32, y as i32));
        }
    }
}

/// On `MazeChanged`, despawn all maze tiles and track icons, then re-render
#[allow(clippy::needless_pass_by_value)]
fn respawn_maze_tiles(
    mut commands: Commands,
    mut events: MessageReader<MazeChanged>,
    tile_query: Query<Entity, With<MazeTile>>,
    icon_query: Query<Entity, With<TrackIcon>>,
    maze: Res<Maze>,
) {
    // Only process if there are MazeChanged events
    let mut changed = false;
    for _event in events.read() {
        changed = true;
    }
    if !changed {
        return;
    }

    // Despawn old tiles (Bevy 0.18 despawn() handles children automatically)
    for entity in &tile_query {
        commands.entity(entity).despawn();
    }
    for entity in &icon_query {
        commands.entity(entity).despawn();
    }

    // Spawn fresh tiles from current maze
    spawn_maze_tiles(&mut commands, &maze);
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
