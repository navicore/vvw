//! Spatial audio — platform-independent gain/pan interpolation and lighting sync
//!
//! The actual audio backend (kira, Web Audio API, etc.) is provided by the
//! platform layer via [`TrackHandles`]. This module only computes spatial
//! targets and interpolates them smoothly.

use std::collections::HashMap;

use bevy::prelude::*;
use vvw_core::audio::TrackHandle;
use vvw_light::{AmbientLight2d, LightingConfig, PointLight2d};

use crate::maze::{TrackIcon, TrackLight};
use crate::player::{Player, PlayerLight};
use crate::spatial;
use crate::tiles::TilePos;

/// Holds all active track handles, indexed by `track_id`.
/// Uses `Box<dyn TrackHandle>` so the game layer is audio-backend agnostic.
#[derive(Resource, Default)]
pub struct TrackHandles {
    pub handles: HashMap<usize, Box<dyn TrackHandle>>,
}

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
        }
    }
}

/// Spatial audio plugin: gain/pan interpolation and lighting sync.
/// Platform-independent — works with any audio backend that implements [`TrackHandle`].
pub struct SpatialAudioPlugin;

impl Plugin for SpatialAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightingConfig>()
            .init_resource::<TrackHandles>()
            .init_resource::<TrackIdCounter>()
            .add_systems(
                Update,
                (
                    (compute_spatial_targets, interpolate_and_send).chain(),
                    apply_lighting_config,
                ),
            );
    }
}

/// Compute spatial targets (gain, pan, visibility) from player position and maze LOS
#[allow(clippy::needless_pass_by_value)]
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

        if visible {
            let distance = player_pos.distance(*tile_pos);
            state.target_gain = spatial::distance_gain(
                distance,
                spatial::DEFAULT_HALF_DISTANCE,
                spatial::DEFAULT_MAX_DISTANCE,
            );
            let track_world = tile_pos.to_world();
            state.target_pan = spatial::calculate_pan(player_world, track_world);
        } else {
            state.target_gain = 0.0;
        }
    }
}

/// Interpolate current gain/pan toward targets and send to the audio backend.
/// Pauses tracks at zero gain to free audio thread resources;
/// resumes them when they become audible again.
#[allow(clippy::needless_pass_by_value)]
fn interpolate_and_send(
    time: Res<Time>,
    mut handles: ResMut<TrackHandles>,
    mut track_query: Query<(&TrackIcon, &mut TrackAudioState)>,
) {
    let dt = time.delta_secs();

    for (track_icon, mut state) in &mut track_query {
        let was_silent = state.current_gain == 0.0;

        let lerp_factor = (state.fade_speed * dt).min(1.0);
        state.current_gain += (state.target_gain - state.current_gain) * lerp_factor;
        state.current_pan += (state.target_pan - state.current_pan) * lerp_factor;

        if state.current_gain < 0.001 {
            state.current_gain = 0.0;
        }

        if let Some(track) = handles.handles.get_mut(&track_icon.track_id) {
            if state.current_gain == 0.0 {
                // Fully silent — pause to save audio thread work
                if !was_silent {
                    track.set_volume(0.0);
                    track.pause();
                }
            } else {
                // Audible — resume if we were paused, then update volume/pan
                if was_silent {
                    track.resume();
                }
                track.set_volume(state.current_gain);
                track.set_panning(state.current_pan);
            }
        }
    }
}

/// Push `LightingConfig` values to actual light components when config changes.
#[allow(clippy::needless_pass_by_value)]
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
    }
    for mut light in &mut track_lights {
        light.intensity = config.track_intensity;
        light.radius = config.track_radius;
        light.falloff = config.track_falloff;
    }
}
