//! 3D view toggle — switch between 2D top-down and 3D first-person view
//!
//! The view toggle is independent of the interaction mode framework. Modes
//! (Mute, Pipe, Breadcrumbs) work identically in both views. Toggle via
//! `V` key (desktop) or three-finger tap (mobile).
//!
//! Enabled per-album via `morph_3d: true` in `project.ron`.

use bevy::input::touch::Touches;
use bevy::prelude::*;

use vvw_core::maze::Maze;
use vvw_light::LightMapOverlay;

use crate::camera::GameCamera;
use crate::maze::{MazeTile, colors};
use crate::player::{Player, PlayerHeading};
use crate::tiles::{TILE_SIZE, TileKind, TilePos};

/// Marker component for 3D tile meshes (walls, floors, track cubes).
#[derive(Component)]
pub struct Mesh3dTile;

/// Marker for the 3D camera.
#[derive(Component)]
pub struct GameCamera3d;

/// Marker for the player's 3D spotlight.
#[derive(Component)]
pub struct PlayerSpotlight3d;

/// Marker for 3D point lights at track positions.
#[derive(Component)]
pub struct TrackLight3d;

/// Whether the 3D view is currently active. Checked by player input
/// (heading-relative controls) and the follow-camera system.
#[derive(Resource, Default)]
pub struct Morph3dActive(pub bool);

/// Whether the 3D view toggle is enabled for this album.
/// Defaults to false; platform layer inserts `Morph3dEnabled(true)` when
/// `morph_3d: true` in album config.
#[derive(Resource, Default)]
pub struct Morph3dEnabled(pub bool);

// ── Three-finger tap detection ─────────────────────────────────────────────

/// Max finger movement (pixels) before we reject as a drag
const TAP3_MOVE_THRESHOLD: f32 = 50.0;
/// Max duration (seconds) fingers can be held before we reject
const TAP3_MAX_DURATION: f64 = 1.5;
/// Cooldown (seconds) after a successful tap
const TAP3_COOLDOWN: f64 = 0.8;

/// State machine for detecting three-finger taps.
///
/// Arms when 3+ fingers are down. Fires when all fingers lift if
/// movement was small and duration was short. The two-finger tap
/// detector cancels itself when 3+ fingers appear, so no conflict.
#[derive(Resource, Default)]
struct ThreeFingerTapState {
    tracking: bool,
    cancelled: bool,
    start_positions: [Vec2; 3],
    start_ids: [u64; 3],
    last_positions: [Vec2; 3],
    start_time: f64,
    cooldown_until: f64,
}

/// Wall height in world units.
const WALL_HEIGHT: f32 = TILE_SIZE;

/// Camera eye height (fraction of tile size).
const EYE_HEIGHT: f32 = TILE_SIZE * 0.4;

pub struct Morph3dPlugin;

impl Plugin for Morph3dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Morph3dActive>()
            .init_resource::<Morph3dEnabled>()
            .init_resource::<ThreeFingerTapState>()
            .add_systems(
                PostStartup,
                (setup_3d_meshes, setup_3d_camera_and_lights)
                    .run_if(|enabled: Res<Morph3dEnabled>| enabled.0),
            )
            .add_systems(
                Update,
                toggle_3d_view
                    .run_if(|enabled: Res<Morph3dEnabled>| enabled.0)
                    .before(crate::player::handle_player_input),
            )
            .add_systems(
                Update,
                follow_player_3d
                    .run_if(|active: Res<Morph3dActive>| active.0)
                    .after(toggle_3d_view),
            );
    }
}

// ── Startup ────────────────────────────────────────────────────────────────

fn setup_3d_meshes(
    mut commands: Commands,
    maze: Res<Maze>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_3d_meshes_from_maze(&mut commands, &maze, &mut meshes, &mut materials);
}

fn setup_3d_camera_and_lights(mut commands: Commands, maze: Res<Maze>) {
    let start_pos = maze.find_player_start().unwrap_or(TilePos::new(1, 1));
    let world = start_pos.to_world();
    let cam_pos = Vec3::new(world.x, EYE_HEIGHT, -world.y);

    commands.spawn((
        Camera3d::default(),
        Camera {
            order: -1,
            is_active: false,
            ..default()
        },
        Transform::from_translation(cam_pos).looking_to(Vec3::NEG_Z, Vec3::Y),
        GameCamera3d,
    ));

    for &(x, y) in maze.track_ids.keys() {
        let tw = TilePos::new(x as i32, y as i32).to_world();
        commands.spawn((
            PointLight {
                color: Color::srgb(0.8, 0.4, 0.2),
                intensity: 800.0,
                range: TILE_SIZE * 4.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(tw.x, WALL_HEIGHT * 0.8, -tw.y),
            Visibility::Hidden,
            TrackLight3d,
        ));
    }

    commands.spawn((
        SpotLight {
            color: Color::srgb(1.0, 0.9, 0.6),
            intensity: 1500.0,
            range: TILE_SIZE * 5.0,
            outer_angle: 0.8,
            inner_angle: 0.5,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(cam_pos).looking_to(Vec3::NEG_Z, Vec3::Y),
        Visibility::Hidden,
        PlayerSpotlight3d,
    ));
}

// ── View toggle ────────────────────────────────────────────────────────────

/// Use a `SystemParam` to bundle the many queries needed for view toggling.
#[derive(bevy::ecs::system::SystemParam)]
#[allow(clippy::type_complexity)]
struct MorphQueries<'w, 's> {
    tiles: Query<'w, 's, &'static mut Visibility, (With<MazeTile>, Without<Mesh3dTile>)>,
    meshes: Query<'w, 's, &'static mut Visibility, (With<Mesh3dTile>, Without<MazeTile>)>,
    cam2d: Query<'w, 's, (Entity, &'static mut Camera), (With<GameCamera>, Without<GameCamera3d>)>,
    cam3d: Query<'w, 's, (Entity, &'static mut Camera), (With<GameCamera3d>, Without<GameCamera>)>,
    lights3d: Query<
        'w,
        's,
        &'static mut Visibility,
        (
            Or<(With<TrackLight3d>, With<PlayerSpotlight3d>)>,
            Without<MazeTile>,
            Without<Mesh3dTile>,
            Without<LightMapOverlay>,
        ),
    >,
    lightmap: Query<
        'w,
        's,
        &'static mut Visibility,
        (
            With<LightMapOverlay>,
            Without<MazeTile>,
            Without<Mesh3dTile>,
            Without<TrackLight3d>,
            Without<PlayerSpotlight3d>,
        ),
    >,
    player: Query<
        'w,
        's,
        &'static mut Visibility,
        (
            With<Player>,
            Without<MazeTile>,
            Without<Mesh3dTile>,
            Without<TrackLight3d>,
            Without<PlayerSpotlight3d>,
            Without<LightMapOverlay>,
        ),
    >,
}

/// Toggle 3D view on `V` key or three-finger tap.
fn toggle_3d_view(
    keyboard: Res<ButtonInput<KeyCode>>,
    touches: Res<Touches>,
    time: Res<Time>,
    mut tap_state: ResMut<ThreeFingerTapState>,
    mut morph_active: ResMut<Morph3dActive>,
    mut q: MorphQueries,
    mut commands: Commands,
) {
    let toggled = keyboard.just_pressed(KeyCode::KeyV)
        || detect_three_finger_tap(&touches, &time, &mut tap_state);

    if !toggled {
        return;
    }

    let want_3d = !morph_active.0;
    morph_active.0 = want_3d;

    let (show_2d, show_3d) = if want_3d {
        (Visibility::Hidden, Visibility::Inherited)
    } else {
        (Visibility::Inherited, Visibility::Hidden)
    };

    for mut vis in &mut q.tiles {
        *vis = show_2d;
    }
    for mut vis in &mut q.meshes {
        *vis = show_3d;
    }
    for (entity, mut cam) in &mut q.cam2d {
        cam.is_active = !want_3d;
        if want_3d {
            commands.entity(entity).remove::<IsDefaultUiCamera>();
        } else {
            commands.entity(entity).insert(IsDefaultUiCamera);
        }
    }
    for (entity, mut cam) in &mut q.cam3d {
        cam.is_active = want_3d;
        if want_3d {
            commands.entity(entity).insert(IsDefaultUiCamera);
        } else {
            commands.entity(entity).remove::<IsDefaultUiCamera>();
        }
    }
    for mut vis in &mut q.lights3d {
        *vis = show_3d;
    }
    for mut vis in &mut q.lightmap {
        *vis = show_2d;
    }
    // Hide player sprite in 3D (camera IS the player)
    for mut vis in &mut q.player {
        *vis = show_2d;
    }

    if want_3d {
        info!("View: switched to 3D");
    } else {
        info!("View: switched to 2D");
    }
}

/// Three-finger tap detection using the same state-machine approach as two-finger tap.
///
/// Arms when 3+ fingers are down. Fires when all lift if movement was small
/// and duration was short.
fn detect_three_finger_tap(
    touches: &Touches,
    time: &Time,
    state: &mut ThreeFingerTapState,
) -> bool {
    let now = time.elapsed_secs_f64();

    if now < state.cooldown_until {
        return false;
    }

    let count = touches.iter().count();

    // Clear cancelled state once all fingers lift
    if state.cancelled {
        if count == 0 {
            state.cancelled = false;
        }
        return false;
    }

    // 4+ fingers without tracking — reject immediately
    if count > 3 && !state.tracking {
        state.cancelled = true;
        return false;
    }

    if count == 3 && !state.tracking {
        // Exactly three fingers appeared — start tracking
        let mut iter = touches.iter();
        let f0 = iter.next().unwrap();
        let f1 = iter.next().unwrap();
        let f2 = iter.next().unwrap();
        state.tracking = true;
        state.start_ids = [f0.id(), f1.id(), f2.id()];
        state.start_positions = [f0.position(), f1.position(), f2.position()];
        state.last_positions = state.start_positions;
        state.start_time = now;
        return false;
    }

    if state.tracking && count > 3 {
        // Too many fingers — cancel
        state.tracking = false;
        state.cancelled = true;
        return false;
    }

    if state.tracking && count >= 3 {
        // Still holding — update positions and check movement
        for (i, id) in state.start_ids.iter().enumerate() {
            if let Some(touch) = touches.get_pressed(*id) {
                state.last_positions[i] = touch.position();
                if touch.position().distance(state.start_positions[i]) > TAP3_MOVE_THRESHOLD {
                    state.tracking = false;
                    return false;
                }
            }
        }
        if now - state.start_time > TAP3_MAX_DURATION {
            state.tracking = false;
            return false;
        }
        return false;
    }

    if state.tracking && count == 0 {
        // All fingers lifted — check final positions
        // Update from just-released events
        for (i, id) in state.start_ids.iter().enumerate() {
            for released in touches.iter_just_released() {
                if released.id() == *id {
                    state.last_positions[i] = released.position();
                }
            }
        }

        for i in 0..3 {
            if state.last_positions[i].distance(state.start_positions[i]) > TAP3_MOVE_THRESHOLD {
                state.tracking = false;
                return false;
            }
        }

        state.tracking = false;
        let duration = now - state.start_time;
        if duration <= TAP3_MAX_DURATION {
            state.cooldown_until = now + TAP3_COOLDOWN;
            return true;
        }
    }

    // Fingers partially lifted (1-2 remaining) — keep tracking, they may
    // just be lifting sequentially. Time out if held too long.
    if state.tracking && count > 0 && count < 3 && now - state.start_time > TAP3_MAX_DURATION {
        state.tracking = false;
    }

    false
}

// ── Camera follow ──────────────────────────────────────────────────────────

type CamFilter = (
    With<GameCamera3d>,
    Without<Player>,
    Without<PlayerSpotlight3d>,
);
type SpotFilter = (
    With<PlayerSpotlight3d>,
    Without<Player>,
    Without<GameCamera3d>,
);

/// Keep the 3D camera and spotlight in sync with the player's 2D position
/// and heading. Only runs when `Morph3dActive` is true.
fn follow_player_3d(
    player_query: Query<(&Transform, &PlayerHeading), With<Player>>,
    mut cam_query: Query<&mut Transform, CamFilter>,
    mut spot_query: Query<&mut Transform, SpotFilter>,
) {
    let Ok((player_tf, heading)) = player_query.single() else {
        return;
    };

    let player_2d = player_tf.translation.truncate();
    let pos_3d = Vec3::new(player_2d.x, EYE_HEIGHT, -player_2d.y);

    // Convert 2D heading (X, Y) to 3D look direction (X, 0, -Y)
    let look_dir = Vec3::new(heading.0.x, 0.0, -heading.0.y).normalize_or(Vec3::NEG_Z);

    if let Ok(mut cam_tf) = cam_query.single_mut() {
        *cam_tf = Transform::from_translation(pos_3d).looking_to(look_dir, Vec3::Y);
    }

    if let Ok(mut spot_tf) = spot_query.single_mut() {
        *spot_tf = Transform::from_translation(pos_3d).looking_to(look_dir, Vec3::Y);
    }
}

// ── 3D mesh spawning ───────────────────────────────────────────────────────

/// Spawn hidden 3D meshes for the current maze state.
///
/// Called at startup and on `MazeChanged` (via `respawn_maze_tiles`).
pub fn spawn_3d_meshes_from_maze(
    commands: &mut Commands,
    maze: &Maze,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let wall_mesh = meshes.add(Cuboid::new(TILE_SIZE, WALL_HEIGHT, TILE_SIZE));
    let floor_mesh = meshes.add(Cuboid::new(TILE_SIZE, 0.1, TILE_SIZE));
    let icon_size = TILE_SIZE * 0.6;
    let icon_mesh = meshes.add(Cuboid::new(icon_size, icon_size, icon_size));

    let wall_mat = materials.add(StandardMaterial {
        base_color: colors::WALL,
        ..default()
    });
    let floor_mat = materials.add(StandardMaterial {
        base_color: colors::FLOOR,
        ..default()
    });
    let icon_mat = materials.add(StandardMaterial {
        base_color: colors::TRACK_ICON,
        ..default()
    });

    let mut start_mat = None;

    for y in 0..maze.height {
        for x in 0..maze.width {
            let tile = maze.get(x, y).unwrap_or_default();

            // 3D coordinates: X maps to 2D X, Z maps to -2D Y (Bevy 3D is Y-up, -Z forward)
            let world_x = (x as f32).mul_add(TILE_SIZE, TILE_SIZE / 2.0);
            let world_z = -(y as f32).mul_add(TILE_SIZE, TILE_SIZE / 2.0);

            match tile {
                TileKind::Wall => {
                    commands.spawn((
                        Mesh3d(wall_mesh.clone()),
                        MeshMaterial3d(wall_mat.clone()),
                        Transform::from_xyz(world_x, WALL_HEIGHT / 2.0, world_z),
                        Visibility::Hidden,
                        Mesh3dTile,
                    ));
                }
                TileKind::Floor | TileKind::PlayerStart => {
                    let mat = if tile == TileKind::PlayerStart {
                        start_mat
                            .get_or_insert_with(|| {
                                materials.add(StandardMaterial {
                                    base_color: colors::PLAYER_START,
                                    unlit: true,
                                    ..default()
                                })
                            })
                            .clone()
                    } else {
                        floor_mat.clone()
                    };
                    commands.spawn((
                        Mesh3d(floor_mesh.clone()),
                        MeshMaterial3d(mat),
                        Transform::from_xyz(world_x, 0.0, world_z),
                        Visibility::Hidden,
                        Mesh3dTile,
                    ));
                }
                TileKind::TrackIcon => {
                    commands.spawn((
                        Mesh3d(floor_mesh.clone()),
                        MeshMaterial3d(floor_mat.clone()),
                        Transform::from_xyz(world_x, 0.0, world_z),
                        Visibility::Hidden,
                        Mesh3dTile,
                    ));
                    commands.spawn((
                        Mesh3d(icon_mesh.clone()),
                        MeshMaterial3d(icon_mat.clone()),
                        Transform::from_xyz(world_x, icon_size / 2.0, world_z),
                        Visibility::Hidden,
                        Mesh3dTile,
                    ));
                }
            }
        }
    }
}
