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
    Maze, MazeTile, SpatialAudioSet, TILE_SIZE, TrackAudioState, TrackIcon, TrackIdCounter,
    VvwGamePlugin, spawn_maze_tiles,
};

use audio::WebAudioEngine;

/// Shared flag set by the overlay click handler, read by a Bevy system.
#[derive(Resource)]
struct AudioActivationFlag(Arc<AtomicBool>);

/// Decoded background image data, ready to be turned into a Bevy sprite.
#[derive(Resource)]
struct BackgroundImageData {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

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
    let audio_base_url = &loaded.audio_base_url;
    ui::populate_album_info(&loaded.manifest.album, audio_base_url);
    ui::set_build_info();

    // 3. Set up Web Audio engine — tracks are registered but NOT connected yet
    let mut engine = WebAudioEngine::new()?;

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
    ui::inject_track_metadata(&loaded.manifest.tracks, audio_base_url);

    // 6. Fetch background image if configured
    let background_data = if let Some(ref bg_url) = loaded.manifest.album.background_url {
        let resolved = ui::resolve_bg_url(bg_url, audio_base_url);
        web_sys::console::log_1(&format!("Loading background: {resolved}").into());
        match fetch_and_decode_image(&resolved).await {
            Ok(data) => {
                web_sys::console::log_1(
                    &format!("Background loaded: {}×{}", data.width, data.height).into(),
                );
                Some(data)
            }
            Err(e) => {
                web_sys::console::error_1(&format!("Background load failed: {e:?}").into());
                None
            }
        }
    } else {
        None
    };

    // 7. Create and run Bevy app
    let maze = loaded.manifest.maze;
    let lighting = loaded.manifest.lighting;
    let physics = loaded.manifest.physics;

    let mut app = App::new();
    app.insert_resource(maze)
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
        .add_systems(
            Update,
            (
                activate_audio_on_click,
                resume_suspended_audio.after(activate_audio_on_click),
                web_audio_sync.after(SpatialAudioSet),
                update_nearest_track_info.after(SpatialAudioSet),
            ),
        );

    if let Some(data) = background_data {
        app.insert_resource(data);
        app.add_systems(
            Startup,
            (setup_web_maze, ApplyDeferred, setup_background).chain(),
        );
    } else {
        app.add_systems(Startup, setup_web_maze);
    }

    app.run();

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

/// Spawn the background image and make maze tile sprites transparent.
/// The background renders at z=-1 (behind tiles), and the lightmap at z=90
/// still modulates brightness over it. Wall colliders and occluders are unchanged.
#[allow(clippy::needless_pass_by_value)]
fn setup_background(
    mut commands: Commands,
    mut bg_data: ResMut<BackgroundImageData>,
    maze: Res<Maze>,
    mut images: ResMut<Assets<Image>>,
    mut tile_query: Query<&mut Sprite, With<MazeTile>>,
) {
    use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    // Create Bevy image from decoded RGBA data (take ownership to avoid cloning)
    let mut image = Image::new(
        Extent3d {
            width: bg_data.width,
            height: bg_data.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        std::mem::take(&mut bg_data.rgba),
        TextureFormat::Rgba8UnormSrgb,
        default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });

    let handle = images.add(image);

    // Size the sprite to cover the full maze
    let maze_width = maze.width as f32 * TILE_SIZE;
    let maze_height = maze.height as f32 * TILE_SIZE;

    commands.spawn((
        Sprite {
            image: handle,
            custom_size: Some(Vec2::new(maze_width, maze_height)),
            ..default()
        },
        Transform::from_xyz(maze_width / 2.0, maze_height / 2.0, -1.0),
    ));

    // Make all maze tile sprites fully transparent
    for mut sprite in &mut tile_query {
        sprite.color = Color::NONE;
    }

    // Remove the now-empty resource (rgba was taken above)
    commands.remove_resource::<BackgroundImageData>();
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
/// so we only call resume when a click, touch, or D-pad press is detected.
#[allow(clippy::needless_pass_by_value)]
fn resume_suspended_audio(
    engine: NonSend<WebAudioEngine>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    interactions: Query<&Interaction>,
) {
    if !engine.needs_resume() {
        return;
    }
    let has_gesture = mouse.just_pressed(MouseButton::Left)
        || touches.iter_just_pressed().next().is_some()
        || interactions.iter().any(|i| *i == Interaction::Pressed);
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

/// Fetch an image URL and decode it to RGBA pixels using the browser's native decoder.
async fn fetch_and_decode_image(url: &str) -> Result<BackgroundImageData, JsValue> {
    use js_sys::Promise;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    // Create an <img> element and load the URL
    let img = web_sys::HtmlImageElement::new()?;
    img.set_cross_origin(Some("anonymous"));

    // Wait for load via a promise
    let load_promise = Promise::new(&mut |resolve, reject| {
        let on_load = Closure::once(move || {
            let _ = resolve.call0(&JsValue::NULL);
        });
        let on_error = Closure::once(move || {
            let _ = reject.call1(&JsValue::NULL, &"image load failed".into());
        });
        img.set_onload(Some(on_load.as_ref().unchecked_ref()));
        img.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_load.forget();
        on_error.forget();
    });

    img.set_src(url);
    JsFuture::from(load_promise).await?;

    let w = img.natural_width();
    let h = img.natural_height();
    if w == 0 || h == 0 {
        return Err("image has zero dimensions".into());
    }

    // Draw to an offscreen canvas to extract RGBA pixels
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;
    let canvas: web_sys::HtmlCanvasElement = document.create_element("canvas")?.dyn_into()?;
    canvas.set_width(w);
    canvas.set_height(h);

    let ctx: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or("no 2d context")?
        .dyn_into()?;

    ctx.draw_image_with_html_image_element(&img, 0.0, 0.0)?;
    let image_data = ctx.get_image_data(0.0, 0.0, f64::from(w), f64::from(h))?;
    let rgba = image_data.data().0;

    Ok(BackgroundImageData {
        rgba,
        width: w,
        height: h,
    })
}
