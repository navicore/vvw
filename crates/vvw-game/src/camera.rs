//! Dead-zone camera that follows the player

use bevy::prelude::*;
use vvw_light::{AmbientLight2d, Lighting2dPlugin};

use crate::player::Player;

/// Marker for the game camera
#[derive(Component)]
pub struct GameCamera;

/// Camera configuration
#[derive(Resource)]
pub struct CameraConfig {
    /// Half-width of the dead zone (camera won't move if player is within this)
    pub dead_zone_half_width: f32,
    /// Half-height of the dead zone
    pub dead_zone_half_height: f32,
    /// Smoothing factor (higher = faster following)
    pub smoothing: f32,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            dead_zone_half_width: 100.0,
            dead_zone_half_height: 75.0,
            smoothing: 5.0,
        }
    }
}

/// Camera plugin — spawns camera and manages following behavior
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Lighting2dPlugin)
            .init_resource::<CameraConfig>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, update_camera);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.1)));
    commands.insert_resource(AmbientLight2d {
        color: Color::srgb(0.1, 0.1, 0.2),
        brightness: 0.15,
    });

    commands.spawn((Camera2d, GameCamera, Transform::default()));
}

#[allow(clippy::needless_pass_by_value)]
fn update_camera(
    time: Res<Time>,
    config: Res<CameraConfig>,
    player_query: Query<&Transform, (With<Player>, Without<GameCamera>)>,
    mut camera_query: Query<&mut Transform, (With<GameCamera>, Without<Player>)>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    let player_pos = player_transform.translation.truncate();
    let camera_pos = camera_transform.translation.truncate();
    let offset = player_pos - camera_pos;

    // Calculate how far outside the dead zone the player is
    let excess_x = (offset.x.abs() - config.dead_zone_half_width).max(0.0) * offset.x.signum();
    let excess_y = (offset.y.abs() - config.dead_zone_half_height).max(0.0) * offset.y.signum();

    if excess_x.abs() > 0.1 || excess_y.abs() > 0.1 {
        let target = camera_pos + Vec2::new(excess_x, excess_y);
        let lerp_factor = (config.smoothing * time.delta_secs()).min(1.0);
        let new_pos = camera_pos.lerp(target, lerp_factor);
        camera_transform.translation.x = new_pos.x;
        camera_transform.translation.y = new_pos.y;
    }
}
