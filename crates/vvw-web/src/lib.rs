//! VVW WASM web player — Bevy app with shared game plugin and Web Audio API
//!
//! Uses the same `VvwGamePlugin` as the desktop app: avian2d physics,
//! custom 2D lighting, and spatial audio. Audio playback uses the Web Audio API
//! with `MediaElementAudioSourceNode` for streaming from R2.

// WASM is single-threaded; futures don't need Send
#![allow(clippy::future_not_send)]

mod audio;
mod project;
mod ui;

use std::collections::HashMap;

use bevy::prelude::*;
use wasm_bindgen::prelude::*;
use web_sys::HtmlAudioElement;

use vvw_game::{Maze, TrackAudioState, TrackIcon, TrackIdCounter, VvwGamePlugin, spawn_maze_tiles};

use audio::WebAudioEngine;

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

    // 3. Set up Web Audio engine with streaming tracks
    let mut engine = WebAudioEngine::new()?;
    let audio_base_url = &loaded.audio_base_url;

    // Map track_id -> tile_pos from the maze
    let maze = &loaded.manifest.maze;
    let mut track_tile_map: HashMap<usize, vvw_core::tiles::TilePos> = HashMap::new();
    for ((x, y), track_id) in &maze.track_ids {
        track_tile_map.insert(
            *track_id,
            vvw_core::tiles::TilePos::new(*x as i32, *y as i32),
        );
    }

    // Fallback: if track_ids map is empty, assign by order of track icons
    if track_tile_map.is_empty() {
        let track_icons = maze.find_track_icons();
        for (i, entry) in loaded.manifest.tracks.iter().enumerate() {
            if i < track_icons.len() {
                track_tile_map.insert(entry.track_id, track_icons[i]);
            }
        }
    }

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

    // 4. Set up overlay click handler (must use cloned refs — engine moves into Bevy)
    let ctx_for_click = engine.ctx();
    let elements_for_click = engine.audio_elements();
    setup_overlay_click(ctx_for_click, elements_for_click)?;

    // 5. Create and run Bevy app
    let maze = loaded.manifest.maze;
    let lighting = loaded.manifest.lighting;

    App::new()
        .insert_resource(maze)
        .insert_resource(lighting)
        .insert_resource(TrackIdCounter(next_id))
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
        .add_systems(Update, web_audio_sync)
        .run();

    Ok(())
}

/// Spawn maze tiles from the pre-loaded `Maze` resource.
#[allow(clippy::needless_pass_by_value)]
fn setup_web_maze(mut commands: Commands, maze: Res<Maze>) {
    spawn_maze_tiles(&mut commands, &maze);
}

/// Sync spatial audio state to the Web Audio API engine each frame.
///
/// Reads the interpolated gain/pan values from `TrackAudioState` (computed by
/// `VvwGamePlugin`'s spatial audio systems) and pushes them to the Web Audio nodes.
#[allow(clippy::needless_pass_by_value)]
fn web_audio_sync(
    engine: NonSendMut<WebAudioEngine>,
    track_query: Query<(&TrackIcon, &TrackAudioState)>,
) {
    for (track_icon, state) in &track_query {
        engine.set_volume(track_icon.track_id, state.current_gain);
        engine.set_panning(track_icon.track_id, state.current_pan);
    }
}

/// Set up the overlay click handler that starts audio playback.
///
/// `AudioContext.resume()` and `<audio>.play()` must be called synchronously
/// within the user gesture — NOT after an await or in a `spawn_local`.
fn setup_overlay_click(
    ctx: web_sys::AudioContext,
    elements: Vec<HtmlAudioElement>,
) -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;

    let overlay = document.get_element_by_id("overlay").ok_or("no overlay")?;

    let closure = Closure::once(move || {
        // Resume AudioContext synchronously within the click gesture
        if let Err(e) = ctx.resume() {
            web_sys::console::error_1(&format!("audio resume error: {e:?}").into());
        }

        // Start playback on all tracks
        for el in &elements {
            match el.play() {
                Ok(promise) => {
                    let on_err = Closure::once(move |e: JsValue| {
                        web_sys::console::error_1(&format!("track play rejected: {e:?}").into());
                    });
                    let _ = promise.catch(&on_err);
                    on_err.forget();
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("track play() failed: {e:?}").into());
                }
            }
        }

        // Hide the overlay
        let _ = ui::hide_overlay();
    });

    overlay.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
    closure.forget(); // Leak intentionally — the overlay click only fires once

    Ok(())
}
