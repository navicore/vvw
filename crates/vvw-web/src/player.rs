//! Player state: position, velocity, collision with maze walls

use vvw_core::maze::Maze;
use vvw_core::tiles::TILE_SIZE;

use crate::input::InputState;

/// Player physics state for free movement with AABB wall collision
pub struct Player {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub speed: f32,
    pub damping: f32,
    pub radius: f32,
}

impl Player {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            speed: 1000.0,
            damping: 5.0,
            radius: TILE_SIZE * 0.4,
        }
    }

    /// Update player position from input, applying velocity, damping, and collision
    pub fn update(&mut self, input: &InputState, maze: &Maze, dt: f32) {
        // Apply input to velocity
        let mut dx = 0.0_f32;
        let mut dy = 0.0_f32;
        if input.up {
            dy += 1.0;
        }
        if input.down {
            dy -= 1.0;
        }
        if input.left {
            dx -= 1.0;
        }
        if input.right {
            dx += 1.0;
        }

        // Normalize diagonal movement
        let len = dx.hypot(dy);
        if len > 0.0 {
            dx /= len;
            dy /= len;
        }

        self.vx += dx * self.speed * dt;
        self.vy += dy * self.speed * dt;

        // Apply damping
        let damp = self.damping.mul_add(-dt, 1.0).max(0.0);
        self.vx *= damp;
        self.vy *= damp;

        // Axis-separated collision: try X first, then Y
        let new_x = self.vx.mul_add(dt, self.x);
        if self.collides_at(new_x, self.y, maze) {
            self.vx = 0.0;
        } else {
            self.x = new_x;
        }

        let new_y = self.vy.mul_add(dt, self.y);
        if self.collides_at(self.x, new_y, maze) {
            self.vy = 0.0;
        } else {
            self.y = new_y;
        }
    }

    /// Check if the player circle at (px, py) overlaps any wall tile
    #[allow(clippy::similar_names)]
    fn collides_at(&self, px: f32, py: f32, maze: &Maze) -> bool {
        // Compute the tile range the player AABB covers
        let min_tx = ((px - self.radius) / TILE_SIZE).floor() as i32;
        let max_tx = ((px + self.radius) / TILE_SIZE).floor() as i32;
        let min_ty = ((py - self.radius) / TILE_SIZE).floor() as i32;
        let max_ty = ((py + self.radius) / TILE_SIZE).floor() as i32;

        for ty in min_ty..=max_ty {
            for tx in min_tx..=max_tx {
                if maze.is_wall(tx, ty) {
                    // AABB overlap test: tile rect vs player circle (simplified as AABB)
                    let tile_left = tx as f32 * TILE_SIZE;
                    let tile_bottom = ty as f32 * TILE_SIZE;
                    let tile_right = tile_left + TILE_SIZE;
                    let tile_top = tile_bottom + TILE_SIZE;

                    let closest_x = px.clamp(tile_left, tile_right);
                    let closest_y = py.clamp(tile_bottom, tile_top);

                    let dist_x = px - closest_x;
                    let dist_y = py - closest_y;

                    if dist_x.mul_add(dist_x, dist_y * dist_y) < self.radius * self.radius {
                        return true;
                    }
                }
            }
        }
        false
    }
}
