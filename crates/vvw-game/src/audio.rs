//! Spatial audio — platform-independent gain/pan interpolation and lighting sync
//!
//! Computes spatial gain/pan targets per frame and interpolates smoothly.
//! Platform layers read `TrackAudioState` after `SpatialAudioSet` to push
//! values to their audio backend (e.g. Web Audio API).

use bevy::prelude::*;
use vvw_light::{AmbientLight2d, LightingConfig, PointLight2d};

use crate::maze::{TrackIcon, TrackLight};
use crate::player::{Player, PlayerLight};
use crate::spatial;
use crate::tiles::TilePos;

/// Counter for track IDs
#[derive(Resource, Default)]
pub struct TrackIdCounter(pub usize);

/// Per-track audio state for smooth interpolation
#[derive(Component)]
pub struct TrackAudioState {
    pub target_gain: f32,
    pub current_gain: f32,
    pub target_pan: f32,
    pub current_pan: f32,
    /// Interpolation speed: 2.0 means full fade in 0.5s
    pub fade_speed: f32,
    pub visible: bool,
    /// Distance from player in tiles (used by platform layer for streaming control)
    pub distance: f32,
}

impl Default for TrackAudioState {
    fn default() -> Self {
        Self {
            target_gain: 0.0,
            current_gain: 0.0,
            target_pan: 0.0,
            current_pan: 0.0,
            fade_speed: 2.0,
            visible: false,
            distance: 0.0,
        }
    }
}

/// System set for spatial audio interpolation.
/// Platform layers can use `.after(SpatialAudioSet)` to read `TrackAudioState`
/// after gain/pan values have been updated for the current frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpatialAudioSet;

/// Spatial audio plugin: gain/pan interpolation and lighting sync.
pub struct SpatialAudioPlugin;

impl Plugin for SpatialAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightingConfig>()
            .init_resource::<TrackIdCounter>()
            .add_systems(
                Update,
                (
                    compute_spatial_targets,
                    interpolate_audio_state,
                    apply_lighting_config,
                )
                    .chain()
                    .in_set(SpatialAudioSet),
            );
    }
}

/// Compute spatial targets (gain, pan, visibility) from player position and maze LOS
fn compute_spatial_targets(
    player_query: Query<&Transform, With<Player>>,
    mut track_query: Query<(&TrackIcon, &TilePos, &mut TrackAudioState)>,
    maze: Res<crate::maze::Maze>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let player_world = player_transform.translation.truncate();
    let player_pos = TilePos::from_world(player_world);

    for (_track_icon, tile_pos, mut state) in &mut track_query {
        let visible = spatial::has_line_of_sight(&maze, player_pos, *tile_pos);
        state.visible = visible;

        // Always update pan and distance so direction is current when LOS resumes
        let track_world = tile_pos.to_world();
        state.target_pan = spatial::calculate_pan(player_world, track_world);

        let distance = player_pos.distance(*tile_pos);
        state.distance = distance;

        if visible {
            state.target_gain = spatial::distance_gain(
                distance,
                spatial::DEFAULT_HALF_DISTANCE,
                spatial::DEFAULT_MAX_DISTANCE,
            );
        } else {
            state.target_gain = 0.0;
        }
    }
}

/// Interpolate current gain/pan toward targets.
/// Platform layers read the resulting `TrackAudioState` after `SpatialAudioSet`.
fn interpolate_audio_state(time: Res<Time>, mut track_query: Query<&mut TrackAudioState>) {
    let dt = time.delta_secs();

    for mut state in &mut track_query {
        let lerp_factor = (state.fade_speed * dt).min(1.0);
        state.current_gain += (state.target_gain - state.current_gain) * lerp_factor;
        state.current_pan += (state.target_pan - state.current_pan) * lerp_factor;

        if state.current_gain < 0.001 {
            state.current_gain = 0.0;
        }
    }
}

/// Push `LightingConfig` values to actual light components when config changes.
fn apply_lighting_config(
    config: Res<LightingConfig>,
    mut ambient: ResMut<AmbientLight2d>,
    mut player_lights: Query<&mut PointLight2d, (With<PlayerLight>, Without<TrackLight>)>,
    mut track_lights: Query<&mut PointLight2d, (With<TrackLight>, Without<PlayerLight>)>,
) {
    if !config.is_changed() {
        return;
    }

    ambient.brightness = config.ambient_brightness;

    for mut light in &mut player_lights {
        light.intensity = config.player_intensity;
        light.radius = config.player_radius;
        light.falloff = config.player_falloff;
        if light.direction.is_some() {
            light.half_angle_cos = Some(config.flashlight_half_angle.to_radians().cos());
        }
    }
    for mut light in &mut track_lights {
        light.intensity = config.track_intensity;
        light.radius = config.track_radius;
        light.falloff = config.track_falloff;
    }
}
