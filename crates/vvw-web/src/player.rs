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

#[cfg(test)]
mod tests {
    use super::*;
    use vvw_core::maze::Maze;
    use vvw_core::tiles::{TILE_SIZE, TileKind};
    use wasm_bindgen_test::wasm_bindgen_test;

    /// 5x5 maze with walls on borders, floor inside:
    /// WWWWW
    /// W...W
    /// W...W
    /// W...W
    /// WWWWW
    fn test_maze() -> Maze {
        let mut maze = Maze::new_floor(5, 5);
        for x in 0..5 {
            maze.set(x, 0, TileKind::Wall);
            maze.set(x, 4, TileKind::Wall);
        }
        for y in 0..5 {
            maze.set(0, y, TileKind::Wall);
            maze.set(4, y, TileKind::Wall);
        }
        maze
    }

    const NO_INPUT: InputState = InputState {
        up: false,
        down: false,
        left: false,
        right: false,
    };

    fn pressing(direction: &str) -> InputState {
        InputState {
            up: direction.contains("up"),
            down: direction.contains("down"),
            left: direction.contains("left"),
            right: direction.contains("right"),
        }
    }

    fn center_of(tx: i32, ty: i32) -> (f32, f32) {
        (
            (tx as f32).mul_add(TILE_SIZE, TILE_SIZE / 2.0),
            (ty as f32).mul_add(TILE_SIZE, TILE_SIZE / 2.0),
        )
    }

    #[wasm_bindgen_test]
    fn player_starts_at_given_position() {
        let p = Player::new(100.0, 200.0);
        assert!((p.x - 100.0).abs() < f32::EPSILON);
        assert!((p.y - 200.0).abs() < f32::EPSILON);
        assert!((p.vx).abs() < f32::EPSILON);
        assert!((p.vy).abs() < f32::EPSILON);
    }

    #[wasm_bindgen_test]
    fn player_moves_right() {
        let maze = test_maze();
        let (cx, cy) = center_of(2, 2);
        let mut player = Player::new(cx, cy);
        let dt = 1.0 / 60.0;

        player.update(&pressing("right"), &maze, dt);
        assert!(player.x > cx, "player should have moved right");
        assert!(
            (player.y - cy).abs() < 0.01,
            "player should not move vertically"
        );
    }

    #[wasm_bindgen_test]
    fn player_moves_up() {
        let maze = test_maze();
        let (cx, cy) = center_of(2, 2);
        let mut player = Player::new(cx, cy);
        let dt = 1.0 / 60.0;

        player.update(&pressing("up"), &maze, dt);
        assert!(player.y > cy, "player should have moved up");
    }

    #[wasm_bindgen_test]
    fn player_blocked_by_wall() {
        let maze = test_maze();
        // Place player near left wall (wall at x=0, floor at x=1)
        let (cx, cy) = center_of(1, 2);
        let mut player = Player::new(cx, cy);
        let dt = 1.0 / 60.0;

        // Push left into wall for many frames
        for _ in 0..120 {
            player.update(&pressing("left"), &maze, dt);
        }

        // Player circle edge should not penetrate the wall tile
        let wall_right_edge = 1.0 * TILE_SIZE;
        let player_left_edge = player.x - player.radius;
        assert!(
            player_left_edge >= wall_right_edge,
            "player circle should not penetrate wall: left_edge={player_left_edge}, wall_right={wall_right_edge}",
        );
    }

    #[wasm_bindgen_test]
    fn player_slides_along_wall() {
        let maze = test_maze();
        // Start near bottom wall, move diagonally down-right
        let (cx, cy) = center_of(2, 1);
        let mut player = Player::new(cx, cy);
        let dt = 1.0 / 60.0;

        // Move down-right: Y should be blocked by wall, X should still move
        for _ in 0..30 {
            player.update(&pressing("down right"), &maze, dt);
        }

        assert!(
            player.x > cx,
            "player should slide right along wall: x={cx} -> {}",
            player.x
        );

        // Y should be blocked: player circle must not penetrate the bottom wall (y=0)
        let wall_top_edge = 1.0 * TILE_SIZE;
        let player_bottom_edge = player.y - player.radius;
        assert!(
            player_bottom_edge >= wall_top_edge,
            "player circle should not penetrate bottom wall: bottom_edge={player_bottom_edge}, wall_top={wall_top_edge}",
        );
    }

    #[wasm_bindgen_test]
    fn diagonal_movement_normalized() {
        let maze = test_maze();
        let (cx, cy) = center_of(2, 2);
        let dt = 1.0 / 60.0;

        // Move right only
        let mut p_cardinal = Player::new(cx, cy);
        p_cardinal.update(&pressing("right"), &maze, dt);
        let cardinal_dist = (p_cardinal.x - cx).hypot(p_cardinal.y - cy);

        // Move up-right (diagonal)
        let mut p_diagonal = Player::new(cx, cy);
        p_diagonal.update(&pressing("up right"), &maze, dt);
        let diagonal_dist = (p_diagonal.x - cx).hypot(p_diagonal.y - cy);

        // Distances should be approximately equal (normalization prevents faster diagonal)
        let ratio = diagonal_dist / cardinal_dist;
        assert!(
            (ratio - 1.0).abs() < 0.01,
            "diagonal speed should match cardinal: ratio={ratio}"
        );
    }

    #[wasm_bindgen_test]
    fn damping_reduces_velocity() {
        let maze = test_maze();
        let (cx, cy) = center_of(2, 2);
        let mut player = Player::new(cx, cy);
        let dt = 1.0 / 60.0;
        let no_input = NO_INPUT;

        // Give player a push
        player.update(&pressing("right"), &maze, dt);
        let vx_after_push = player.vx;
        assert!(vx_after_push > 0.0);

        // Let it coast with no input
        player.update(&no_input, &maze, dt);
        assert!(
            player.vx < vx_after_push,
            "velocity should decrease with damping: {} -> {}",
            vx_after_push,
            player.vx
        );
    }
}
