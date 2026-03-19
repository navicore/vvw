//! 3D morph mode — hidden 3D geometry, inactive camera, and inactive lights
//!
//! **Stage 1**: Wall boxes, floor quads, track cubes — all `Visibility::Hidden`.
//! **Stage 2**: `Camera3d` (inactive), `PointLight` at tracks, `SpotLight` on player.
//!
//! The 2D game is unchanged. Stage 3 will activate the morph via the mode framework.

use bevy::prelude::*;

use vvw_core::maze::Maze;

use crate::maze::{TrackIcon, colors};
use crate::player::{Player, PlayerHeading};
use crate::tiles::{TILE_SIZE, TileKind, TilePos};

/// Marker component for 3D tile meshes (walls, floors, track cubes).
#[derive(Component)]
pub struct Mesh3dTile;

/// Marker for the 3D camera.
#[derive(Component)]
pub struct GameCamera3d;

/// Marker for the player's 3D spotlight.
#[derive(Component)]
pub struct PlayerSpotlight3d;

/// Marker for 3D point lights at track positions.
#[derive(Component)]
pub struct TrackLight3d;

/// Whether the 3D morph is currently active. Systems that update the 3D camera
/// and lights check this resource. Default false — the 3D view is dormant.
#[derive(Resource, Default)]
pub struct Morph3dActive(pub bool);

/// Wall height in world units.
const WALL_HEIGHT: f32 = TILE_SIZE;

/// Camera eye height (fraction of tile size).
const EYE_HEIGHT: f32 = TILE_SIZE * 0.4;

pub struct Morph3dPlugin;

impl Plugin for Morph3dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Morph3dActive>()
            .add_systems(PostStartup, (setup_3d_meshes, setup_3d_camera_and_lights))
            .add_systems(
                Update,
                follow_player_3d.run_if(|active: Res<Morph3dActive>| active.0),
            );
    }
}

// ── Startup ────────────────────────────────────────────────────────────────

fn setup_3d_meshes(
    mut commands: Commands,
    maze: Res<Maze>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_3d_meshes_from_maze(&mut commands, &maze, &mut meshes, &mut materials);
}

fn setup_3d_camera_and_lights(
    mut commands: Commands,
    maze: Res<Maze>,
    track_query: Query<(&TrackIcon, &TilePos)>,
) {
    // Find player start for initial camera position
    let start_pos = maze.find_player_start().unwrap_or(TilePos::new(1, 1));
    let world = start_pos.to_world();
    let cam_pos = Vec3::new(world.x, EYE_HEIGHT, -world.y);

    // Spawn inactive Camera3d — low order so Camera2d remains primary
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: -1,
            is_active: false,
            ..default()
        },
        Transform::from_translation(cam_pos).looking_to(Vec3::NEG_Z, Vec3::Y),
        GameCamera3d,
    ));

    // Spawn inactive point lights at track icon positions
    for (_, tile_pos) in &track_query {
        let tw = tile_pos.to_world();
        commands.spawn((
            PointLight {
                color: Color::srgb(0.8, 0.4, 0.2),
                intensity: 800.0,
                range: TILE_SIZE * 4.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(tw.x, WALL_HEIGHT * 0.8, -tw.y),
            Visibility::Hidden,
            TrackLight3d,
        ));
    }

    // Spawn inactive spotlight as a standalone entity (follows player in update)
    commands.spawn((
        SpotLight {
            color: Color::srgb(1.0, 0.9, 0.6),
            intensity: 1500.0,
            range: TILE_SIZE * 5.0,
            outer_angle: 0.8,
            inner_angle: 0.5,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(cam_pos).looking_to(Vec3::NEG_Z, Vec3::Y),
        Visibility::Hidden,
        PlayerSpotlight3d,
    ));
}

// ── Query filters ──────────────────────────────────────────────────────────

type CamFilter = (
    With<GameCamera3d>,
    Without<Player>,
    Without<PlayerSpotlight3d>,
);
type SpotFilter = (
    With<PlayerSpotlight3d>,
    Without<Player>,
    Without<GameCamera3d>,
);

// ── Update ─────────────────────────────────────────────────────────────────

/// Keep the 3D camera and spotlight in sync with the player's 2D position
/// and heading. Only runs when `Morph3dActive` is true.
fn follow_player_3d(
    player_query: Query<(&Transform, &PlayerHeading), With<Player>>,
    mut cam_query: Query<&mut Transform, CamFilter>,
    mut spot_query: Query<&mut Transform, SpotFilter>,
) {
    let Ok((player_tf, heading)) = player_query.single() else {
        return;
    };

    let player_2d = player_tf.translation.truncate();
    let pos_3d = Vec3::new(player_2d.x, EYE_HEIGHT, -player_2d.y);

    // Convert 2D heading (X, Y) to 3D look direction (X, 0, -Y)
    let look_dir = Vec3::new(heading.0.x, 0.0, -heading.0.y).normalize_or(Vec3::NEG_Z);

    if let Ok(mut cam_tf) = cam_query.single_mut() {
        *cam_tf = Transform::from_translation(pos_3d).looking_to(look_dir, Vec3::Y);
    }

    if let Ok(mut spot_tf) = spot_query.single_mut() {
        *spot_tf = Transform::from_translation(pos_3d).looking_to(look_dir, Vec3::Y);
    }
}

// ── 3D mesh spawning ───────────────────────────────────────────────────────

/// Spawn hidden 3D meshes for the current maze state.
///
/// Called at startup and on `MazeChanged` (via `respawn_maze_tiles`).
pub fn spawn_3d_meshes_from_maze(
    commands: &mut Commands,
    maze: &Maze,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    // Shared meshes
    let wall_mesh = meshes.add(Cuboid::new(TILE_SIZE, WALL_HEIGHT, TILE_SIZE));
    let floor_mesh = meshes.add(Cuboid::new(TILE_SIZE, 0.1, TILE_SIZE));
    let icon_size = TILE_SIZE * 0.6;
    let icon_mesh = meshes.add(Cuboid::new(icon_size, icon_size, icon_size));

    // Shared materials (unlit while 3D lights are inactive)
    let wall_mat = materials.add(StandardMaterial {
        base_color: colors::WALL,
        unlit: true,
        ..default()
    });
    let floor_mat = materials.add(StandardMaterial {
        base_color: colors::FLOOR,
        unlit: true,
        ..default()
    });
    let icon_mat = materials.add(StandardMaterial {
        base_color: colors::TRACK_ICON,
        unlit: true,
        ..default()
    });

    // Deferred — only allocated if maze contains PlayerStart tiles
    let mut start_mat = None;

    for y in 0..maze.height {
        for x in 0..maze.width {
            let tile = maze.get(x, y).unwrap_or_default();

            // 3D coordinates: X maps to 2D X, Z maps to -2D Y (Bevy 3D is Y-up, -Z forward)
            let world_x = (x as f32).mul_add(TILE_SIZE, TILE_SIZE / 2.0);
            let world_z = -(y as f32).mul_add(TILE_SIZE, TILE_SIZE / 2.0);

            match tile {
                TileKind::Wall => {
                    commands.spawn((
                        Mesh3d(wall_mesh.clone()),
                        MeshMaterial3d(wall_mat.clone()),
                        Transform::from_xyz(world_x, WALL_HEIGHT / 2.0, world_z),
                        Visibility::Hidden,
                        Mesh3dTile,
                    ));
                }
                TileKind::Floor | TileKind::PlayerStart => {
                    let mat = if tile == TileKind::PlayerStart {
                        start_mat
                            .get_or_insert_with(|| {
                                materials.add(StandardMaterial {
                                    base_color: colors::PLAYER_START,
                                    unlit: true,
                                    ..default()
                                })
                            })
                            .clone()
                    } else {
                        floor_mat.clone()
                    };
                    commands.spawn((
                        Mesh3d(floor_mesh.clone()),
                        MeshMaterial3d(mat),
                        Transform::from_xyz(world_x, 0.0, world_z),
                        Visibility::Hidden,
                        Mesh3dTile,
                    ));
                }
                TileKind::TrackIcon => {
                    // Floor under the track icon
                    commands.spawn((
                        Mesh3d(floor_mesh.clone()),
                        MeshMaterial3d(floor_mat.clone()),
                        Transform::from_xyz(world_x, 0.0, world_z),
                        Visibility::Hidden,
                        Mesh3dTile,
                    ));
                    // Track cube
                    commands.spawn((
                        Mesh3d(icon_mesh.clone()),
                        MeshMaterial3d(icon_mat.clone()),
                        Transform::from_xyz(world_x, icon_size / 2.0, world_z),
                        Visibility::Hidden,
                        Mesh3dTile,
                    ));
                }
            }
        }
    }
}
