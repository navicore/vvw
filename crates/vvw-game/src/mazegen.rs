//! Procedural maze generation: rooms connected by corridors

use bevy::prelude::*;
use rand::Rng;

use crate::maze::Maze;
use crate::tiles::{TileKind, TilePos};

/// Configuration for maze generation
#[derive(Resource, Debug, Clone)]
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
        }
    }
}

/// A rectangular room in the maze
#[derive(Debug, Clone)]
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
#[derive(Resource)]
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

/// Grow the maze by extending a corridor from a random existing room and carving a new room.
/// `track_id` is stored in the maze's `track_ids` map for stable ID assignment.
/// Returns the center of the new room (for placing a track icon).
///
/// Attempts multiple placements to avoid overlapping existing rooms. Overlap is
/// accepted only as a last resort (~1 in 20 chance when all attempts fail).
#[allow(clippy::too_many_lines)]
pub fn grow_maze(maze: &mut Maze, state: &mut MazeGenState, track_id: usize) -> Option<TilePos> {
    const MAX_ATTEMPTS: usize = 40;

    let mut rng = rand::thread_rng();

    if state.rooms.is_empty() {
        return None;
    }

    // Try multiple placements, keeping the first non-overlapping one
    let mut best = None;

    let threshold = state.config.max_overlap_fraction;

    for _ in 0..MAX_ATTEMPTS {
        let candidate = propose_room(&mut rng, state);
        // Reject if any existing room overlaps the new room beyond the
        // configured threshold. Lower thresholds spread rooms further apart.
        let too_much = state
            .rooms
            .iter()
            .any(|r| overlap_fraction(r, &candidate.room) > threshold);
        if !too_much {
            best = Some(candidate);
            break;
        }
        // Keep the last candidate as a fallback
        if best.is_none() {
            best = Some(candidate);
        }
    }

    let proposal = best?;

    // Ensure maze is large enough - expand if needed (account for corridor width)
    let half = proposal.corridor_width / 2;
    let needed_right = proposal.room.x + proposal.room.width + 2 + half;
    let needed_top = proposal.room.y + proposal.room.height + 2 + half;
    let expand_right = needed_right.saturating_sub(maze.width);
    let expand_top = needed_top.saturating_sub(maze.height);

    if expand_right > 0 || expand_top > 0 {
        let (dx, dy) = maze.expand(0, expand_right, 0, expand_top);
        for room in &mut state.rooms {
            room.x = (room.x as i32 + dx) as usize;
            room.y = (room.y as i32 + dy) as usize;
        }
    }

    // Carve the corridor (variable width: carve parallel tiles)
    let Proposal {
        corridor_start_x,
        corridor_start_y,
        corridor_width,
        is_horizontal,
        room: new_room,
    } = proposal;

    if is_horizontal {
        let target_x = new_room.x + new_room.width / 2;
        let min_x = corridor_start_x.min(target_x);
        let max_x = corridor_start_x.max(target_x);
        for x in min_x..=max_x {
            for offset in 0..corridor_width {
                let y = corridor_start_y.saturating_sub(half) + offset;
                if x < maze.width && y < maze.height {
                    maze.set(x, y, TileKind::Floor);
                }
            }
        }
    } else {
        let target_y = new_room.y + new_room.height / 2;
        let min_y = corridor_start_y.min(target_y);
        let max_y = corridor_start_y.max(target_y);
        for y in min_y..=max_y {
            for offset in 0..corridor_width {
                let x = corridor_start_x.saturating_sub(half) + offset;
                if x < maze.width && y < maze.height {
                    maze.set(x, y, TileKind::Floor);
                }
            }
        }
    }

    // Carve the new room
    carve_room(maze, &new_room);

    // Place track icon at room center, or nearby if center is already taken
    let center = new_room.center();
    let mut ix = center.x as usize;
    let mut iy = center.y as usize;
    if maze.track_ids.contains_key(&(ix, iy)) {
        'search: for dy in 0..new_room.height {
            for dx in 0..new_room.width {
                let ax = new_room.x + dx;
                let ay = new_room.y + dy;
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

    // Restore any existing TrackIcons that were overwritten by corridor/room carving
    maze.restore_track_icons();

    state.rooms.push(new_room);

    Some(center)
}

/// A proposed room placement (before committing to the maze).
struct Proposal {
    corridor_start_x: usize,
    corridor_start_y: usize,
    corridor_width: usize,
    is_horizontal: bool,
    room: Room,
}

/// Generate a candidate room placement from a random source room and direction.
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

    let (corridor_start_x, corridor_start_y, new_room_x, new_room_y, is_horizontal) =
        match direction {
            0 => {
                // Right
                let cy = source.y + source.height / 2;
                let cx = source.x + source.width;
                (cx, cy, cx + corridor_len, cy.saturating_sub(new_room_h / 2), true)
            }
            1 => {
                // Up
                let cx = source.x + source.width / 2;
                let cy = source.y + source.height;
                (cx, cy, cx.saturating_sub(new_room_w / 2), cy + corridor_len, false)
            }
            2 => {
                // Left
                let cy = source.y + source.height / 2;
                let cx = source.x;
                (cx, cy, cx.saturating_sub(corridor_len + new_room_w), cy.saturating_sub(new_room_h / 2), true)
            }
            _ => {
                // Down
                let cx = source.x + source.width / 2;
                let cy = source.y;
                (cx, cy, cx.saturating_sub(new_room_w / 2), cy.saturating_sub(corridor_len + new_room_h), false)
            }
        };

    Proposal {
        corridor_start_x,
        corridor_start_y,
        corridor_width,
        is_horizontal,
        room: Room {
            x: new_room_x,
            y: new_room_y,
            width: new_room_w,
            height: new_room_h,
        },
    }
}

/// Returns the fraction of `b`'s area that overlaps with `a` (0.0 = no overlap, 1.0 = fully inside).
fn overlap_fraction(a: &Room, b: &Room) -> f32 {
    let ox = (a.x + a.width).min(b.x + b.width).saturating_sub(a.x.max(b.x));
    let oy = (a.y + a.height).min(b.y + b.height).saturating_sub(a.y.max(b.y));
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
}
