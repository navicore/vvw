//! Audio integration: kira engine, spatial audio, drag-and-drop loading, egui UI

use bevy::prelude::*;
use bevy::window::FileDragAndDrop;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use vvw_audio::{GameAudioManager, GameTrack};

use crate::maze::{MazeChanged, TrackIcon};
use crate::mazegen;
use crate::player::Player;
use crate::spatial;
use crate::tiles::TilePos;

/// Holds all active kira track handles, indexed by `track_id`
#[derive(Resource, Default)]
pub struct TrackHandles {
    handles: Vec<Option<GameTrack>>,
}

/// Counter for track IDs
#[derive(Resource)]
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

/// Audio plugin: kira engine + spatial audio + drag-and-drop + egui panel
pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, setup_audio)
            .add_systems(
                Update,
                (
                    handle_file_drop,
                    compute_spatial_targets,
                    interpolate_and_send,
                )
                    .chain(),
            )
            .add_systems(EguiPrimaryContextPass, audio_ui_panel);
    }
}

/// Initialize the kira audio manager
fn setup_audio(world: &mut World) {
    match GameAudioManager::new() {
        Ok(manager) => {
            tracing::info!("Kira audio manager started");
            world.insert_non_send_resource(manager);
            world.insert_resource(TrackHandles::default());
            world.insert_resource(TrackIdCounter(0));
        }
        Err(e) => {
            tracing::error!("Failed to start audio manager: {e}");
            // Insert resources anyway so systems don't panic
            world.insert_resource(TrackHandles::default());
            world.insert_resource(TrackIdCounter(0));
        }
    }
}

/// Handle file drag-and-drop: load audio, grow maze, create track
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn handle_file_drop(
    mut drop_events: MessageReader<FileDragAndDrop>,
    mut manager: Option<NonSendMut<GameAudioManager>>,
    mut handles: ResMut<TrackHandles>,
    mut counter: ResMut<TrackIdCounter>,
    mut maze: ResMut<crate::maze::Maze>,
    mut state: ResMut<mazegen::MazeGenState>,
    mut maze_changed: MessageWriter<MazeChanged>,
) {
    let Some(ref mut manager) = manager else {
        return;
    };

    let mut any_added = false;

    for event in drop_events.read() {
        let FileDragAndDrop::DroppedFile { path_buf, .. } = event else {
            continue;
        };

        // Only accept audio files
        let extension = path_buf
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "wav" | "mp3" | "ogg" | "flac") {
            tracing::warn!("Ignoring non-audio file: {}", path_buf.display());
            continue;
        }

        // Read file bytes
        let Ok(audio_bytes) = std::fs::read(path_buf) else {
            tracing::error!("Failed to read file: {}", path_buf.display());
            continue;
        };

        // Add track to kira
        let track = match manager.add_track(audio_bytes) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to add track: {e}");
                continue;
            }
        };

        let track_id = counter.0;
        counter.0 += 1;

        // Grow maze to accommodate new track
        let Some(_track_pos) = mazegen::grow_maze(&mut maze, &mut state, track_id) else {
            tracing::error!("Failed to grow maze for new track");
            continue;
        };

        // Store the kira handle
        while handles.handles.len() <= track_id {
            handles.handles.push(None);
        }
        handles.handles[track_id] = Some(track);

        tracing::info!("Added track {track_id} from {}", path_buf.display(),);
        any_added = true;
    }

    // Signal a single respawn after all drops are processed
    if any_added {
        maze_changed.write(MazeChanged);
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

/// Interpolate current gain/pan toward targets and send to kira
#[allow(clippy::needless_pass_by_value)]
fn interpolate_and_send(
    time: Res<Time>,
    mut handles: ResMut<TrackHandles>,
    mut track_query: Query<(&TrackIcon, &mut TrackAudioState)>,
) {
    let dt = time.delta_secs();

    for (track_icon, mut state) in &mut track_query {
        let lerp_factor = (state.fade_speed * dt).min(1.0);

        state.current_gain += (state.target_gain - state.current_gain) * lerp_factor;
        state.current_pan += (state.target_pan - state.current_pan) * lerp_factor;

        if state.current_gain < 0.001 {
            state.current_gain = 0.0;
        }

        if let Some(Some(track)) = handles.handles.get_mut(track_icon.track_id) {
            track.set_volume(state.current_gain);
            track.set_panning(state.current_pan);
        }
    }
}

/// Render the audio track panel with `bevy_egui`
#[allow(clippy::needless_pass_by_value)]
fn audio_ui_panel(
    mut contexts: EguiContexts,
    track_query: Query<(&TrackIcon, &TrackAudioState)>,
    counter: Res<TrackIdCounter>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::SidePanel::right("audio_panel")
        .resizable(false)
        .default_width(180.0)
        .show(ctx, |ui| {
            ui.heading("Audio Tracks");
            ui.separator();

            if counter.0 == 0 {
                ui.label("Drop a .wav file onto\nthe window to add a track.");
            } else {
                for (track_icon, state) in &track_query {
                    ui.group(|ui| {
                        ui.label(format!("Track {}", track_icon.track_id));
                        ui.add(
                            egui::ProgressBar::new(state.current_gain)
                                .desired_width(120.0)
                                .text(format!("{:.0}%", state.current_gain * 100.0)),
                        );
                        if state.visible {
                            ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "visible");
                        } else {
                            ui.colored_label(egui::Color32::from_rgb(200, 80, 80), "occluded");
                        }
                    });
                }
                ui.separator();
                ui.label("Drop more files to\nadd tracks.");
            }
        });
}
