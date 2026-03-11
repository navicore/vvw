//! Procedural maze generation: rooms connected by L-shaped corridors

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::maze::Maze;
use crate::tiles::{TileKind, TilePos};

/// Configuration for maze generation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy-ecs", derive(bevy::prelude::Resource))]
pub struct MazeGenConfig {
    pub min_room_size: usize,
    pub max_room_size: usize,
    pub min_corridor_length: usize,
    pub max_corridor_length: usize,
    pub min_corridor_width: usize,
    pub max_corridor_width: usize,
    /// Maximum allowed overlap fraction (0.0–1.0) between a new room and any
    /// existing room. Lower values spread rooms further apart. Proposals
    /// exceeding this threshold are rejected and retried.
    pub max_overlap_fraction: f32,
    /// Minimum tile distance between new room center and any existing track.
    /// Proposals closer than this are rejected and retried.
    #[serde(default)]
    pub min_track_distance: usize,
    /// Probability (0.0–1.0) that a corridor uses an L-bend to block LOS.
    /// The rest are straight, allowing sound to travel between rooms.
    #[serde(default = "default_l_bend_chance")]
    pub l_bend_chance: f32,
}

fn default_l_bend_chance() -> f32 {
    0.35
}

impl Default for MazeGenConfig {
    fn default() -> Self {
        Self {
            min_room_size: 3,
            max_room_size: 7,
            min_corridor_length: 4,
            max_corridor_length: 8,
            min_corridor_width: 1,
            max_corridor_width: 3,
            max_overlap_fraction: 0.2,
            min_track_distance: 0,
            l_bend_chance: 0.35,
        }
    }
}

/// Return a `MazeGenConfig` tuned for the given number of tracks.
///
/// More tracks → longer corridors, tighter overlap, and minimum track spacing
/// to ensure rooms are spread apart and LOS between tracks is blocked.
pub fn config_for_track_count(n: usize) -> MazeGenConfig {
    // Each L-corridor splits its total length into two legs, so the per-leg
    // distance is roughly half these values. Corridors need to be long enough
    // that the bend plus walls block LOS between rooms, but short enough to
    // keep the maze compact (large mazes cause lightmap performance issues and
    // isolate tracks too much — neighbouring tracks should be mixable).
    match n {
        0..=4 => MazeGenConfig {
            min_corridor_length: 8,
            max_corridor_length: 12,
            min_room_size: 3,
            max_room_size: 5,
            min_corridor_width: 1,
            max_corridor_width: 1,
            max_overlap_fraction: 0.0,
            min_track_distance: 10,
            l_bend_chance: 0.3,
        },
        5..=8 => MazeGenConfig {
            min_corridor_length: 10,
            max_corridor_length: 14,
            min_room_size: 3,
            max_room_size: 5,
            min_corridor_width: 1,
            max_corridor_width: 1,
            max_overlap_fraction: 0.0,
            min_track_distance: 12,
            l_bend_chance: 0.35,
        },
        _ => MazeGenConfig {
            min_corridor_length: 10,
            max_corridor_length: 16,
            min_room_size: 3,
            max_room_size: 5,
            min_corridor_width: 1,
            max_corridor_width: 1,
            max_overlap_fraction: 0.0,
            min_track_distance: 14,
            l_bend_chance: 0.35,
        },
    }
}

/// A rectangular room in the maze
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Room {
    pub fn center(&self) -> TilePos {
        TilePos::new(
            (self.x + self.width / 2) as i32,
            (self.y + self.height / 2) as i32,
        )
    }
}

/// Persistent state for incremental maze generation
#[cfg_attr(feature = "bevy-ecs", derive(bevy::prelude::Resource))]
pub struct MazeGenState {
    pub rooms: Vec<Room>,
    pub config: MazeGenConfig,
}

/// Generate the initial maze with a single starting room
pub fn generate_initial_maze(config: &MazeGenConfig) -> (Maze, MazeGenState) {
    let mut rng = rand::thread_rng();

    let room_w = rng.gen_range(config.min_room_size..=config.max_room_size);
    let room_h = rng.gen_range(config.min_room_size..=config.max_room_size);

    // Maze = room + 2-tile wall border on each side
    let maze_w = room_w + 4;
    let maze_h = room_h + 4;
    let mut maze = Maze::new(maze_w, maze_h); // All walls

    // Carve the room
    let room = Room {
        x: 2,
        y: 2,
        width: room_w,
        height: room_h,
    };
    carve_room(&mut maze, &room);

    // Place player start at room center
    let center = room.center();
    maze.set(center.x as usize, center.y as usize, TileKind::PlayerStart);

    let state = MazeGenState {
        rooms: vec![room],
        config: config.clone(),
    };

    (maze, state)
}

/// Grow the maze by extending an L-shaped corridor from a random existing room
/// and carving a new room at the end.
///
/// `track_id` is stored in the maze's `track_ids` map for stable ID assignment.
/// Returns the position where the track icon was actually placed.
///
/// Attempts multiple placements to avoid overlapping existing rooms and to
/// maintain minimum distance between tracks. Falls back to the best available
/// proposal after exhausting attempts.
#[allow(clippy::too_many_lines)]
pub fn grow_maze(maze: &mut Maze, state: &mut MazeGenState, track_id: usize) -> Option<TilePos> {
    const MAX_ATTEMPTS: usize = 60;

    let mut rng = rand::thread_rng();

    if state.rooms.is_empty() {
        return None;
    }

    // Collect existing track positions for distance checks
    let existing_tracks: Vec<TilePos> = maze
        .track_ids
        .keys()
        .map(|&(x, y)| TilePos::new(x as i32, y as i32))
        .collect();

    let threshold = state.config.max_overlap_fraction;
    let min_dist = state.config.min_track_distance as f32;

    // Try multiple placements. Track the best fallback by severity:
    // - Grade 0: perfect (no overlap, distance OK) — accept immediately
    // - Grade 1: no overlap but too close to a track
    // - Grade 2: overlapping rooms
    let mut best: Option<(Proposal, u8)> = None;

    for _ in 0..MAX_ATTEMPTS {
        let candidate = propose_room(&mut rng, state);

        // Check room overlap
        let max_overlap = state
            .rooms
            .iter()
            .map(|r| overlap_fraction(r, &candidate.room))
            .fold(0.0_f32, f32::max);
        let too_much_overlap = max_overlap > threshold;

        if too_much_overlap {
            // Grade 2 — only keep if no better fallback exists
            if best.as_ref().is_none_or(|(_, g)| *g > 1) {
                best = Some((candidate, 2));
            }
            continue;
        }

        // Check track distance
        if min_dist > 0.0 {
            let center = candidate.room.center();
            let too_close = existing_tracks
                .iter()
                .any(|t| center.distance(*t) < min_dist);
            if too_close {
                // Grade 1 — better than overlap
                if best.as_ref().is_none_or(|(_, g)| *g > 0) {
                    best = Some((candidate, 1));
                }
                continue;
            }
        }

        // Grade 0 — perfect, use immediately
        best = Some((candidate, 0));
        break;
    }

    let (proposal, _grade) = best?;

    // Ensure maze is large enough — expand if needed
    let needed_right =
        proposal.room.x.max(proposal.bend_x) + proposal.room.width + 2 + proposal.corridor_width;
    let needed_top =
        proposal.room.y.max(proposal.bend_y) + proposal.room.height + 2 + proposal.corridor_width;
    let expand_right = needed_right.saturating_sub(maze.width);
    let expand_top = needed_top.saturating_sub(maze.height);

    if expand_right > 0 || expand_top > 0 {
        let (dx, dy) = maze.expand(0, expand_right, 0, expand_top);
        for room in &mut state.rooms {
            room.x = (room.x as i32 + dx) as usize;
            room.y = (room.y as i32 + dy) as usize;
        }
    }

    // Carve the L-shaped corridor (two segments meeting at the bend point)
    carve_l_corridor(maze, &proposal);

    // Carve the new room
    carve_room(maze, &proposal.room);

    // Place track icon at room center, or nearby if center is already taken
    let center = proposal.room.center();
    let mut ix = center.x as usize;
    let mut iy = center.y as usize;
    if maze.track_ids.contains_key(&(ix, iy)) {
        'search: for dy in 0..proposal.room.height {
            for dx in 0..proposal.room.width {
                let ax = proposal.room.x + dx;
                let ay = proposal.room.y + dy;
                if !maze.track_ids.contains_key(&(ax, ay)) {
                    ix = ax;
                    iy = ay;
                    break 'search;
                }
            }
        }
    }
    maze.set(ix, iy, TileKind::TrackIcon);
    maze.track_ids.insert((ix, iy), track_id);

    // Restore any tiles that were overwritten by corridor/room carving
    maze.restore_track_icons();
    maze.restore_player_start(&state.rooms[0]);

    state.rooms.push(proposal.room);

    Some(TilePos::new(ix as i32, iy as i32))
}

/// A proposed room placement with an L-shaped corridor.
struct Proposal {
    /// Start of the corridor (at the source room edge)
    start_x: usize,
    start_y: usize,
    /// The bend point where the corridor turns 90 degrees
    bend_x: usize,
    bend_y: usize,
    corridor_width: usize,
    /// Whether the exit direction is vertical (up/down). Controls carving order:
    /// horizontal exits carve horizontal-first, vertical exits carve vertical-first.
    vertical_exit: bool,
    room: Room,
}

/// Generate a candidate room placement with an L-shaped corridor.
///
/// The corridor exits the source room in one direction, runs for a random
/// length, bends 90 degrees, then runs to the new room. This ensures walls
/// exist at the bend point, blocking line-of-sight between rooms.
fn propose_room(rng: &mut impl Rng, state: &MazeGenState) -> Proposal {
    let source_idx = rng.gen_range(0..state.rooms.len());
    let source = &state.rooms[source_idx];

    let direction: u8 = rng.gen_range(0..4);
    let corridor_len =
        rng.gen_range(state.config.min_corridor_length..=state.config.max_corridor_length);
    let corridor_width =
        rng.gen_range(state.config.min_corridor_width..=state.config.max_corridor_width);
    let new_room_w = rng.gen_range(state.config.min_room_size..=state.config.max_room_size);
    let new_room_h = rng.gen_range(state.config.min_room_size..=state.config.max_room_size);

    // Decide whether this corridor bends (L-shape) or runs straight.
    // Straight corridors preserve LOS between rooms, enabling audio mixing.
    let use_bend = rng.gen_bool(f64::from(state.config.l_bend_chance));

    let (leg1, leg2) = if use_bend {
        // Split corridor length into two legs for the L-shape.
        // Minimum 2 tiles per leg so the bend is meaningful.
        let min_leg = 2_usize;
        let l1 = if corridor_len > min_leg * 2 {
            rng.gen_range(min_leg..=(corridor_len - min_leg))
        } else {
            corridor_len / 2
        };
        (l1, corridor_len - l1)
    } else {
        // Straight: all length in leg1, no perpendicular offset
        (corridor_len, 0)
    };

    // Randomly choose which perpendicular direction for the second leg
    let bend_sign: i32 = if rng.gen_bool(0.5) { 1 } else { -1 };

    let (start_x, start_y, bend_x, bend_y, room_x, room_y) = match direction {
        0 => {
            // Exit right, then bend up/down
            let sx = source.x + source.width;
            let sy = source.y + source.height / 2;
            let bx = sx + leg1;
            let by = (sy as i32 + bend_sign * leg2 as i32).max(0) as usize;
            let rx = bx;
            let ry = if bend_sign > 0 {
                by
            } else {
                by.saturating_sub(new_room_h / 2)
            };
            (sx, sy, bx, by, rx, ry)
        }
        1 => {
            // Exit up, then bend left/right
            let sx = source.x + source.width / 2;
            let sy = source.y + source.height;
            let bx = (sx as i32 + bend_sign * leg2 as i32).max(0) as usize;
            let by = sy + leg1;
            let rx = if bend_sign > 0 {
                bx
            } else {
                bx.saturating_sub(new_room_w / 2)
            };
            let ry = by;
            (sx, sy, bx, by, rx, ry)
        }
        2 => {
            // Exit left, then bend up/down
            let sx = source.x;
            let sy = source.y + source.height / 2;
            let bx = sx.saturating_sub(leg1);
            let by = (sy as i32 + bend_sign * leg2 as i32).max(0) as usize;
            let rx = bx.saturating_sub(new_room_w);
            let ry = if bend_sign > 0 {
                by
            } else {
                by.saturating_sub(new_room_h / 2)
            };
            (sx, sy, bx, by, rx, ry)
        }
        _ => {
            // Exit down, then bend left/right
            let sx = source.x + source.width / 2;
            let sy = source.y;
            let bx = (sx as i32 + bend_sign * leg2 as i32).max(0) as usize;
            let by = sy.saturating_sub(leg1);
            let rx = if bend_sign > 0 {
                bx
            } else {
                bx.saturating_sub(new_room_w / 2)
            };
            let ry = by.saturating_sub(new_room_h);
            (sx, sy, bx, by, rx, ry)
        }
    };

    Proposal {
        start_x,
        start_y,
        bend_x,
        bend_y,
        corridor_width,
        vertical_exit: direction == 1 || direction == 3,
        room: Room {
            x: room_x,
            y: room_y,
            width: new_room_w,
            height: new_room_h,
        },
    }
}

/// Carve an L-shaped corridor: start→bend, then bend→room center.
///
/// Each leg may span both axes (the proposal geometry isn't always axis-aligned),
/// so we carve an L-connector for each leg. The axis order matters:
/// - Horizontal exits (left/right): carve horizontal-first, then vertical
/// - Vertical exits (up/down): carve vertical-first, then horizontal
/// This ensures the first segment of each leg extends away from the source room,
/// placing the bend mid-corridor rather than flush against the room wall.
fn carve_l_corridor(maze: &mut Maze, proposal: &Proposal) {
    let w = proposal.corridor_width;
    let target_x = proposal.room.x + proposal.room.width / 2;
    let target_y = proposal.room.y + proposal.room.height / 2;

    if proposal.vertical_exit {
        // Vertical exit: carve vertical-first for both legs
        carve_vertical_first(
            maze,
            proposal.start_x,
            proposal.start_y,
            proposal.bend_x,
            proposal.bend_y,
            w,
        );
        carve_vertical_first(
            maze,
            proposal.bend_x,
            proposal.bend_y,
            target_x,
            target_y,
            w,
        );
    } else {
        // Horizontal exit: carve horizontal-first for both legs
        carve_horizontal_first(
            maze,
            proposal.start_x,
            proposal.start_y,
            proposal.bend_x,
            proposal.bend_y,
            w,
        );
        carve_horizontal_first(
            maze,
            proposal.bend_x,
            proposal.bend_y,
            target_x,
            target_y,
            w,
        );
    }
}

/// Carve an L-connector: horizontal segment at y1, then vertical segment at x2.
/// The bend lands at (x2, y1).
fn carve_horizontal_first(
    maze: &mut Maze,
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
    width: usize,
) {
    let half = width / 2;
    // Horizontal segment at y1
    let min_x = x1.min(x2);
    let max_x = x1.max(x2);
    for x in min_x..=max_x {
        for offset in 0..width {
            if let Some(y) = (y1 + offset).checked_sub(half)
                && x < maze.width
                && y < maze.height
            {
                maze.set(x, y, TileKind::Floor);
            }
        }
    }
    // Vertical segment at x2
    let min_y = y1.min(y2);
    let max_y = y1.max(y2);
    for y in min_y..=max_y {
        for offset in 0..width {
            if let Some(x) = (x2 + offset).checked_sub(half)
                && x < maze.width
                && y < maze.height
            {
                maze.set(x, y, TileKind::Floor);
            }
        }
    }
}

/// Carve an L-connector: vertical segment at x1, then horizontal segment at y2.
/// The bend lands at (x1, y2).
fn carve_vertical_first(maze: &mut Maze, x1: usize, y1: usize, x2: usize, y2: usize, width: usize) {
    let half = width / 2;
    // Vertical segment at x1
    let min_y = y1.min(y2);
    let max_y = y1.max(y2);
    for y in min_y..=max_y {
        for offset in 0..width {
            if let Some(x) = (x1 + offset).checked_sub(half)
                && x < maze.width
                && y < maze.height
            {
                maze.set(x, y, TileKind::Floor);
            }
        }
    }
    // Horizontal segment at y2
    let min_x = x1.min(x2);
    let max_x = x1.max(x2);
    for x in min_x..=max_x {
        for offset in 0..width {
            if let Some(y) = (y2 + offset).checked_sub(half)
                && x < maze.width
                && y < maze.height
            {
                maze.set(x, y, TileKind::Floor);
            }
        }
    }
}

/// Returns the fraction of `b`'s area that overlaps with `a` (0.0 = no overlap, 1.0 = fully inside).
fn overlap_fraction(a: &Room, b: &Room) -> f32 {
    let ox = (a.x + a.width)
        .min(b.x + b.width)
        .saturating_sub(a.x.max(b.x));
    let oy = (a.y + a.height)
        .min(b.y + b.height)
        .saturating_sub(a.y.max(b.y));
    let overlap_area = ox * oy;
    let b_area = b.width * b.height;
    if b_area == 0 {
        return 1.0;
    }
    overlap_area as f32 / b_area as f32
}

/// Carve a rectangular room (set all tiles to Floor)
fn carve_room(maze: &mut Maze, room: &Room) {
    for y in room.y..room.y + room.height {
        for x in room.x..room.x + room.width {
            if x < maze.width && y < maze.height {
                maze.set(x, y, TileKind::Floor);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_maze_has_player_start() {
        let config = MazeGenConfig::default();
        let (maze, state) = generate_initial_maze(&config);
        assert!(maze.find_player_start().is_some());
        assert_eq!(state.rooms.len(), 1);
    }

    #[test]
    fn grow_adds_room_and_track() {
        let config = MazeGenConfig::default();
        let (mut maze, mut state) = generate_initial_maze(&config);
        let result = grow_maze(&mut maze, &mut state, 0);
        assert!(result.is_some());
        assert_eq!(state.rooms.len(), 2);
        assert_eq!(maze.find_track_icons().len(), 1);
    }

    #[test]
    fn grow_multiple_rooms() {
        let config = MazeGenConfig::default();
        let (mut maze, mut state) = generate_initial_maze(&config);
        for i in 0..5 {
            let _ = grow_maze(&mut maze, &mut state, i);
        }
        assert_eq!(state.rooms.len(), 6);
    }

    #[test]
    fn track_ids_survive_expansion() {
        let config = MazeGenConfig::default();
        let (mut maze, mut state) = generate_initial_maze(&config);
        for i in 0..3 {
            let _ = grow_maze(&mut maze, &mut state, i);
        }
        // Each track icon should have a unique track_id in the map
        assert_eq!(maze.track_ids.len(), 3);
        let mut ids: Vec<usize> = maze.track_ids.values().copied().collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    #[test]
    fn corridor_near_edge_no_panic() {
        // Verify that corridor carving near the maze edge does not panic.
        let config = MazeGenConfig {
            min_room_size: 3,
            max_room_size: 3,
            min_corridor_length: 2,
            max_corridor_length: 4,
            min_corridor_width: 3,
            max_corridor_width: 3,
            max_overlap_fraction: 1.0,
            min_track_distance: 0,
            l_bend_chance: 0.35,
        };
        let (mut maze, mut state) = generate_initial_maze(&config);
        for i in 0..20 {
            let _ = grow_maze(&mut maze, &mut state, i);
        }
        for y in 0..maze.height {
            for x in 0..maze.width {
                assert!(maze.get(x, y).is_some());
            }
        }
    }

    #[test]
    fn corridor_completeness() {
        // Verify flood-fill connectivity: all tracks reachable from player start.
        let config = MazeGenConfig {
            min_room_size: 3,
            max_room_size: 5,
            min_corridor_length: 4,
            max_corridor_length: 8,
            min_corridor_width: 1,
            max_corridor_width: 3,
            max_overlap_fraction: 0.3,
            min_track_distance: 0,
            l_bend_chance: 0.35,
        };
        let (mut maze, mut state) = generate_initial_maze(&config);
        let num_tracks = 8;
        for i in 0..num_tracks {
            grow_maze(&mut maze, &mut state, i);
        }

        let start = maze
            .find_player_start()
            .unwrap_or_else(|| state.rooms[0].center());
        let mut visited = vec![false; maze.width * maze.height];
        let mut queue = std::collections::VecDeque::new();
        let sx = start.x as usize;
        let sy = start.y as usize;
        visited[sy * maze.width + sx] = true;
        queue.push_back((sx, sy));

        while let Some((cx, cy)) = queue.pop_front() {
            for (nx, ny) in [
                (cx.wrapping_sub(1), cy),
                (cx + 1, cy),
                (cx, cy.wrapping_sub(1)),
                (cx, cy + 1),
            ] {
                if nx < maze.width && ny < maze.height {
                    let idx = ny * maze.width + nx;
                    if !visited[idx] && !maze.is_wall(nx as i32, ny as i32) {
                        visited[idx] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }
        }

        let track_positions = maze.find_track_icons();
        assert_eq!(track_positions.len(), num_tracks);
        for pos in &track_positions {
            let idx = pos.y as usize * maze.width + pos.x as usize;
            assert!(
                visited[idx],
                "Track at ({}, {}) is unreachable from player start",
                pos.x, pos.y
            );
        }
    }

    #[test]
    fn scaled_config_has_longer_corridors() {
        // Verify structural property: the scaled config for 12 tracks
        // specifies longer corridors than the default config.
        let scaled = config_for_track_count(12);
        let default = MazeGenConfig::default();

        assert!(
            scaled.min_corridor_length > default.min_corridor_length,
            "Scaled min corridor ({}) should exceed default ({})",
            scaled.min_corridor_length,
            default.min_corridor_length
        );
        assert!(
            scaled.max_corridor_length > default.max_corridor_length,
            "Scaled max corridor ({}) should exceed default ({})",
            scaled.max_corridor_length,
            default.max_corridor_length
        );
        assert!(
            scaled.min_track_distance > default.min_track_distance,
            "Scaled min track distance ({}) should exceed default ({})",
            scaled.min_track_distance,
            default.min_track_distance
        );
    }

    #[test]
    fn l_corridors_with_12_tracks() {
        // Smoke test: generate a full 12-track album with scaled config.
        // All tracks should be reachable.
        let config = config_for_track_count(12);
        let (mut maze, mut state) = generate_initial_maze(&config);
        for i in 0..12 {
            grow_maze(&mut maze, &mut state, i);
        }

        assert_eq!(state.rooms.len(), 13); // 1 start + 12 tracks
        assert_eq!(maze.find_track_icons().len(), 12);

        // Flood fill check
        let start = maze
            .find_player_start()
            .unwrap_or_else(|| state.rooms[0].center());
        let mut visited = vec![false; maze.width * maze.height];
        let mut queue = std::collections::VecDeque::new();
        let sx = start.x as usize;
        let sy = start.y as usize;
        visited[sy * maze.width + sx] = true;
        queue.push_back((sx, sy));

        while let Some((cx, cy)) = queue.pop_front() {
            for (nx, ny) in [
                (cx.wrapping_sub(1), cy),
                (cx + 1, cy),
                (cx, cy.wrapping_sub(1)),
                (cx, cy + 1),
            ] {
                if nx < maze.width && ny < maze.height {
                    let idx = ny * maze.width + nx;
                    if !visited[idx] && !maze.is_wall(nx as i32, ny as i32) {
                        visited[idx] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }
        }

        for pos in &maze.find_track_icons() {
            let idx = pos.y as usize * maze.width + pos.x as usize;
            assert!(
                visited[idx],
                "Track at ({}, {}) is unreachable",
                pos.x, pos.y
            );
        }
    }
}
