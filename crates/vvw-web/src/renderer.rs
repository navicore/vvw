//! Canvas 2D maze rendering with camera follow

use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use vvw_core::maze::Maze;
use vvw_core::tiles::{TILE_SIZE, TileKind};

use crate::spatial::TrackSpatialState;

/// Colors matching the desktop app
const COLOR_FLOOR: &str = "#262633";
const COLOR_WALL: &str = "#665980";
const COLOR_TRACK: &str = "#CC6633";
const COLOR_PLAYER: &str = "#33B34D";
const COLOR_BACKGROUND: &str = "#0d0d1a";

/// Set up the canvas and return the 2D context
pub fn setup_canvas() -> Result<(HtmlCanvasElement, CanvasRenderingContext2d), JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;

    let canvas: HtmlCanvasElement = document
        .get_element_by_id("game-canvas")
        .ok_or("no game-canvas element")?
        .dyn_into()?;

    let window = web_sys::window().ok_or("no window")?;
    let width = window.inner_width()?.as_f64().unwrap_or(800.0) as u32;
    let height = window.inner_height()?.as_f64().unwrap_or(600.0) as u32;
    canvas.set_width(width);
    canvas.set_height(height);

    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or("no 2d context")?
        .dyn_into()?;

    Ok((canvas, ctx))
}

/// Render one frame: maze tiles + player, with camera centered on the player
#[allow(clippy::similar_names)]
pub fn render(
    ctx: &CanvasRenderingContext2d,
    canvas: &HtmlCanvasElement,
    maze: &Maze,
    player_x: f32,
    player_y: f32,
    tracks: &[TrackSpatialState],
) {
    let cw = f64::from(canvas.width());
    let ch = f64::from(canvas.height());

    // Clear
    ctx.set_fill_style_str(COLOR_BACKGROUND);
    ctx.fill_rect(0.0, 0.0, cw, ch);

    // Camera: center on player
    let cam_x = f64::from(player_x) - cw / 2.0;
    let cam_y = f64::from(player_y) - ch / 2.0;

    let ts = f64::from(TILE_SIZE);

    // Visible tile range (frustum culling)
    let min_tx = ((cam_x / ts).floor() as i32).max(0);
    let min_ty = ((cam_y / ts).floor() as i32).max(0);
    let max_tx = (((cam_x + cw) / ts).ceil() as i32).min(maze.width as i32);
    let max_ty = (((cam_y + ch) / ts).ceil() as i32).min(maze.height as i32);

    // Build a set of track tile positions for fast lookup
    let track_positions: std::collections::HashSet<(i32, i32)> = tracks
        .iter()
        .map(|t| (t.tile_pos.x, t.tile_pos.y))
        .collect();

    // Draw tiles
    for ty in min_ty..max_ty {
        for tx in min_tx..max_tx {
            let tile = maze.get(tx as usize, ty as usize);
            let screen_x = f64::from(tx).mul_add(ts, -cam_x);
            let screen_y = ch - (f64::from(ty).mul_add(ts, -cam_y) + ts); // flip Y: tile Y goes up

            let color = match tile {
                Some(TileKind::Wall) => COLOR_WALL,
                Some(TileKind::TrackIcon) => {
                    if track_positions.contains(&(tx, ty)) {
                        COLOR_TRACK
                    } else {
                        COLOR_FLOOR
                    }
                }
                _ => COLOR_FLOOR,
            };

            ctx.set_fill_style_str(color);
            ctx.fill_rect(screen_x, screen_y, ts, ts);
        }
    }

    // Draw track gain indicators (pulsing glow on track tiles)
    for track in tracks {
        if track.current_gain > 0.001 {
            let screen_x = f64::from(track.tile_pos.x).mul_add(ts, -cam_x);
            let screen_y = ch - (f64::from(track.tile_pos.y).mul_add(ts, -cam_y) + ts);

            let alpha = f64::from(track.current_gain) * 0.5;
            ctx.set_fill_style_str(&format!("rgba(204, 102, 51, {alpha:.2})"));
            ctx.fill_rect(screen_x - 2.0, screen_y - 2.0, ts + 4.0, ts + 4.0);
        }
    }

    // Draw player
    let player_screen_x = f64::from(player_x) - cam_x;
    let player_screen_y = ch - (f64::from(player_y) - cam_y);

    ctx.set_fill_style_str(COLOR_PLAYER);
    ctx.begin_path();
    ctx.arc(
        player_screen_x,
        player_screen_y,
        f64::from(TILE_SIZE * 0.35),
        0.0,
        std::f64::consts::PI * 2.0,
    )
    .ok();
    ctx.fill();
}
