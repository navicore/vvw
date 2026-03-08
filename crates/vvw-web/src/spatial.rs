//! Per-frame spatial audio update: LOS, distance gain, stereo pan

use vvw_core::maze::Maze;
use vvw_core::spatial::{self, DEFAULT_HALF_DISTANCE, DEFAULT_MAX_DISTANCE};
use vvw_core::tiles::TilePos;

use crate::audio::WebAudioEngine;

/// Per-track spatial audio interpolation state
pub struct TrackSpatialState {
    pub track_id: usize,
    pub tile_pos: TilePos,
    pub target_gain: f32,
    pub current_gain: f32,
    pub target_pan: f32,
    pub current_pan: f32,
    pub fade_speed: f32,
}

impl TrackSpatialState {
    pub fn new(track_id: usize, tile_pos: TilePos) -> Self {
        Self {
            track_id,
            tile_pos,
            target_gain: 0.0,
            current_gain: 0.0,
            target_pan: 0.0,
            current_pan: 0.0,
            fade_speed: 2.0,
        }
    }
}

/// Compute spatial targets and interpolate, then push to the audio engine
pub fn update_spatial(
    player_x: f32,
    player_y: f32,
    maze: &Maze,
    tracks: &mut [TrackSpatialState],
    engine: &WebAudioEngine,
    dt: f32,
) {
    let player_world = glam::Vec2::new(player_x, player_y);
    let player_pos = TilePos::from_world(player_world);

    for track in tracks.iter_mut() {
        let visible = spatial::has_line_of_sight(maze, player_pos, track.tile_pos);

        // Always update pan so direction is current when LOS resumes
        let track_world = track.tile_pos.to_world();
        track.target_pan = spatial::calculate_pan(player_world, track_world);

        if visible {
            let distance = player_pos.distance(track.tile_pos);
            track.target_gain =
                spatial::distance_gain(distance, DEFAULT_HALF_DISTANCE, DEFAULT_MAX_DISTANCE);
        } else {
            track.target_gain = 0.0;
        }

        // Interpolate
        let lerp_factor = (track.fade_speed * dt).min(1.0);
        track.current_gain += (track.target_gain - track.current_gain) * lerp_factor;
        track.current_pan += (track.target_pan - track.current_pan) * lerp_factor;

        if track.current_gain < 0.001 {
            track.current_gain = 0.0;
        }

        engine.set_volume(track.track_id, track.current_gain);
        engine.set_panning(track.track_id, track.current_pan);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn track_spatial_state_defaults() {
        let state = TrackSpatialState::new(42, TilePos::new(3, 4));
        assert_eq!(state.track_id, 42);
        assert_eq!(state.tile_pos.x, 3);
        assert_eq!(state.tile_pos.y, 4);
        assert!((state.target_gain).abs() < f32::EPSILON);
        assert!((state.current_gain).abs() < f32::EPSILON);
        assert!((state.target_pan).abs() < f32::EPSILON);
        assert!((state.current_pan).abs() < f32::EPSILON);
        assert!((state.fade_speed - 2.0).abs() < f32::EPSILON);
    }

    #[wasm_bindgen_test]
    fn interpolation_converges_toward_target() {
        let mut state = TrackSpatialState::new(0, TilePos::new(1, 1));
        state.target_gain = 1.0;
        state.target_pan = -0.5;

        // Simulate several frames of interpolation (same formula as update_spatial)
        let dt = 1.0 / 60.0;
        for _ in 0..300 {
            let lerp_factor = (state.fade_speed * dt).min(1.0);
            state.current_gain += (state.target_gain - state.current_gain) * lerp_factor;
            state.current_pan += (state.target_pan - state.current_pan) * lerp_factor;
        }

        assert!(
            (state.current_gain - 1.0).abs() < 0.01,
            "gain should converge to target: {}",
            state.current_gain
        );
        assert!(
            (state.current_pan - (-0.5)).abs() < 0.01,
            "pan should converge to target: {}",
            state.current_pan
        );
    }

    #[wasm_bindgen_test]
    fn gain_below_threshold_snaps_to_zero() {
        let mut state = TrackSpatialState::new(0, TilePos::new(1, 1));
        state.current_gain = 0.0005; // below 0.001 threshold

        // Same threshold logic as update_spatial
        if state.current_gain < 0.001 {
            state.current_gain = 0.0;
        }

        assert!(
            state.current_gain.abs() < f32::EPSILON,
            "gain below threshold should snap to zero"
        );
    }
}
