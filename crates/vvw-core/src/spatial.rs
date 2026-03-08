//! Spatial audio calculations: line-of-sight and distance attenuation

use crate::maze::Maze;
use crate::tiles::TilePos;

/// Check if there is a clear line of sight between two tile positions.
/// Uses Bresenham's line algorithm on the tile grid.
/// Start and end tiles are not considered blockers.
pub fn has_line_of_sight(maze: &Maze, from: TilePos, to: TilePos) -> bool {
    if from == to {
        return true;
    }

    let mut x0 = from.x;
    let mut y0 = from.y;
    let x1 = to.x;
    let y1 = to.y;

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        // Check if we've reached the destination
        if x0 == x1 && y0 == y1 {
            return true;
        }

        // Check intermediate tiles (skip the start tile)
        if (x0 != from.x || y0 != from.y) && maze.is_wall(x0, y0) {
            return false;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// Calculate distance-based gain with exponential rolloff.
///
/// - At distance 0: gain = 1.0
/// - At `half_distance`: gain = 0.5
/// - Beyond `max_distance`: gain = 0.0
pub fn distance_gain(distance: f32, half_distance: f32, max_distance: f32) -> f32 {
    if distance >= max_distance {
        return 0.0;
    }
    0.5_f32.powf(distance / half_distance)
}

/// Default half-distance for exponential rolloff (in tiles)
pub const DEFAULT_HALF_DISTANCE: f32 = 5.0;
/// Default max distance beyond which gain is zero (in tiles)
pub const DEFAULT_MAX_DISTANCE: f32 = 15.0;

/// Calculate stereo pan value from player and track positions.
/// Returns a value in [-1.0, 1.0] where -1.0 = full left, 1.0 = full right.
pub fn calculate_pan(player_world: glam::Vec2, track_world: glam::Vec2) -> f32 {
    let diff = track_world - player_world;
    let distance = diff.length();
    if distance < 0.001 {
        return 0.0;
    }
    // dx / distance gives normalized horizontal offset
    (diff.x / distance).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_maze() -> Maze {
        // Simple 5x5 maze:
        // WWWWW
        // W...W
        // W.W.W
        // W...W
        // WWWWW
        let mut maze = Maze::new_floor(5, 5);
        for x in 0..5 {
            maze.set(x, 0, crate::tiles::TileKind::Wall);
            maze.set(x, 4, crate::tiles::TileKind::Wall);
        }
        for y in 0..5 {
            maze.set(0, y, crate::tiles::TileKind::Wall);
            maze.set(4, y, crate::tiles::TileKind::Wall);
        }
        maze.set(2, 2, crate::tiles::TileKind::Wall); // Center wall
        maze
    }

    #[test]
    fn los_open_corridor() {
        let maze = test_maze();
        // (1,1) to (3,1) — open horizontal corridor
        assert!(has_line_of_sight(
            &maze,
            TilePos::new(1, 1),
            TilePos::new(3, 1)
        ));
    }

    #[test]
    fn los_blocked_by_wall() {
        let maze = test_maze();
        // (1,1) to (3,3) — diagonal blocked by center wall at (2,2)
        assert!(!has_line_of_sight(
            &maze,
            TilePos::new(1, 1),
            TilePos::new(3, 3)
        ));
    }

    #[test]
    fn los_adjacent_tiles() {
        let maze = test_maze();
        // Adjacent tiles should always have LOS
        assert!(has_line_of_sight(
            &maze,
            TilePos::new(1, 1),
            TilePos::new(1, 2)
        ));
    }

    #[test]
    fn los_same_tile() {
        let maze = test_maze();
        assert!(has_line_of_sight(
            &maze,
            TilePos::new(1, 1),
            TilePos::new(1, 1)
        ));
    }

    #[test]
    fn distance_gain_at_zero() {
        let gain = distance_gain(0.0, 5.0, 15.0);
        assert!((gain - 1.0).abs() < 0.001);
    }

    #[test]
    fn distance_gain_at_half() {
        let gain = distance_gain(5.0, 5.0, 15.0);
        assert!((gain - 0.5).abs() < 0.001);
    }

    #[test]
    fn distance_gain_beyond_max() {
        let gain = distance_gain(20.0, 5.0, 15.0);
        assert!((gain - 0.0).abs() < 0.001);
    }

    #[test]
    fn pan_track_to_right() {
        let pan = calculate_pan(glam::Vec2::new(0.0, 0.0), glam::Vec2::new(10.0, 0.0));
        assert!((pan - 1.0).abs() < 0.01);
    }

    #[test]
    fn pan_track_to_left() {
        let pan = calculate_pan(glam::Vec2::new(0.0, 0.0), glam::Vec2::new(-10.0, 0.0));
        assert!((pan - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn pan_track_centered() {
        let pan = calculate_pan(glam::Vec2::new(0.0, 0.0), glam::Vec2::new(0.0, 10.0));
        assert!(pan.abs() < 0.01);
    }
}
