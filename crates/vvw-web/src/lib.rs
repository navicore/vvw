//! VVW WASM web player — Bevy app with shared game plugin and Web Audio API
//!
//! Uses `VvwGamePlugin` for platform-independent game logic. Audio playback
//! uses the Web Audio API with `MediaElementAudioSourceNode` for streaming from R2.

// WASM is single-threaded; futures don't need Send
#![allow(clippy::future_not_send)]

mod audio;
mod project;
mod ui;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;
use wasm_bindgen::prelude::*;

use vvw_game::{
    Maze, SpatialAudioSet, TILE_SIZE, TrackAudioState, TrackIcon, TrackIdCounter, VvwGamePlugin,
    spawn_maze_tiles,
};

use audio::WebAudioEngine;

/// Shared flag set by the overlay click handler, read by a Bevy system.
#[derive(Resource)]
struct AudioActivationFlag(Arc<AtomicBool>);

/// WASM entry point — called automatically when the module loads
#[cfg_attr(not(test), wasm_bindgen(start))]
pub fn main() {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"VVW web player initializing...".into());

    wasm_bindgen_futures::spawn_local(async {
        if let Err(e) = run().await {
            web_sys::console::error_1(&format!("VVW error: {e:?}").into());
        }
    });
}

async fn run() -> Result<(), JsValue> {
    // 1. Fetch project manifest and audio base URL
    let loaded = project::load_project().await?;
    web_sys::console::log_1(
        &format!(
            "Loaded project: {} tracks, maze {}x{}",
            loaded.manifest.tracks.len(),
            loaded.manifest.maze.width,
            loaded.manifest.maze.height,
        )
        .into(),
    );

    // 2. Populate album info on the overlay
    ui::populate_album_info(&loaded.manifest.album);

    // 3. Set up Web Audio engine — tracks are registered but NOT connected yet
    let mut engine = WebAudioEngine::new()?;
    let audio_base_url = &loaded.audio_base_url;

    for entry in &loaded.manifest.tracks {
        let url = format!("{audio_base_url}{}.audio", entry.track_id);
        engine.add_track(entry.track_id, &url)?;
        web_sys::console::log_1(
            &format!(
                "Streaming track {} ({})",
                entry.track_id, entry.original_filename
            )
            .into(),
        );
    }

    // Track counter: max id + 1
    let next_id = loaded
        .manifest
        .tracks
        .iter()
        .map(|t| t.track_id.saturating_add(1))
        .max()
        .unwrap_or(0);

    // 4. Set up overlay click handler with shared activation flag.
    // The click resumes AudioContext; a Bevy system picks up the flag
    // and calls engine.activate() to wire tracks + start playback.
    let activation_flag = Arc::new(AtomicBool::new(false));
    let ctx_for_click = engine.ctx();
    setup_overlay_click(ctx_for_click, Arc::clone(&activation_flag))?;

    // 5. Inject track metadata into DOM for the foldout
    ui::inject_track_metadata(&loaded.manifest.tracks);

    // 6. Create and run Bevy app
    let maze = loaded.manifest.maze;
    let lighting = loaded.manifest.lighting;
    let physics = loaded.manifest.physics;

    App::new()
        .insert_resource(maze)
        .insert_resource(lighting)
        .insert_resource(physics)
        .insert_resource(TrackIdCounter(next_id))
        .insert_resource(AudioActivationFlag(activation_flag))
        .insert_non_send_resource(engine)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "VVW Player".into(),
                canvas: Some("#game-canvas".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(VvwGamePlugin)
        .add_systems(Startup, setup_web_maze)
        .add_systems(
            Update,
            (
                activate_audio_on_click,
                web_audio_sync.after(SpatialAudioSet),
                handle_track_clicks.after(SpatialAudioSet),
            ),
        )
        .run();

    Ok(())
}

/// Spawn maze tiles from the pre-loaded `Maze` resource.
#[allow(clippy::needless_pass_by_value)]
fn setup_web_maze(
    mut commands: Commands,
    maze: Res<Maze>,
    lighting: Res<vvw_light::LightingConfig>,
    physics: Res<vvw_core::physics::PhysicsConfig>,
) {
    spawn_maze_tiles(&mut commands, &maze, &lighting, &physics);
}

/// Check the activation flag each frame. When the overlay is clicked,
/// wire up the Web Audio graph and start playback.
#[allow(clippy::needless_pass_by_value)]
fn activate_audio_on_click(flag: Res<AudioActivationFlag>, mut engine: NonSendMut<WebAudioEngine>) {
    if flag.0.swap(false, Ordering::Relaxed) {
        web_sys::console::log_1(&"Activating audio engine...".into());
        if let Err(e) = engine.activate() {
            web_sys::console::error_1(&format!("Audio activation failed: {e:?}").into());
        }
    }
}

/// Sync spatial audio state to the Web Audio API engine each frame.
///
/// Reads the interpolated gain/pan values from `TrackAudioState` (computed by
/// `VvwGamePlugin`'s spatial audio systems) and pushes them to the Web Audio nodes.
#[allow(clippy::needless_pass_by_value)]
fn web_audio_sync(
    engine: NonSend<WebAudioEngine>,
    track_query: Query<(&TrackIcon, &TrackAudioState)>,
) {
    for (track_icon, state) in &track_query {
        engine.set_volume(track_icon.track_id, state.current_gain);
        engine.set_panning(track_icon.track_id, state.current_pan);
    }
}

/// Detect mouse clicks on the canvas and show info for the nearest audible track.
#[allow(clippy::needless_pass_by_value)]
fn handle_track_clicks(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<vvw_game::GameCamera>>,
    track_query: Query<(&TrackIcon, &GlobalTransform, &TrackAudioState)>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, cursor_pos) else {
        return;
    };

    // Find the nearest audible track icon within click range
    let click_radius = TILE_SIZE * 1.5;
    let mut best: Option<(usize, f32)> = None;

    for (icon, global_transform, state) in &track_query {
        if state.current_gain < 0.01 {
            continue; // Skip inaudible tracks
        }
        let dist = world_pos.distance(global_transform.translation().truncate());
        if dist < click_radius && (best.is_none() || dist < best.unwrap().1) {
            best = Some((icon.track_id, dist));
        }
    }

    if let Some((track_id, _)) = best {
        ui::dispatch_track_select(track_id);
    }
}

/// Set up the overlay click handler.
///
/// The click handler resumes the `AudioContext` (required by browser autoplay policy)
/// and sets the activation flag. A Bevy system then calls `engine.activate()` to
/// wire tracks into the Web Audio graph and start playback.
///
/// This two-step approach is needed because Safari throws `NotSupportedError` when
/// `play()` is called on an `<audio>` element already captured by
/// `createMediaElementSource()`. By deferring the capture until after `play()`,
/// both Safari and other browsers work correctly.
fn setup_overlay_click(ctx: web_sys::AudioContext, flag: Arc<AtomicBool>) -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;

    let overlay = document.get_element_by_id("overlay").ok_or("no overlay")?;

    let closure = Closure::once(move || {
        // Hide overlay and show header immediately (visual feedback)
        let _ = ui::hide_overlay();
        ui::show_header();

        // Resume AudioContext synchronously within the user gesture
        if let Err(e) = ctx.resume() {
            web_sys::console::error_1(&format!("audio resume error: {e:?}").into());
        }

        // Signal the Bevy system to activate the audio engine
        flag.store(true, Ordering::Relaxed);
    });

    overlay.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
    closure.forget(); // Leak intentionally — the overlay click only fires once

    Ok(())
}
