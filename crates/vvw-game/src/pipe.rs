//! Sound piping — route audio from a distant track to a new location
//!
//! When pipe mode is activated, the nearest audible track becomes the source.
//! As the player walks, a dashed preview line extends from start to current
//! position. On deactivation, the pipe is finalized: a `PipeSpeaker` entity
//! is spawned at the player's position, participating in the same LOS/proximity
//! rules as any track. The platform layer forks the source track's audio graph.

use bevy::prelude::*;

use vvw_core::modes::{ModeDescriptor, ModeId};

use crate::audio::{TrackAudioState, TrackIdCounter};
use crate::maze::TrackIcon;
use crate::modes::{ActiveMode, ModeRegistry};
use crate::player::Player;
use crate::tiles::TilePos;

/// Maximum number of pipes per session
const MAX_PIPES: usize = 8;

/// Width of pipe dash segments (world units)
const DASH_WIDTH: f32 = 7.0;
/// Length of each dash segment
const DASH_LENGTH: f32 = 12.0;
/// Gap between dash segments
const DASH_GAP: f32 = 6.0;
/// Pipe color (construction line blue, semi-transparent)
const PIPE_COLOR: Color = Color::srgba(0.3, 0.5, 0.9, 0.75);
/// Preview pipe color (dimmer)
const PREVIEW_COLOR: Color = Color::srgba(0.3, 0.5, 0.9, 0.45);

const PIPE_MODE_ID: &str = "sound_pipe";

// ── Components & Resources ──────────────────────────────────────────────────

/// Marker for a finalized pipe speaker entity. Participates in spatial audio
/// via `TrackIcon` + `TilePos` + `TrackAudioState`.
#[derive(Component)]
pub struct PipeSpeaker {
    pub source_track_id: usize,
}

/// A single dash segment of a pipe (preview or finalized)
#[derive(Component)]
struct PipeDash;

/// Marker for preview dash segments (despawned on finalize/cancel)
#[derive(Component)]
struct PipePreview;

/// State tracked during pipe placement
#[derive(Resource, Default)]
struct PipePlacementState {
    active: bool,
    source_track_id: usize,
    start_pos: Vec2,
}

/// Registry of placed pipes for future serialization
#[derive(Resource, Default)]
pub struct PipeRegistry {
    pub pipes: Vec<PipeDescriptor>,
}

/// Describes a placed pipe
#[derive(Clone)]
pub struct PipeDescriptor {
    pub source_track_id: usize,
    pub speaker_track_id: usize,
    pub start: Vec2,
    pub end: Vec2,
}

/// Message sent when a pipe is placed. Platform layer consumes this to fork
/// the audio graph.
#[derive(Message)]
pub struct PipePlaced {
    pub source_track_id: usize,
    pub speaker_track_id: usize,
    pub start: Vec2,
    pub end: Vec2,
}

// ── Plugin ──────────────────────────────────────────────────────────────────

pub struct SoundPipePlugin;

impl Plugin for SoundPipePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PipePlacementState>()
            .init_resource::<PipeRegistry>()
            .add_message::<PipePlaced>()
            .add_systems(Startup, register_pipe_mode)
            .add_systems(
                Update,
                // Chain is load-bearing: on deactivation, `watch_mode_changes`
                // sets `state.active = false`; `update_pipe_preview` must run
                // after to see the updated state and despawn preview dashes.
                (watch_mode_changes, update_pipe_preview)
                    .chain()
                    .run_if(resource_changed::<ActiveMode>.or(pipe_placement_active)),
            );
    }
}

fn pipe_placement_active(state: Res<PipePlacementState>) -> bool {
    state.active
}

fn register_pipe_mode(mut registry: ResMut<ModeRegistry>) {
    registry.register(ModeDescriptor {
        id: ModeId(PIPE_MODE_ID.into()),
        label: "Pipe".into(),
        suppresses_movement: false,
        order: 200,
    });
}

// ── Mode lifecycle ──────────────────────────────────────────────────────────

/// React to `ActiveMode` changes: start or stop pipe placement.
#[allow(clippy::too_many_arguments)]
fn watch_mode_changes(
    active: Res<ActiveMode>,
    mut state: ResMut<PipePlacementState>,
    mut counter: ResMut<TrackIdCounter>,
    mut registry: ResMut<PipeRegistry>,
    mut pipe_messages: MessageWriter<PipePlaced>,
    player_query: Query<&Transform, With<Player>>,
    track_query: Query<(&TrackIcon, &TrackAudioState), Without<PipeSpeaker>>,
    mut commands: Commands,
) {
    if !active.is_changed() {
        return;
    }

    let pipe_mode_active = active.0.as_ref().is_some_and(|id| id.0 == PIPE_MODE_ID);

    if pipe_mode_active && !state.active {
        // Mode just activated — start pipe placement
        let Ok(player_tf) = player_query.single() else {
            return;
        };
        let player_pos = player_tf.translation.truncate();

        // Find loudest audible track (highest current_gain)
        let source = track_query
            .iter()
            .filter(|(_, audio)| audio.current_gain > 0.01)
            .max_by(|(_, a), (_, b)| {
                a.current_gain
                    .partial_cmp(&b.current_gain)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        let Some((track_icon, _)) = source else {
            // No audible track — can't start piping
            info!("Pipe mode: no audible track nearby");
            return;
        };

        state.active = true;
        state.source_track_id = track_icon.track_id;
        state.start_pos = player_pos;
        info!(
            "Pipe started from track {} at ({:.0}, {:.0})",
            track_icon.track_id, player_pos.x, player_pos.y
        );
    } else if !pipe_mode_active && state.active {
        // Mode just deactivated — finalize or cancel
        // Preview cleanup is handled by `update_pipe_preview` (which runs
        // next in the chain) seeing `state.active == false`.
        state.active = false;

        if registry.pipes.len() >= MAX_PIPES {
            info!("Pipe limit reached ({MAX_PIPES})");
            return;
        }

        let Ok(player_tf) = player_query.single() else {
            return;
        };
        let end_pos = player_tf.translation.truncate();

        // Don't place pipe if player didn't move
        if end_pos.distance(state.start_pos) < 20.0 {
            info!("Pipe cancelled — didn't move far enough");
            return;
        }

        // Allocate a new track ID for the speaker
        let speaker_track_id = counter.0;
        counter.0 += 1;

        // Spawn the pipe speaker entity
        let tile_pos = TilePos::from_world(end_pos);
        commands.spawn((
            PipeSpeaker {
                source_track_id: state.source_track_id,
            },
            TrackIcon {
                track_id: speaker_track_id,
            },
            tile_pos,
            TrackAudioState::default(),
            Transform::from_xyz(end_pos.x, end_pos.y, 1.0),
            GlobalTransform::default(),
            Visibility::Inherited,
        ));

        // Spawn finalized pipe dashes
        spawn_dashes(&mut commands, state.start_pos, end_pos, PIPE_COLOR, false);

        // Record in registry
        let descriptor = PipeDescriptor {
            source_track_id: state.source_track_id,
            speaker_track_id,
            start: state.start_pos,
            end: end_pos,
        };
        registry.pipes.push(descriptor);

        // Notify platform layer to fork the audio graph
        pipe_messages.write(PipePlaced {
            source_track_id: state.source_track_id,
            speaker_track_id,
            start: state.start_pos,
            end: end_pos,
        });

        info!(
            "Pipe placed: track {} → speaker {} at ({:.0}, {:.0})",
            state.source_track_id, speaker_track_id, end_pos.x, end_pos.y
        );
    }
}

// ── Preview ─────────────────────────────────────────────────────────────────

/// Update the preview dashes each frame while placing a pipe.
fn update_pipe_preview(
    state: Res<PipePlacementState>,
    player_query: Query<&Transform, With<Player>>,
    preview_query: Query<Entity, With<PipePreview>>,
    mut commands: Commands,
) {
    // Despawn old preview (also handles cleanup on deactivation)
    for entity in &preview_query {
        commands.entity(entity).despawn();
    }

    if !state.active {
        return;
    }

    let Ok(player_tf) = player_query.single() else {
        return;
    };

    spawn_dashes(
        &mut commands,
        state.start_pos,
        player_tf.translation.truncate(),
        PREVIEW_COLOR,
        true,
    );
}

// ── Dash rendering ──────────────────────────────────────────────────────────

/// Spawn a series of dashes along a line from `start` to `end`.
fn spawn_dashes(commands: &mut Commands, start: Vec2, end: Vec2, color: Color, is_preview: bool) {
    let diff = end - start;
    let total_length = diff.length();
    if total_length < 1.0 {
        return;
    }
    let direction = diff / total_length;
    let angle = direction.y.atan2(direction.x);

    let step = DASH_LENGTH + DASH_GAP;
    let dash_count = (total_length / step).ceil() as usize;

    for i in 0..dash_count {
        let offset = i as f32 * step;
        let dash_len = (total_length - offset).min(DASH_LENGTH);
        if dash_len <= 0.0 {
            break;
        }
        let center = start + direction * dash_len.mul_add(0.5, offset);

        let mut entity = commands.spawn((
            PipeDash,
            Sprite {
                color,
                custom_size: Some(Vec2::new(dash_len, DASH_WIDTH)),
                ..default()
            },
            Transform::from_xyz(center.x, center.y, 0.5)
                .with_rotation(Quat::from_rotation_z(angle)),
        ));

        if is_preview {
            entity.insert(PipePreview);
        }
    }
}
