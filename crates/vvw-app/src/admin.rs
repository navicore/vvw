//! Desktop admin plugin: egui UI, file drag-and-drop, project persistence, kira audio
//!
//! This plugin adds all desktop-specific functionality on top of `VvwGamePlugin`:
//! - Kira audio engine setup and track loading
//! - File drag-and-drop for adding audio tracks
//! - egui settings panel (project management, maze settings, lighting, track metadata)
//! - Project save/load to filesystem
//! - Maze regeneration

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::FileDragAndDrop;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use vvw_audio::GameAudioManager;
use vvw_core::project::{AlbumMetadata, TrackMetadata};
use vvw_light::LightingConfig;

use vvw_game::{
    Maze, MazeChanged, Player, TrackAudioState, TrackHandles, TrackIcon, TrackIdCounter,
    mazegen::{self, MazeGenConfig, MazeGenState, generate_initial_maze},
    spawn_maze_tiles,
};

use crate::project;
use crate::project::StartupProject;

/// Raw audio file data retained for project saving
pub struct TrackAudioFile {
    pub original_filename: String,
    pub bytes: Vec<u8>,
    pub metadata: TrackMetadata,
}

/// Album-level metadata, stored as a Bevy resource
#[derive(Resource, Default)]
pub struct AlbumMetadataResource(pub AlbumMetadata);

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

/// Whether the right-side settings panel is expanded or collapsed
#[derive(Resource)]
pub struct UiPanelOpen(pub bool);

impl Default for UiPanelOpen {
    fn default() -> Self {
        Self(true)
    }
}

/// Message requesting a fresh random maze layout using current settings
#[derive(Message)]
pub struct MazeRegenRequested;

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

/// Desktop admin plugin: kira audio, egui UI, file drops, project persistence
pub struct AdminPlugin;

impl Plugin for AdminPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<TrackAudioFiles>()
            .init_resource::<AlbumMetadataResource>()
            .init_resource::<ProjectNameInput>()
            .init_resource::<CachedProjectList>()
            .init_resource::<UiPanelOpen>()
            .add_message::<ProjectSaveRequested>()
            .add_message::<ProjectLoadRequested>()
            .add_message::<MazeRegenRequested>()
            .add_systems(Startup, admin_setup_maze)
            .add_systems(PostStartup, (setup_audio, load_project_audio).chain())
            .add_systems(
                Update,
                (
                    handle_file_drop,
                    handle_project_save,
                    handle_project_load,
                    handle_maze_regen,
                    refresh_project_list,
                ),
            )
            .add_systems(EguiPrimaryContextPass, audio_ui_panel);
    }
}

/// Load a project from disk or generate a fresh maze, then spawn tiles.
#[allow(clippy::needless_pass_by_value)]
fn admin_setup_maze(mut commands: Commands, startup_project: Option<Res<StartupProject>>) {
    if let Some(name) = startup_project.as_ref().and_then(|p| p.0.as_deref()) {
        let path = project::project_dir(name);
        match project::load_project(&path) {
            Ok((manifest, audio_bytes)) => {
                tracing::info!("Loading project '{}' from {}", name, path.display());
                spawn_maze_tiles(&mut commands, &manifest.maze);

                // Set track counter to max id + 1
                let next_id = manifest
                    .tracks
                    .iter()
                    .map(|t| t.track_id.saturating_add(1))
                    .max()
                    .unwrap_or(0);
                commands.insert_resource(TrackIdCounter(next_id));

                // Store audio bytes for later replay (in load_project_audio)
                let mut track_files = TrackAudioFiles::default();
                for entry in &manifest.tracks {
                    if let Some(bytes) = audio_bytes.get(&entry.track_id) {
                        track_files.files.insert(
                            entry.track_id,
                            TrackAudioFile {
                                original_filename: entry.original_filename.clone(),
                                bytes: bytes.clone(),
                                metadata: entry.metadata.clone(),
                            },
                        );
                    }
                }
                commands.insert_resource(track_files);

                let state = MazeGenState {
                    rooms: manifest.rooms,
                    config: manifest.maze_config,
                };
                commands.insert_resource(manifest.lighting);
                commands.insert_resource(AlbumMetadataResource(manifest.album));
                commands.insert_resource(manifest.maze);
                commands.insert_resource(state);
                return;
            }
            Err(e) => {
                tracing::error!("Failed to load project from {}: {e}", path.display());
                tracing::info!("Falling back to fresh maze");
            }
        }
    }

    // Default: generate fresh maze
    let config = MazeGenConfig::default();
    let (maze, state) = generate_initial_maze(&config);
    spawn_maze_tiles(&mut commands, &maze);
    commands.insert_resource(maze);
    commands.insert_resource(state);
}

/// Initialize the kira audio manager
fn setup_audio(world: &mut World) {
    match GameAudioManager::new() {
        Ok(manager) => {
            tracing::info!("Kira audio manager started");
            world.insert_non_send_resource(manager);
        }
        Err(e) => {
            tracing::error!("Failed to start audio manager: {e}");
        }
    }
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
                handles.handles.insert(*track_id, Box::new(track));
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

/// Handle file drag-and-drop: load audio, grow maze, create track
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn handle_file_drop(
    mut drop_events: MessageReader<FileDragAndDrop>,
    mut manager: Option<NonSendMut<GameAudioManager>>,
    mut handles: ResMut<TrackHandles>,
    mut counter: ResMut<TrackIdCounter>,
    mut maze: ResMut<Maze>,
    mut state: ResMut<MazeGenState>,
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

        // Read file bytes and reject oversized files
        let Ok(audio_bytes) = std::fs::read(path_buf) else {
            tracing::error!("Failed to read file: {}", path_buf.display());
            continue;
        };
        if audio_bytes.len() > 500_000_000 {
            tracing::warn!(
                "Ignoring oversized file ({} bytes): {}",
                audio_bytes.len(),
                path_buf.display()
            );
            continue;
        }

        // Retain a copy for project saving
        let original_filename = path_buf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let track_id = counter.0;

        // Grow maze BEFORE allocating audio resources — if the maze can't
        // accommodate the track, we skip without leaking a kira handle.
        let Some(_track_pos) = mazegen::grow_maze(&mut maze, &mut state, track_id) else {
            tracing::error!("Failed to grow maze for new track");
            continue;
        };

        // Advance counter immediately — the maze room exists for this ID now,
        // so the next drop must use a fresh ID even if add_track fails below.
        counter.0 += 1;

        // Maze was mutated — tiles must be re-rendered even if add_track fails
        any_added = true;

        // Add track to kira (only after maze growth succeeded)
        let track = match manager.add_track(audio_bytes.clone()) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to add track: {e}");
                continue;
            }
        };

        // Store raw audio for saving
        track_audio.files.insert(
            track_id,
            TrackAudioFile {
                original_filename,
                bytes: audio_bytes,
                metadata: TrackMetadata::default(),
            },
        );

        // Store the kira handle (boxed as dyn TrackHandle)
        handles.handles.insert(track_id, Box::new(track));

        tracing::info!("Added track {track_id} from {}", path_buf.display(),);
    }

    // Signal a single respawn after all drops are processed
    if any_added {
        maze_changed.write(MazeChanged);
    }
}

/// Regenerate the maze layout from scratch using current settings and existing tracks
#[allow(clippy::needless_pass_by_value)]
fn handle_maze_regen(
    mut events: MessageReader<MazeRegenRequested>,
    mut maze: ResMut<Maze>,
    mut state: ResMut<MazeGenState>,
    track_audio: Res<TrackAudioFiles>,
    mut handles: ResMut<TrackHandles>,
    mut maze_changed: MessageWriter<MazeChanged>,
    mut player_query: Query<&mut Transform, With<Player>>,
) {
    let mut any = false;
    for _ in events.read() {
        any = true;
    }
    if !any {
        return;
    }

    // Collect and sort track IDs so the maze is built deterministically per set
    let mut track_ids: Vec<usize> = track_audio.files.keys().copied().collect();
    track_ids.sort_unstable();

    // Fresh maze with one starting room, using current slider settings
    let (new_maze, new_state) = generate_initial_maze(&state.config);
    *maze = new_maze;
    state.rooms = new_state.rooms;

    // Grow a room + corridor for each track; stop orphaned handles on failure
    for &track_id in &track_ids {
        if mazegen::grow_maze(&mut maze, &mut state, track_id).is_none() {
            tracing::warn!("Maze regen could not place track {track_id}; stopping its handle");
            if let Some(mut handle) = handles.handles.remove(&track_id) {
                handle.stop();
            }
        }
    }

    // Reposition player to the new start
    if let Some(start) = maze.find_player_start() {
        let world_pos = start.to_world();
        for mut transform in &mut player_query {
            transform.translation.x = world_pos.x;
            transform.translation.y = world_pos.y;
        }
    }

    maze_changed.write(MazeChanged);
    tracing::info!("Maze regenerated with {} tracks", track_ids.len());
}

/// Handle save requests: serialize current state to disk
#[allow(clippy::needless_pass_by_value)]
fn handle_project_save(
    mut events: MessageReader<ProjectSaveRequested>,
    maze: Res<Maze>,
    state: Res<MazeGenState>,
    lighting: Res<LightingConfig>,
    track_audio: Res<TrackAudioFiles>,
    album_meta: Res<AlbumMetadataResource>,
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
    match project::save_project(
        &save_path,
        &maze,
        &state,
        &lighting,
        &track_audio.files,
        &album_meta.0,
    ) {
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
    mut album_meta: ResMut<AlbumMetadataResource>,
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
    *album_meta = AlbumMetadataResource(manifest.album.clone());

    // Set counter to max track_id + 1
    counter.0 = manifest
        .tracks
        .iter()
        .map(|t| t.track_id.saturating_add(1))
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
            match manager.add_track(bytes.clone()) {
                Ok(track) => {
                    track_audio.files.insert(
                        entry.track_id,
                        TrackAudioFile {
                            original_filename: entry.original_filename.clone(),
                            bytes,
                            metadata: entry.metadata.clone(),
                        },
                    );
                    handles.handles.insert(entry.track_id, Box::new(track));
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

/// Refresh the cached project list in Update (not in the render pass) to avoid
/// blocking the frame with filesystem I/O.
fn refresh_project_list(mut project_list: ResMut<CachedProjectList>) {
    if project_list.dirty {
        project_list.names = project::list_projects();
        project_list.dirty = false;
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
    project_list: Res<CachedProjectList>,
    mut panel_open: ResMut<UiPanelOpen>,
    mut regen_events: MessageWriter<MazeRegenRequested>,
    mut album_meta: ResMut<AlbumMetadataResource>,
    mut track_audio: ResMut<TrackAudioFiles>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // When panel is collapsed, show a small "<<" button to reopen it
    if !panel_open.0 {
        egui::Area::new(egui::Id::new("panel_toggle"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-4.0, 4.0))
            .show(ctx, |ui| {
                if ui.button("<<").clicked() {
                    panel_open.0 = true;
                }
            });
        return;
    }

    egui::SidePanel::right("audio_panel")
        .resizable(false)
        .default_width(180.0)
        .show(ctx, |ui| {
            // Hide button at the top
            ui.horizontal(|ui| {
                if ui.button(">>").clicked() {
                    panel_open.0 = false;
                }
            });
            ui.separator();

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

            ui.collapsing("Album Info", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Title:");
                    ui.text_edit_singleline(&mut album_meta.0.title);
                });
                ui.horizontal(|ui| {
                    ui.label("Artist:");
                    ui.text_edit_singleline(&mut album_meta.0.artist);
                });
                ui.label("Description:");
                ui.text_edit_multiline(&mut album_meta.0.description);
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

                        if let Some(file) = track_audio.files.get_mut(&track_icon.track_id) {
                            ui.horizontal(|ui| {
                                ui.label("Title:");
                                ui.text_edit_singleline(&mut file.metadata.title);
                            });
                            ui.horizontal(|ui| {
                                ui.label("Artist:");
                                ui.text_edit_singleline(&mut file.metadata.artist);
                            });
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

                ui.add_space(8.0);
                ui.add_enabled_ui(counter.0 > 0, |ui| {
                    if ui.button("Regenerate Maze").clicked() {
                        regen_events.write(MazeRegenRequested);
                    }
                });
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
