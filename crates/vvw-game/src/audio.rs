//! Audio integration: kira engine, spatial audio, drag-and-drop loading, egui UI

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::FileDragAndDrop;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use vvw_audio::{GameAudioManager, GameTrack};
use vvw_light::{AmbientLight2d, LightingConfig, PointLight2d};

use crate::maze::{Maze, MazeChanged, TrackIcon, TrackLight};
use crate::mazegen::{self, MazeGenState};
use crate::player::{Player, PlayerLight};
use crate::project;
use crate::spatial;
use crate::tiles::TilePos;

/// Holds all active kira track handles, indexed by `track_id`
#[derive(Resource, Default)]
pub struct TrackHandles {
    handles: HashMap<usize, GameTrack>,
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

/// Raw audio file data retained for project saving
pub struct TrackAudioFile {
    pub original_filename: String,
    pub bytes: Vec<u8>,
}

/// Stores the raw audio bytes for each track, indexed by `track_id`.
/// Used to save projects back to disk.
#[derive(Resource, Default)]
pub struct TrackAudioFiles {
    pub files: HashMap<usize, TrackAudioFile>,
}

/// Message requesting the project be saved (carries the project name)
#[derive(Message)]
pub struct ProjectSaveRequested(pub String);

/// Message requesting a project be loaded (carries the project name)
#[derive(Message)]
pub struct ProjectLoadRequested(pub String);

/// UI state for the project name text field
#[derive(Resource)]
pub struct ProjectNameInput(pub String);

impl Default for ProjectNameInput {
    fn default() -> Self {
        Self("my-maze".to_string())
    }
}

/// Cached list of saved project names to avoid per-frame filesystem I/O
#[derive(Resource)]
struct CachedProjectList {
    names: Vec<String>,
    dirty: bool,
}

impl Default for CachedProjectList {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            dirty: true, // refresh on first frame
        }
    }
}

/// Audio plugin: kira engine + spatial audio + drag-and-drop + egui panel
pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightingConfig>()
            .init_resource::<TrackAudioFiles>()
            .init_resource::<ProjectNameInput>()
            .init_resource::<CachedProjectList>()
            .add_message::<ProjectSaveRequested>()
            .add_message::<ProjectLoadRequested>()
            .add_systems(PostStartup, (setup_audio, load_project_audio).chain())
            .add_systems(
                Update,
                (
                    handle_file_drop,
                    handle_project_save,
                    handle_project_load,
                    (compute_spatial_targets, interpolate_and_send).chain(),
                    apply_lighting_config,
                ),
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
    mut maze: ResMut<Maze>,
    mut state: ResMut<mazegen::MazeGenState>,
    mut maze_changed: MessageWriter<MazeChanged>,
    mut track_audio: ResMut<TrackAudioFiles>,
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

        // Retain a copy for project saving
        let original_filename = path_buf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Add track to kira
        let track = match manager.add_track(audio_bytes.clone()) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to add track: {e}");
                continue;
            }
        };

        let track_id = counter.0;
        counter.0 += 1;

        // Store raw audio for saving
        track_audio.files.insert(
            track_id,
            TrackAudioFile {
                original_filename,
                bytes: audio_bytes,
            },
        );

        // Grow maze to accommodate new track
        let Some(_track_pos) = mazegen::grow_maze(&mut maze, &mut state, track_id) else {
            tracing::error!("Failed to grow maze for new track");
            continue;
        };

        // Store the kira handle
        handles.handles.insert(track_id, track);

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

/// Interpolate current gain/pan toward targets and send to kira.
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

/// Render the audio track panel with `bevy_egui`
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
fn audio_ui_panel(
    mut contexts: EguiContexts,
    track_query: Query<(&TrackIcon, &TrackAudioState)>,
    counter: Res<TrackIdCounter>,
    mut state: ResMut<MazeGenState>,
    mut lighting: ResMut<LightingConfig>,
    mut project_name: ResMut<ProjectNameInput>,
    mut save_events: MessageWriter<ProjectSaveRequested>,
    mut load_events: MessageWriter<ProjectLoadRequested>,
    mut project_list: ResMut<CachedProjectList>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::SidePanel::right("audio_panel")
        .resizable(false)
        .default_width(180.0)
        .show(ctx, |ui| {
            // Project section
            ui.collapsing("Project", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut project_name.0);
                });
                let name_valid = !project_name.0.trim().is_empty();
                ui.add_enabled_ui(name_valid, |ui| {
                    if ui.button("Save").clicked() {
                        save_events.write(ProjectSaveRequested(project_name.0.trim().to_string()));
                    }
                });

                if project_list.dirty {
                    project_list.names = project::list_projects();
                    project_list.dirty = false;
                }
                if !project_list.names.is_empty() {
                    ui.separator();
                    ui.label("Saved projects:");
                    for name in &project_list.names {
                        if ui.button(name).clicked() {
                            load_events.write(ProjectLoadRequested(name.clone()));
                        }
                    }
                }
            });

            ui.heading("Audio Tracks");
            ui.separator();

            if counter.0 == 0 {
                ui.label("Drop a .wav file onto\nthe window to add a track.");
            } else {
                for (track_icon, audio_state) in &track_query {
                    let status = if audio_state.visible {
                        format!(
                            "Track {} {:.0}%",
                            track_icon.track_id,
                            audio_state.current_gain * 100.0
                        )
                    } else {
                        format!("Track {} --", track_icon.track_id)
                    };
                    ui.collapsing(status, |ui| {
                        ui.add(
                            egui::ProgressBar::new(audio_state.current_gain)
                                .desired_width(120.0)
                                .text(format!("{:.0}%", audio_state.current_gain * 100.0)),
                        );
                        if audio_state.visible {
                            ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "visible");
                        } else {
                            ui.colored_label(egui::Color32::from_rgb(200, 80, 80), "occluded");
                        }
                    });
                }
                ui.separator();
                ui.label("Drop more files to\nadd tracks.");
            }

            ui.add_space(12.0);

            ui.collapsing("Maze Settings", |ui| {
                let cfg = &mut state.config;

                ui.label("Room size");
                ui.add(egui::Slider::new(&mut cfg.min_room_size, 2..=10).text("min"));
                ui.add(egui::Slider::new(&mut cfg.max_room_size, 2..=15).text("max"));

                ui.add_space(4.0);
                ui.label("Corridor length");
                ui.add(egui::Slider::new(&mut cfg.min_corridor_length, 1..=10).text("min"));
                ui.add(egui::Slider::new(&mut cfg.max_corridor_length, 2..=15).text("max"));

                ui.add_space(4.0);
                ui.label("Corridor width");
                ui.add(egui::Slider::new(&mut cfg.min_corridor_width, 1..=5).text("min"));
                ui.add(egui::Slider::new(&mut cfg.max_corridor_width, 1..=5).text("max"));

                ui.add_space(4.0);
                ui.label("Room overlap limit");
                ui.add(
                    egui::Slider::new(&mut cfg.max_overlap_fraction, 0.0..=0.5)
                        .text("max %")
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                );
            });

            ui.collapsing("Lighting", |ui| {
                ui.label("Ambient");
                ui.add(
                    egui::Slider::new(&mut lighting.ambient_brightness, 0.0..=1.0)
                        .text("brightness"),
                );

                ui.add_space(4.0);
                ui.label("Player lantern");
                ui.add(
                    egui::Slider::new(&mut lighting.player_intensity, 0.0..=2.0).text("intensity"),
                );
                ui.add(egui::Slider::new(&mut lighting.player_radius, 10.0..=500.0).text("radius"));
                ui.add(egui::Slider::new(&mut lighting.player_falloff, 0.1..=5.0).text("falloff"));

                ui.add_space(4.0);
                ui.label("Track lights");
                ui.add(
                    egui::Slider::new(&mut lighting.track_intensity, 0.0..=2.0).text("intensity"),
                );
                ui.add(egui::Slider::new(&mut lighting.track_radius, 10.0..=500.0).text("radius"));
                ui.add(egui::Slider::new(&mut lighting.track_falloff, 0.1..=5.0).text("falloff"));
            });
        });
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

/// Handle save requests: serialize current state to disk
#[allow(clippy::needless_pass_by_value)]
fn handle_project_save(
    mut events: MessageReader<ProjectSaveRequested>,
    maze: Res<Maze>,
    state: Res<MazeGenState>,
    lighting: Res<LightingConfig>,
    track_audio: Res<TrackAudioFiles>,
    mut project_list: ResMut<CachedProjectList>,
) {
    let mut name = None;
    for event in events.read() {
        name = Some(event.0.clone());
    }
    let Some(project_name) = name else {
        return;
    };

    let save_path = project::project_dir(&project_name);
    match project::save_project(&save_path, &maze, &state, &lighting, &track_audio.files) {
        Ok(()) => {
            tracing::info!("Project '{project_name}' saved to {}", save_path.display());
            project_list.dirty = true;
        }
        Err(e) => tracing::error!("Failed to save project '{project_name}': {e}"),
    }
}

/// Handle load requests at runtime: replace all state from a saved project
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn handle_project_load(
    mut events: MessageReader<ProjectLoadRequested>,
    mut manager: Option<NonSendMut<GameAudioManager>>,
    mut handles: ResMut<TrackHandles>,
    mut counter: ResMut<TrackIdCounter>,
    mut maze: ResMut<Maze>,
    mut state: ResMut<MazeGenState>,
    mut lighting: ResMut<LightingConfig>,
    mut track_audio: ResMut<TrackAudioFiles>,
    mut maze_changed: MessageWriter<MazeChanged>,
    mut player_query: Query<&mut Transform, With<Player>>,
    mut project_name: ResMut<ProjectNameInput>,
    mut project_list: ResMut<CachedProjectList>,
) {
    let mut load_name = None;
    for event in events.read() {
        load_name = Some(event.0.clone());
    }
    let Some(name) = load_name else {
        return;
    };

    let path = project::project_dir(&name);
    let (manifest, mut audio_bytes) = match project::load_project(&path) {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("Failed to load project '{name}': {e}");
            return;
        }
    };

    // Stop all playing tracks
    for track in handles.handles.values_mut() {
        track.stop();
    }
    handles.handles.clear();

    // Clear track audio files
    track_audio.files.clear();

    // Replace maze, gen state, and lighting
    *maze = manifest.maze;
    *state = MazeGenState {
        rooms: manifest.rooms,
        config: manifest.maze_config,
    };
    *lighting = manifest.lighting;

    // Set counter to max track_id + 1
    counter.0 = manifest
        .tracks
        .iter()
        .map(|t| t.track_id + 1)
        .max()
        .unwrap_or(0);

    // Store audio files and replay through kira
    let Some(ref mut manager) = manager else {
        tracing::error!("No audio manager available for loading tracks");
        maze_changed.write(MazeChanged);
        return;
    };

    for entry in &manifest.tracks {
        if let Some(bytes) = audio_bytes.remove(&entry.track_id) {
            // Clone for kira; move the original into storage (avoids double clone)
            let kira_bytes = bytes.clone();
            track_audio.files.insert(
                entry.track_id,
                TrackAudioFile {
                    original_filename: entry.original_filename.clone(),
                    bytes,
                },
            );

            match manager.add_track(kira_bytes) {
                Ok(track) => {
                    handles.handles.insert(entry.track_id, track);
                    tracing::info!(
                        "Loaded track {} ({})",
                        entry.track_id,
                        entry.original_filename
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to load track {} ({}): {e}",
                        entry.track_id,
                        entry.original_filename
                    );
                }
            }
        }
    }

    // Move player to new start position
    if let Some(start) = maze.find_player_start() {
        let world_pos = start.to_world();
        for mut transform in &mut player_query {
            transform.translation.x = world_pos.x;
            transform.translation.y = world_pos.y;
        }
    }

    // Update the name input to match the loaded project
    project_name.0.clone_from(&name);

    project_list.dirty = true;
    maze_changed.write(MazeChanged);
    tracing::info!("Project '{name}' loaded from {}", path.display());
}

/// At `PostStartup`, replay audio from a loaded project.
/// This runs after `setup_audio` so the kira manager is available.
#[allow(clippy::needless_pass_by_value)]
fn load_project_audio(
    mut manager: Option<NonSendMut<GameAudioManager>>,
    mut handles: ResMut<TrackHandles>,
    track_audio: Res<TrackAudioFiles>,
) {
    if track_audio.files.is_empty() || !handles.handles.is_empty() {
        return;
    }

    let Some(ref mut manager) = manager else {
        return;
    };

    let mut entries: Vec<(&usize, &TrackAudioFile)> = track_audio.files.iter().collect();
    entries.sort_by_key(|(id, _)| *id);

    for (track_id, audio_file) in entries {
        match manager.add_track(audio_file.bytes.clone()) {
            Ok(track) => {
                handles.handles.insert(*track_id, track);
                tracing::info!(
                    "Replayed track {} ({}) from loaded project",
                    track_id,
                    audio_file.original_filename
                );
            }
            Err(e) => {
                tracing::error!(
                    "Failed to replay track {} ({}): {e}",
                    track_id,
                    audio_file.original_filename
                );
            }
        }
    }
}
