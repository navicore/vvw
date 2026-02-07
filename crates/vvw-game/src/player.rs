//! Player entity and physics-based movement

use avian2d::prelude::*;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use vvw_light::PointLight2d;

use crate::maze::Maze;
use crate::tiles::{TILE_SIZE, TilePos};

/// Marker component for the player entity
#[derive(Component)]
pub struct Player;

/// Player movement configuration
#[derive(Component)]
pub struct PlayerMovement {
    /// Current tile position (derived from Transform for spatial audio)
    pub tile_pos: TilePos,
    /// Movement speed (impulse magnitude)
    pub speed: f32,
}

impl Default for PlayerMovement {
    fn default() -> Self {
        Self {
            tile_pos: TilePos::new(0, 0),
            speed: 200.0, // Physics impulse units
        }
    }
}

/// Player input actions
#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum PlayerAction {
    Up,
    Down,
    Left,
    Right,
}

impl PlayerAction {
    fn as_vec2(self) -> Vec2 {
        match self {
            Self::Up => Vec2::Y,
            Self::Down => Vec2::NEG_Y,
            Self::Left => Vec2::NEG_X,
            Self::Right => Vec2::X,
        }
    }

    fn input_map() -> InputMap<Self> {
        InputMap::new([
            (Self::Up, KeyCode::KeyW),
            (Self::Up, KeyCode::ArrowUp),
            (Self::Down, KeyCode::KeyS),
            (Self::Down, KeyCode::ArrowDown),
            (Self::Left, KeyCode::KeyA),
            (Self::Left, KeyCode::ArrowLeft),
            (Self::Right, KeyCode::KeyD),
            (Self::Right, KeyCode::ArrowRight),
        ])
    }
}

/// Plugin for player systems
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<PlayerAction>::default())
            .add_systems(PostStartup, spawn_player)
            .add_systems(Update, (handle_player_input, sync_tile_pos).chain());
    }
}

/// Player sprite color
const PLAYER_COLOR: Color = Color::srgb(0.2, 0.7, 0.3);

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters must be passed by value
fn spawn_player(mut commands: Commands, maze: Res<Maze>) {
    // Find player start position from maze
    let start_pos = maze.find_player_start().unwrap_or(TilePos::new(1, 1));
    let world_pos = start_pos.to_world();

    let player_size = TILE_SIZE * 0.8;

    commands
        .spawn((
            Player,
            PlayerMovement {
                tile_pos: start_pos,
                ..default()
            },
            Sprite {
                color: PLAYER_COLOR,
                custom_size: Some(Vec2::splat(player_size)),
                ..default()
            },
            Transform::from_xyz(world_pos.x, world_pos.y, 2.0), // Above tiles
            PlayerAction::input_map(),
            // Physics components
            RigidBody::Dynamic,
            Collider::rectangle(player_size, player_size),
            LockedAxes::ROTATION_LOCKED,
            Friction::new(0.7),
            Restitution::new(0.3), // Slightly bouncy off walls
        ))
        .with_child(PointLight2d {
            color: Color::srgb(1.0, 0.9, 0.6), // Warm lantern
            intensity: 0.4,
            radius: 100.0,
            falloff: 0.6,
        });
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters must be passed by value
fn handle_player_input(
    mut query: Query<
        (
            &ActionState<PlayerAction>,
            &PlayerMovement,
            &mut LinearVelocity,
        ),
        With<Player>,
    >,
) {
    for (action_state, movement, mut velocity) in &mut query {
        let mut direction = Vec2::ZERO;

        for action in [
            PlayerAction::Up,
            PlayerAction::Down,
            PlayerAction::Left,
            PlayerAction::Right,
        ] {
            if action_state.pressed(&action) {
                direction += action.as_vec2();
            }
        }

        // Normalize diagonal movement and apply speed
        if direction != Vec2::ZERO {
            direction = direction.normalize();
        }

        // Set velocity directly for responsive movement with physics collision
        velocity.0 = direction * movement.speed;
    }
}

/// Keep `tile_pos` in sync with the physics-driven `Transform`
#[allow(clippy::needless_pass_by_value)]
fn sync_tile_pos(mut query: Query<(&Transform, &mut PlayerMovement), With<Player>>) {
    for (transform, mut movement) in &mut query {
        let new_tile_pos = TilePos::from_world(transform.translation.truncate());
        if new_tile_pos != movement.tile_pos {
            movement.tile_pos = new_tile_pos;
        }
    }
}
