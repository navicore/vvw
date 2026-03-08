//! VVW WASM web player — browser-based maze exploration with spatial audio

// WASM is single-threaded; futures don't need Send
#![allow(clippy::future_not_send)]

mod audio;
mod game_loop;
mod input;
mod player;
mod project;
mod renderer;
mod spatial;
mod ui;

use wasm_bindgen::prelude::*;

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
    // Fetch project data
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

    // Populate album info on the overlay
    ui::populate_album_info(&loaded.manifest.album);

    // Build game state and decode all audio tracks
    let game = game_loop::Game::build(loaded).await?;

    // Set up overlay click handler to start audio and begin the game loop
    game_loop::start(game)?;

    Ok(())
}
