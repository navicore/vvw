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

use bevy::input::touch::Touches;
use bevy::prelude::*;
use wasm_bindgen::prelude::*;

use vvw_game::{
    Maze, SpatialAudioSet, TrackAudioState, TrackIcon, TrackIdCounter, VvwGamePlugin,
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
    ui::set_build_info();

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
        .init_resource::<CurrentTrackInfo>()
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
                resume_suspended_audio.after(activate_audio_on_click),
                web_audio_sync.after(SpatialAudioSet),
                update_nearest_track_info.after(SpatialAudioSet),
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

/// Resume the `AudioContext` if it was suspended by the browser (e.g. after
/// backgrounding, device sleep, or tab switch). Browsers require a user gesture,
/// so we only call resume when a click or touch is detected.
#[allow(clippy::needless_pass_by_value)]
fn resume_suspended_audio(
    engine: NonSend<WebAudioEngine>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
) {
    if !engine.needs_resume() {
        return;
    }
    let has_gesture =
        mouse.just_pressed(MouseButton::Left) || touches.iter_just_pressed().next().is_some();
    if has_gesture {
        web_sys::console::log_1(&"Resuming suspended AudioContext...".into());
        engine.resume();
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

/// Tracks which `track_id` is currently shown in the info panel.
#[derive(Resource, Default)]
struct CurrentTrackInfo {
    track_id: Option<usize>,
}

/// Show info for the loudest audible track automatically.
/// Updates the foldout whenever the loudest track changes.
#[allow(clippy::needless_pass_by_value)]
fn update_nearest_track_info(
    track_query: Query<(&TrackIcon, &TrackAudioState)>,
    mut current: ResMut<CurrentTrackInfo>,
) {
    // Find the loudest audible track. Hysteresis: a new track must exceed
    // the current one by a margin to prevent rapid switching at equal gains.
    const HYSTERESIS: f32 = 0.05;
    let mut best: Option<(usize, f32)> = None;
    for (icon, state) in &track_query {
        if state.current_gain < 0.01 {
            continue;
        }
        let dominated = if let Some((_, best_gain)) = best {
            let margin = if current.track_id == Some(icon.track_id) {
                0.0
            } else {
                HYSTERESIS
            };
            state.current_gain > best_gain + margin
        } else {
            true
        };
        if dominated {
            best = Some((icon.track_id, state.current_gain));
        }
    }

    let new_id = best.map(|(id, _)| id);
    if new_id != current.track_id {
        current.track_id = new_id;
        if let Some(id) = new_id {
            ui::dispatch_track_select(id);
        } else {
            ui::dispatch_track_hide();
        }
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

    let activated = Arc::new(AtomicBool::new(false));

    let activated_for_closure = Arc::clone(&activated);
    let closure = Closure::<dyn FnMut()>::new(move || {
        // Only fire once
        if activated_for_closure.swap(true, Ordering::Relaxed) {
            return;
        }

        // Hide overlay and show header immediately (visual feedback)
        let _ = ui::hide_overlay();
        ui::show_header();

        // Resume AudioContext synchronously within the user gesture
        if let Err(e) = ctx.resume() {
            web_sys::console::error_1(&format!("audio resume error: {e:?}").into());
        }

        // Focus the canvas so Bevy receives touch and keyboard events
        if let Some(doc) = web_sys::window().and_then(|w| w.document())
            && let Some(canvas) = doc.get_element_by_id("game-canvas")
            && let Ok(html) = canvas.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = html.focus();
        }

        // Signal the Bevy system to activate the audio engine
        flag.store(true, Ordering::Relaxed);
    });

    overlay.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
    closure.forget();

    Ok(())
}
