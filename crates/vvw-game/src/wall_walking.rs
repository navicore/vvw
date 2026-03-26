//! Wall walking — player can jump onto walls and traverse the maze from above
//!
//! When elevated, the player hears all tracks at 20% gain (no wall occlusion).
//! Moving to a non-wall tile drops the player back to the floor.
//! Album opt-in via `wall_walking: bool` in `AlbumMetadata`.
//!
//! Physics approach: collision layers. The player stays `RigidBody::Dynamic`
//! always. Wall/track colliders are on `GameLayer::Floor`. When the player
//! mounts a wall, their collision layer swaps to `GameLayer::Elevated`,
//! making wall colliders invisible to physics. `LinearDamping` still applies.

use avian2d::prelude::*;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

use crate::maze::Maze;
use crate::player::{Player, PlayerAction, PlayerMovement};
use crate::tiles::TilePos;

/// Collision layers for surface-based physics filtering.
///
/// Entities declare which layer they belong to. Only entities on the same
/// layer collide with each other. New surface types = new variants.
#[derive(PhysicsLayer, Clone, Copy, Debug, Default)]
pub enum GameLayer {
    /// Floor level: walls and track icons block movement.
    #[default]
    Floor,
    /// Elevated level: player walks on top of walls, passes through them.
    Elevated,
}

/// Whether the player is elevated (walking on walls).
#[derive(Component, Default, Debug, Clone, Copy)]
pub struct Elevated(pub bool);

/// Resource gating wall-walking availability for this album.
#[derive(Resource, Default)]
pub struct WallWalkingEnabled(pub bool);

/// Fired when the player mounts a wall.
#[derive(Message)]
pub struct PlayerElevated;

/// Fired when the player falls off a wall to the floor.
#[derive(Message)]
pub struct PlayerFell;

/// Message sent by touch systems (swipe-up, double-tap) to request a wall jump.
#[derive(Message)]
pub struct WallJumpRequested;

/// Marker for the drop-shadow sprite shown while elevated.
#[derive(Component)]
struct ElevationShadow;

/// Plugin for wall-walking systems.
pub struct WallWalkingPlugin;

impl Plugin for WallWalkingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WallWalkingEnabled>()
            .add_message::<PlayerElevated>()
            .add_message::<PlayerFell>()
            .add_message::<WallJumpRequested>()
            .add_systems(
                Update,
                (
                    try_mount_wall,
                    clamp_elevated_movement.after(crate::player::sync_tile_pos),
                    swap_collision_layer,
                    ApplyDeferred,
                    update_elevation_visuals,
                )
                    .chain(),
            );
    }
}

/// Mount the nearest adjacent wall on Jump input (spacebar) or mobile gesture.
#[allow(clippy::type_complexity)]
fn try_mount_wall(
    enabled: Res<WallWalkingEnabled>,
    maze: Res<Maze>,
    mut events: MessageWriter<PlayerElevated>,
    mut jump_events: MessageReader<WallJumpRequested>,
    mut player_query: Query<
        (
            &ActionState<PlayerAction>,
            &mut Transform,
            &mut Elevated,
            &mut PlayerMovement,
            &mut LinearVelocity,
        ),
        With<Player>,
    >,
) {
    if !enabled.0 {
        jump_events.clear();
        return;
    }

    let Ok((action_state, mut transform, mut elevated, mut movement, mut velocity)) =
        player_query.single_mut()
    else {
        jump_events.clear();
        return;
    };

    if elevated.0 {
        jump_events.clear();
        return;
    }

    let spacebar = action_state.just_pressed(&PlayerAction::Jump);
    let jump_requested = !jump_events.is_empty();
    if !spacebar && !jump_requested {
        return;
    }

    // Find nearest adjacent wall tile
    let tile = movement.tile_pos;
    let neighbors = [
        TilePos::new(tile.x, tile.y + 1),
        TilePos::new(tile.x, tile.y - 1),
        TilePos::new(tile.x + 1, tile.y),
        TilePos::new(tile.x - 1, tile.y),
    ];

    let player_world = transform.translation.truncate();
    let mut best: Option<(f32, TilePos)> = None;

    for neighbor in neighbors {
        if maze.is_wall(neighbor.x, neighbor.y) {
            let wall_world = neighbor.to_world();
            let dist = player_world.distance(wall_world);
            if best.is_none() || dist < best.unwrap().0 {
                best = Some((dist, neighbor));
            }
        }
    }

    if let Some((_, wall_tile)) = best {
        // Consume jump events only on successful mount
        jump_events.clear();
        let wall_world = wall_tile.to_world();
        transform.translation.x = wall_world.x;
        transform.translation.y = wall_world.y;
        velocity.0 = Vec2::ZERO;
        elevated.0 = true;
        // Sync tile_pos immediately so clamp_elevated_movement sees the wall tile
        movement.tile_pos = wall_tile;
        events.write(PlayerElevated);
    }
}

/// When elevated, if the player's tile is not a wall, they fall to the floor.
fn clamp_elevated_movement(
    maze: Res<Maze>,
    mut events: MessageWriter<PlayerFell>,
    mut player_query: Query<(&PlayerMovement, &mut Elevated), With<Player>>,
) {
    let Ok((movement, mut elevated)) = player_query.single_mut() else {
        return;
    };

    if !elevated.0 {
        return;
    }

    if !maze.is_wall(movement.tile_pos.x, movement.tile_pos.y) {
        elevated.0 = false;
        events.write(PlayerFell);
    }
}

/// Swap player collision layer on mount/fall.
/// `CollisionLayers` is immutable in avian2d — must remove/re-insert.
fn swap_collision_layer(
    mut elevated_events: MessageReader<PlayerElevated>,
    mut fell_events: MessageReader<PlayerFell>,
    mut commands: Commands,
    player_query: Query<Entity, With<Player>>,
) {
    let mounted = elevated_events.read().count() > 0;
    let fell = fell_events.read().count() > 0;

    if !mounted && !fell {
        return;
    }

    let Ok(player_entity) = player_query.single() else {
        return;
    };

    if mounted {
        commands
            .entity(player_entity)
            .remove::<CollisionLayers>()
            .insert(CollisionLayers::new(
                GameLayer::Elevated,
                GameLayer::Elevated,
            ));
    } else if fell {
        commands
            .entity(player_entity)
            .remove::<CollisionLayers>()
            .insert(CollisionLayers::new(GameLayer::Floor, GameLayer::Floor));
    }
}

/// Visual feedback: scale player sprite and show drop shadow when elevated.
fn update_elevation_visuals(
    mut elevated_events: MessageReader<PlayerElevated>,
    mut fell_events: MessageReader<PlayerFell>,
    mut commands: Commands,
    mut player_query: Query<(Entity, &mut Transform), With<Player>>,
    shadow_query: Query<Entity, With<ElevationShadow>>,
) {
    let mounted = elevated_events.read().count() > 0;
    let fell = fell_events.read().count() > 0;

    if !mounted && !fell {
        return;
    }

    let Ok((player_entity, mut transform)) = player_query.single_mut() else {
        return;
    };

    if mounted {
        transform.scale = Vec3::splat(1.15);
        commands.entity(player_entity).with_child((
            ElevationShadow,
            Sprite {
                color: Color::srgba(0.0, 0.0, 0.0, 0.25),
                custom_size: Some(Vec2::splat(crate::tiles::TILE_SIZE * 0.5)),
                ..default()
            },
            Transform::from_xyz(3.0, -3.0, -0.1),
        ));
    } else if fell {
        transform.scale = Vec3::ONE;
        for shadow in &shadow_query {
            commands.entity(shadow).despawn();
        }
    }
}
