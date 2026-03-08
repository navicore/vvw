//! requestAnimationFrame game loop wiring everything together

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use vvw_core::maze::Maze;
use vvw_core::tiles::TilePos;

use crate::audio::WebAudioEngine;
use crate::input::{self, InputState};
use crate::player::Player;
use crate::project::LoadedProject;
use crate::renderer;
use crate::spatial::TrackSpatialState;
use crate::ui;

/// All game state bundled together
pub struct Game {
    pub maze: Maze,
    pub player: Player,
    pub engine: WebAudioEngine,
    pub tracks: Vec<TrackSpatialState>,
    pub track_positions: std::collections::HashSet<(i32, i32)>,
    pub canvas: HtmlCanvasElement,
    pub ctx: web_sys::CanvasRenderingContext2d,
    pub input: Rc<RefCell<InputState>>,
    pub last_time: f64,
    pub started: bool,
    // Keep closures alive so event listeners aren't dropped
    _input_closures: Vec<Closure<dyn FnMut(web_sys::KeyboardEvent)>>,
}

impl Game {
    /// Build the game from a loaded project, setting up streaming audio tracks
    pub fn build(loaded: LoadedProject) -> Result<Self, JsValue> {
        let maze = loaded.manifest.maze;
        let audio_base_url = &loaded.audio_base_url;

        // Find player start
        let start = maze
            .find_player_start()
            .unwrap_or_else(|| TilePos::new(2, 2));
        let world_pos = start.to_world();
        let player = Player::new(world_pos.x, world_pos.y);

        // Set up audio engine
        let mut engine = WebAudioEngine::new()?;

        // Map track_id -> tile_pos from the maze's track_ids
        let mut track_tile_map: HashMap<usize, TilePos> = HashMap::new();
        for ((x, y), track_id) in &maze.track_ids {
            track_tile_map.insert(*track_id, TilePos::new(*x as i32, *y as i32));
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

        // Set up streaming audio tracks (no download — browser streams on play)
        let mut tracks = Vec::new();
        for entry in &loaded.manifest.tracks {
            let Some(tile_pos) = track_tile_map.get(&entry.track_id) else {
                web_sys::console::warn_1(
                    &format!("No tile position for track {}", entry.track_id).into(),
                );
                continue;
            };

            let url = format!("{audio_base_url}{}.audio", entry.track_id);
            engine.add_track(entry.track_id, &url)?;
            tracks.push(TrackSpatialState::new(entry.track_id, *tile_pos));
            web_sys::console::log_1(
                &format!(
                    "Streaming track {} ({}) at ({},{})",
                    entry.track_id, entry.original_filename, tile_pos.x, tile_pos.y
                )
                .into(),
            );
        }

        // Pre-build track position set for rendering (avoids per-frame allocation)
        let track_positions: std::collections::HashSet<(i32, i32)> = tracks
            .iter()
            .map(|t| (t.tile_pos.x, t.tile_pos.y))
            .collect();

        // Set up canvas
        let (canvas, ctx) = renderer::setup_canvas()?;

        // Set up input
        let (input, input_closures) = input::setup_input()?;

        Ok(Self {
            maze,
            player,
            engine,
            tracks,
            track_positions,
            canvas,
            ctx,
            input,
            last_time: 0.0,
            started: false,
            _input_closures: input_closures,
        })
    }
}

/// Start the game: set up overlay click, begin rAF loop
pub fn start(game: Game) -> Result<(), JsValue> {
    let game = Rc::new(RefCell::new(game));

    // Do an initial render so the maze is visible behind the overlay
    {
        let g = game.borrow();
        renderer::render(
            &g.ctx,
            &g.canvas,
            &g.maze,
            g.player.x,
            g.player.y,
            &g.tracks,
            &g.track_positions,
        );
    }

    setup_overlay_click(Rc::clone(&game))?;

    Ok(())
}

fn setup_overlay_click(game: Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;

    let overlay = document.get_element_by_id("overlay").ok_or("no overlay")?;

    let closure = Closure::once(move || {
        // Resume AudioContext and start playback synchronously within the click
        // gesture so the browser permits autoplay.
        {
            let g = game.borrow();
            if let Err(e) = g.engine.resume() {
                web_sys::console::error_1(&format!("audio resume error: {e:?}").into());
            }
            g.engine.play_all();
        }

        let _ = ui::hide_overlay();

        {
            let mut g = game.borrow_mut();
            g.started = true;
        }

        // Start the animation loop
        start_animation_loop(game);
    });

    overlay.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
    closure.forget(); // Leak intentionally — the overlay click only fires once

    Ok(())
}

fn start_animation_loop(game: Rc<RefCell<Game>>) {
    // rAF loop using the standard Rc<RefCell<Option<Closure>>> pattern
    #[allow(clippy::type_complexity)]
    let f: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
    let g = Rc::clone(&f);

    let window = web_sys::window().expect("no window");

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move |timestamp: f64| {
        {
            let mut game = game.borrow_mut();
            if game.last_time == 0.0 {
                game.last_time = timestamp;
            }
            let dt = ((timestamp - game.last_time) / 1000.0) as f32;
            game.last_time = timestamp;

            // Cap dt to prevent physics explosions after tab-switch
            let dt = dt.min(0.05);

            // Copy input state to avoid borrow conflicts
            let (up, down, left, right) = {
                let input = game.input.borrow();
                (input.up, input.down, input.left, input.right)
            };
            let input_snap = crate::input::InputState {
                up,
                down,
                left,
                right,
            };

            // Destructure to get simultaneous mutable + immutable field borrows
            let Game {
                ref maze,
                ref mut player,
                ref engine,
                ref mut tracks,
                ref track_positions,
                ref ctx,
                ref canvas,
                ..
            } = *game;

            // Update player
            player.update(&input_snap, maze, dt);

            // Update spatial audio
            crate::spatial::update_spatial(player.x, player.y, maze, tracks, engine, dt);

            // Render
            renderer::render(
                ctx,
                canvas,
                maze,
                player.x,
                player.y,
                tracks,
                track_positions,
            );
        }

        // Schedule next frame
        let window = web_sys::window().expect("no window");
        window
            .request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .expect("rAF failed");
    }) as Box<dyn FnMut(f64)>));

    window
        .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .expect("rAF failed");
}
