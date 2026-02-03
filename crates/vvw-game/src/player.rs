//! Player entity and movement

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

use crate::maze::Maze;
use crate::tiles::{Direction, TilePos, TILE_SIZE};

/// Marker component for the player entity
#[derive(Component)]
pub struct Player;

/// Player movement state
#[derive(Component)]
pub struct PlayerMovement {
    /// Current grid position
    pub tile_pos: TilePos,
    /// Target world position (for smooth animation)
    pub target_world: Vec2,
    /// Whether the player is currently moving
    pub is_moving: bool,
    /// Movement speed in tiles per second
    pub speed: f32,
}

impl Default for PlayerMovement {
    fn default() -> Self {
        Self {
            tile_pos: TilePos::new(0, 0),
            target_world: Vec2::ZERO,
            is_moving: false,
            speed: 8.0, // tiles per second
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
    fn to_direction(self) -> Direction {
        match self {
            Self::Up => Direction::Up,
            Self::Down => Direction::Down,
            Self::Left => Direction::Left,
            Self::Right => Direction::Right,
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
            .add_systems(Update, (handle_player_input, update_player_position).chain());
    }
}

/// Player sprite color
const PLAYER_COLOR: Color = Color::srgb(0.2, 0.7, 0.3);

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters must be passed by value
fn spawn_player(mut commands: Commands, maze: Res<Maze>) {
    // Find player start position from maze
    let start_pos = maze.find_player_start().unwrap_or(TilePos::new(1, 1));
    let world_pos = start_pos.to_world();

    commands.spawn((
        Player,
        PlayerMovement {
            tile_pos: start_pos,
            target_world: world_pos,
            ..default()
        },
        Sprite {
            color: PLAYER_COLOR,
            custom_size: Some(Vec2::splat(TILE_SIZE * 0.8)),
            ..default()
        },
        Transform::from_xyz(world_pos.x, world_pos.y, 2.0), // Above tiles
        PlayerAction::input_map(),
    ));
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters must be passed by value
fn handle_player_input(
    mut query: Query<(&ActionState<PlayerAction>, &mut PlayerMovement), With<Player>>,
    maze: Res<Maze>,
) {
    for (action_state, mut movement) in &mut query {
        // Only accept input when not already moving
        if movement.is_moving {
            continue;
        }

        // Check each direction for input
        for action in [
            PlayerAction::Up,
            PlayerAction::Down,
            PlayerAction::Left,
            PlayerAction::Right,
        ] {
            if action_state.just_pressed(&action) {
                let direction = action.to_direction();
                let target_tile = movement.tile_pos.neighbor(direction);

                // Check if target tile is walkable
                if maze.is_walkable(&target_tile) {
                    movement.tile_pos = target_tile;
                    movement.target_world = target_tile.to_world();
                    movement.is_moving = true;
                    break; // Only process one direction per frame
                }
            }
        }
    }
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters must be passed by value
fn update_player_position(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut PlayerMovement), With<Player>>,
) {
    for (mut transform, mut movement) in &mut query {
        if !movement.is_moving {
            continue;
        }

        let current = transform.translation.truncate();
        let target = movement.target_world;
        let distance = current.distance(target);

        // Movement speed in world units per second
        let speed = movement.speed * TILE_SIZE;
        let step = speed * time.delta_secs();

        if distance <= step {
            // Arrived at target
            transform.translation.x = target.x;
            transform.translation.y = target.y;
            movement.is_moving = false;
        } else {
            // Move toward target
            let direction = (target - current).normalize();
            transform.translation.x += direction.x * step;
            transform.translation.y += direction.y * step;
        }
    }
}
