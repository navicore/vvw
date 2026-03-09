//! Maze rendering and plugin — game-specific systems built on `vvw_core::maze`
//!
//! The platform layer (desktop app or web player) is responsible for loading
//! the maze and inserting it as a `Maze` resource before the game runs.
//! This module handles rendering, collision, lighting, and respawning.

use avian2d::prelude::*;
use bevy::prelude::*;
use vvw_light::{LightOccluder2d, LightOccluderGrid, PointLight2d};

pub use vvw_core::maze::Maze;

use crate::audio::TrackAudioState;
use crate::tiles::{TILE_SIZE, TileKind, TilePos};

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

/// Plugin for maze rendering.
///
/// Expects a `Maze` resource to be inserted by the platform layer before
/// `PostStartup`. The plugin handles the occluder grid sync and runtime
/// respawning when the maze changes.
pub struct MazePlugin;

impl Plugin for MazePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MazeChanged>()
            .add_systems(PostStartup, sync_occluder_grid)
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

/// Spawn all tile sprites for the current maze state.
///
/// Call this from your platform's startup system after inserting the `Maze` resource.
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
                    Friction::new(0.5),
                    Restitution::new(0.4),
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
/// Only runs when the `Maze` resource has changed, avoiding O(W*H) per frame.
#[allow(clippy::needless_pass_by_value)]
fn sync_occluder_grid(maze: Res<Maze>, mut grid: ResMut<LightOccluderGrid>) {
    if !maze.is_changed() {
        return;
    }
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
