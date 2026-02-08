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
}

impl Default for MazeGenConfig {
    fn default() -> Self {
        Self {
            min_room_size: 3,
            max_room_size: 7,
            min_corridor_length: 2,
            max_corridor_length: 6,
            min_corridor_width: 1,
            max_corridor_width: 3,
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
#[allow(clippy::too_many_lines)]
pub fn grow_maze(maze: &mut Maze, state: &mut MazeGenState, track_id: usize) -> Option<TilePos> {
    let mut rng = rand::thread_rng();

    if state.rooms.is_empty() {
        return None;
    }

    // Pick a random source room
    let source_idx = rng.gen_range(0..state.rooms.len());
    let source = state.rooms[source_idx].clone();

    // Pick a random direction: 0=right, 1=up, 2=left, 3=down
    let direction: u8 = rng.gen_range(0..4);

    let corridor_len =
        rng.gen_range(state.config.min_corridor_length..=state.config.max_corridor_length);
    let corridor_width =
        rng.gen_range(state.config.min_corridor_width..=state.config.max_corridor_width);
    let new_room_w = rng.gen_range(state.config.min_room_size..=state.config.max_room_size);
    let new_room_h = rng.gen_range(state.config.min_room_size..=state.config.max_room_size);

    // Calculate corridor start (edge of source room) and new room position
    let (corridor_start_x, corridor_start_y, new_room_x, new_room_y, is_horizontal) =
        match direction {
            0 => {
                // Right
                let cy = source.y + source.height / 2;
                let cx = source.x + source.width;
                let rx = cx + corridor_len;
                let ry = cy.saturating_sub(new_room_h / 2);
                (cx, cy, rx, ry, true)
            }
            1 => {
                // Up
                let cx = source.x + source.width / 2;
                let cy = source.y + source.height;
                let rx = cx.saturating_sub(new_room_w / 2);
                let ry = cy + corridor_len;
                (cx, cy, rx, ry, false)
            }
            2 => {
                // Left
                let cy = source.y + source.height / 2;
                let cx = source.x;
                let rx = cx.saturating_sub(corridor_len + new_room_w);
                let ry = cy.saturating_sub(new_room_h / 2);
                (cx, cy, rx, ry, true)
            }
            _ => {
                // Down
                let cx = source.x + source.width / 2;
                let cy = source.y;
                let rx = cx.saturating_sub(new_room_w / 2);
                let ry = cy.saturating_sub(corridor_len + new_room_h);
                (cx, cy, rx, ry, false)
            }
        };

    // Ensure maze is large enough - expand if needed (account for corridor width)
    let half = corridor_width / 2;
    let needed_right = new_room_x + new_room_w + 2 + half;
    let needed_top = new_room_y + new_room_h + 2 + half;
    let expand_right = needed_right.saturating_sub(maze.width);
    let expand_top = needed_top.saturating_sub(maze.height);

    if expand_right > 0 || expand_top > 0 {
        let (dx, dy) = maze.expand(0, expand_right, 0, expand_top);
        // Adjust all room coordinates
        for room in &mut state.rooms {
            room.x = (room.x as i32 + dx) as usize;
            room.y = (room.y as i32 + dy) as usize;
        }
    }

    let new_room = Room {
        x: new_room_x,
        y: new_room_y,
        width: new_room_w,
        height: new_room_h,
    };

    // Carve the corridor (variable width: carve parallel tiles)
    if is_horizontal {
        let target_x = new_room_x + new_room_w / 2;
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
        let target_y = new_room_y + new_room_h / 2;
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
        // Find an alternate floor tile within the room
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
