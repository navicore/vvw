//! Maze rendering and plugin — game-specific systems built on `vvw_core::maze`

use avian2d::prelude::*;
use bevy::prelude::*;
use vvw_light::{LightOccluder2d, LightOccluderGrid, PointLight2d};

pub use vvw_core::maze::Maze;

use crate::audio::{
    AlbumMetadataResource, TrackAudioFile, TrackAudioFiles, TrackAudioState, TrackIdCounter,
};
use crate::mazegen::{MazeGenConfig, MazeGenState, generate_initial_maze};
use crate::project::{self, StartupProject};
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
                    .map(|t| t.track_id.saturating_add(1))
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
                                metadata: entry.metadata.clone(),
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
                commands.insert_resource(AlbumMetadataResource(manifest.album));
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
