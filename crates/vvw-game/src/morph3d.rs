//! 3D morph mode — stage 1: spawn hidden 3D meshes mirroring the 2D tile grid
//!
//! Wall tiles become boxes, floor tiles become flat quads, track icons become
//! smaller cubes. All spawned with `Visibility::Hidden` — the 2D game is unchanged.

use bevy::prelude::*;

use vvw_core::maze::Maze;

use crate::maze::colors;
use crate::tiles::{TILE_SIZE, TileKind};

/// Marker component for 3D tile meshes (walls, floors, track cubes).
#[derive(Component)]
pub struct Mesh3dTile;

/// Wall height in world units.
const WALL_HEIGHT: f32 = TILE_SIZE;

pub struct Morph3dPlugin;

impl Plugin for Morph3dPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_3d_meshes);
    }
}

fn spawn_3d_meshes(
    mut commands: Commands,
    maze: Res<Maze>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Shared meshes
    let wall_mesh = meshes.add(Cuboid::new(TILE_SIZE, WALL_HEIGHT, TILE_SIZE));
    let floor_mesh = meshes.add(Cuboid::new(TILE_SIZE, 0.1, TILE_SIZE));
    let icon_size = TILE_SIZE * 0.6;
    let icon_mesh = meshes.add(Cuboid::new(icon_size, icon_size, icon_size));

    // Shared materials (unlit to keep it simple for now)
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
    let start_mat = materials.add(StandardMaterial {
        base_color: colors::PLAYER_START,
        unlit: true,
        ..default()
    });
    let icon_mat = materials.add(StandardMaterial {
        base_color: colors::TRACK_ICON,
        unlit: true,
        ..default()
    });

    for y in 0..maze.height {
        for x in 0..maze.width {
            let tile = maze.get(x, y).unwrap_or_default();

            // 3D coordinates: X maps to 2D X, Z maps to 2D Y (Bevy 3D uses Y-up)
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
                        start_mat.clone()
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
