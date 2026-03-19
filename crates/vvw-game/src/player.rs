//! Player entity and physics-based movement

use avian2d::prelude::*;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use vvw_light::PointLight2d;

use vvw_core::lighting::{LightMode, LightingConfig};
use vvw_core::physics::PhysicsConfig;

use crate::maze::Maze;
use crate::modes::{ActiveMode, ModeRegistry};
use crate::morph3d::Morph3dActive;
use crate::tiles::{TILE_SIZE, TilePos};

/// Marker component for the player entity
#[derive(Component)]
pub struct Player;

/// Marker for the player's point light (lantern/flashlight)
#[derive(Component)]
pub struct PlayerLight;

/// Direction the player is facing. Used for flashlight cone and sprite rotation.
#[derive(Component)]
pub struct PlayerHeading(pub Vec2);

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
            speed: 600.0, // Force units; terminal velocity ≈ speed / damping
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
            .add_systems(
                Update,
                (handle_player_input, sync_tile_pos, sync_player_light).chain(),
            );
    }
}

/// Player sprite color
const PLAYER_COLOR: Color = Color::srgb(0.2, 0.7, 0.3);

fn spawn_player(
    mut commands: Commands,
    maze: Res<Maze>,
    physics: Res<PhysicsConfig>,
    lighting: Res<LightingConfig>,
) {
    // Find player start position from maze
    let start_pos = maze.find_player_start().unwrap_or(TilePos::new(1, 1));
    let world_pos = start_pos.to_world();

    let player_size = TILE_SIZE * 0.5;
    let is_flashlight = lighting.player_light_mode == LightMode::Flashlight;

    let (light_dir, light_half_cos) = if is_flashlight {
        let half_angle_rad = lighting.flashlight_half_angle.to_radians();
        (Some(Vec2::Y), Some(half_angle_rad.cos()))
    } else {
        (None, None)
    };

    commands
        .spawn((
            Player,
            PlayerHeading(Vec2::Y),
            PlayerMovement {
                tile_pos: start_pos,
                speed: physics.player_speed,
            },
            Sprite {
                color: PLAYER_COLOR,
                custom_size: Some(Vec2::splat(player_size)),
                ..default()
            },
            Transform::from_xyz(world_pos.x, world_pos.y, 2.0), // Above tiles
            PlayerAction::input_map(),
            // Physics components — circle collider slides past wall corners
            RigidBody::Dynamic,
            Collider::circle(player_size / 2.0),
            Friction::new(physics.player_friction),
            Restitution::new(physics.player_restitution),
            LinearDamping(physics.player_linear_damping),
            AngularDamping(physics.player_angular_damping),
        ))
        .with_child((
            PointLight2d {
                color: Color::srgb(1.0, 0.9, 0.6),
                intensity: 0.4,
                radius: 100.0,
                falloff: 0.6,
                direction: light_dir,
                half_angle_cos: light_half_cos,
            },
            PlayerLight,
        ));
}

/// Rotation speed for flashlight mode (radians per second)
pub const ROTATION_SPEED: f32 = 3.0;

pub fn handle_player_input(
    time: Res<Time>,
    lighting: Res<LightingConfig>,
    active_mode: Res<ActiveMode>,
    mode_registry: Res<ModeRegistry>,
    morph_active: Res<Morph3dActive>,
    mut query: Query<
        (
            &ActionState<PlayerAction>,
            &PlayerMovement,
            &mut LinearVelocity,
            &mut PlayerHeading,
        ),
        With<Player>,
    >,
) {
    if let Some(id) = &active_mode.0
        && mode_registry.suppresses_movement(id)
    {
        return;
    }
    let dt = time.delta_secs();
    // 3D mode always uses heading-relative controls (like flashlight)
    let heading_relative = morph_active.0 || lighting.player_light_mode == LightMode::Flashlight;

    for (action_state, movement, mut velocity, mut heading) in &mut query {
        if heading_relative {
            // Flashlight mode: Left/Right rotate, Up/Down move relative to heading
            if action_state.pressed(&PlayerAction::Left) {
                let angle = ROTATION_SPEED * dt;
                heading.0 = Vec2::from_angle(angle).rotate(heading.0);
            }
            if action_state.pressed(&PlayerAction::Right) {
                let angle = -ROTATION_SPEED * dt;
                heading.0 = Vec2::from_angle(angle).rotate(heading.0);
            }
            // Prevent float drift from accumulating over long sessions
            heading.0 = heading.0.normalize_or(Vec2::Y);

            let mut forward = 0.0;
            if action_state.pressed(&PlayerAction::Up) {
                forward += 1.0;
            }
            if action_state.pressed(&PlayerAction::Down) {
                forward -= 1.0;
            }

            velocity.0 += heading.0 * forward * movement.speed * dt;
        } else {
            // Lantern mode: cardinal direction movement (original behavior)
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

            if direction != Vec2::ZERO {
                direction = direction.normalize();
                // Update heading to face movement direction
                heading.0 = direction;
            }

            velocity.0 += direction * movement.speed * dt;
        }
    }
}

/// Keep `tile_pos` in sync with the physics-driven `Transform`
fn sync_tile_pos(mut query: Query<(&Transform, &mut PlayerMovement), With<Player>>) {
    for (transform, mut movement) in &mut query {
        let new_tile_pos = TilePos::from_world(transform.translation.truncate());
        if new_tile_pos != movement.tile_pos {
            movement.tile_pos = new_tile_pos;
        }
    }
}

/// Sync the player's heading into the flashlight direction and sprite rotation.
pub fn sync_player_light(
    player_query: Query<(&PlayerHeading, &Children), With<Player>>,
    mut light_query: Query<&mut PointLight2d, With<PlayerLight>>,
) {
    for (heading, children) in &player_query {
        for child in children.iter() {
            if let Ok(mut light) = light_query.get_mut(child)
                && light.direction.is_some()
            {
                light.direction = Some(heading.0);
            }
        }
    }
}
