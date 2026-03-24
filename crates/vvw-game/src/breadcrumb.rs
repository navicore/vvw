//! Breadcrumb recording & replay — record a timed path, walk it as an endless loop
//!
//! Two interaction modes:
//! - **Lay Trail** (`suppresses_movement: false`): records position + heading at 10 Hz
//! - **Walk Trail** (`suppresses_movement: true`): replays the trail backward first
//!   (end → start), then reverses at each end for an endless back-and-forth loop.
//!
//! Audio is not recorded — spatial mixing happens live from player `Transform`.

use avian2d::prelude::*;
use bevy::prelude::*;

use vvw_core::modes::{ModeDescriptor, ModeId};

use crate::modes::{ActiveMode, ModeRegistry};
use crate::player::{Player, PlayerHeading};
use crate::wall_walking::Elevated;

const LAY_TRAIL_MODE_ID: &str = "lay_trail";
const WALK_TRAIL_MODE_ID: &str = "walk_trail";

/// Sample interval: 10 Hz
const SAMPLE_INTERVAL: f32 = 0.1;

/// Maximum number of samples (10 min at 10 Hz)
const MAX_SAMPLES: usize = 6_000;

/// Visual dot size (world units)
const DOT_SIZE: f32 = 4.0;
/// Dot color during recording
const DOT_COLOR: Color = Color::srgba(0.9, 0.7, 0.2, 0.6);
/// Dot color during replay (slightly brighter)
const DOT_REPLAY_COLOR: Color = Color::srgba(0.9, 0.7, 0.2, 0.8);

// ── Data Model ──────────────────────────────────────────────────────────────

/// A single recorded sample along the trail.
struct Breadcrumb {
    position: Vec2,
    heading: Vec2,
    elapsed: f32,
}

/// The recorded trail.
#[derive(Default)]
struct BreadcrumbTrail {
    samples: Vec<Breadcrumb>,
}

impl BreadcrumbTrail {
    fn is_valid(&self) -> bool {
        self.samples.len() >= 2
    }

    fn duration(&self) -> f32 {
        self.samples.last().map_or(0.0, |s| s.elapsed)
    }

    /// Interpolate position and heading at a given elapsed time.
    /// Clamps to trail bounds.
    fn sample_at(&self, elapsed: f32) -> (Vec2, Vec2) {
        let samples = &self.samples;
        if samples.is_empty() {
            return (Vec2::ZERO, Vec2::Y);
        }
        if elapsed <= samples[0].elapsed {
            return (samples[0].position, samples[0].heading);
        }
        if elapsed >= samples[samples.len() - 1].elapsed {
            let last = &samples[samples.len() - 1];
            return (last.position, last.heading);
        }

        // Binary search for the bracketing pair
        let idx = samples
            .partition_point(|s| s.elapsed < elapsed)
            .min(samples.len() - 1);
        let b = &samples[idx];
        if idx == 0 {
            return (b.position, b.heading);
        }
        let a = &samples[idx - 1];

        let range = b.elapsed - a.elapsed;
        if range < f32::EPSILON {
            return (b.position, b.heading);
        }
        let t = (elapsed - a.elapsed) / range;

        let pos = a.position.lerp(b.position, t);
        // Lerp heading and re-normalize
        let hdg = a.heading.lerp(b.heading, t).normalize_or(Vec2::Y);
        (pos, hdg)
    }
}

// ── Resources ───────────────────────────────────────────────────────────────

/// Breadcrumb system state.
#[derive(Resource)]
struct BreadcrumbState {
    phase: BreadcrumbPhase,
    trail: BreadcrumbTrail,
    /// Number of samples taken (used for monotonic elapsed timestamps)
    sample_count: u32,
    /// Accumulated time since last sample
    sample_timer: f32,
    /// Cursor position during replay (in trail elapsed-time space)
    replay_cursor: f32,
    /// True when cursor is moving backward (end → start)
    replay_backward: bool,
}

enum BreadcrumbPhase {
    Idle,
    Recording,
    Playing,
}

impl Default for BreadcrumbState {
    fn default() -> Self {
        Self {
            phase: BreadcrumbPhase::Idle,
            trail: BreadcrumbTrail::default(),
            sample_count: 0,
            sample_timer: 0.0,
            replay_cursor: 0.0,
            replay_backward: true, // first pass walks backward
        }
    }
}

// ── Components ──────────────────────────────────────────────────────────────

/// Marker for breadcrumb dot entities.
#[derive(Component)]
struct BreadcrumbDot;

// ── Plugin ──────────────────────────────────────────────────────────────────

pub struct BreadcrumbPlugin;

impl Plugin for BreadcrumbPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BreadcrumbState>()
            .add_systems(Startup, register_breadcrumb_modes);

        app.add_systems(
            Update,
            (watch_mode_changes, record_samples, replay_trail)
                .chain()
                .run_if(resource_changed::<ActiveMode>.or(breadcrumb_active)),
        );
    }
}

fn breadcrumb_active(state: Res<BreadcrumbState>) -> bool {
    !matches!(state.phase, BreadcrumbPhase::Idle)
}

fn register_breadcrumb_modes(mut registry: ResMut<ModeRegistry>) {
    registry.register(ModeDescriptor {
        id: ModeId(LAY_TRAIL_MODE_ID.into()),
        label: "Lay Trail".into(),
        suppresses_movement: false,
        order: 100,
    });
    registry.register(ModeDescriptor {
        id: ModeId(WALK_TRAIL_MODE_ID.into()),
        label: "Walk Trail".into(),
        suppresses_movement: true,
        order: 101,
    });
}

// ── Mode Lifecycle ──────────────────────────────────────────────────────────

fn watch_mode_changes(
    active: Res<ActiveMode>,
    mut state: ResMut<BreadcrumbState>,
    player_query: Query<(&Transform, &PlayerHeading), With<Player>>,
    dot_query: Query<Entity, With<BreadcrumbDot>>,
    mut commands: Commands,
) {
    if !active.is_changed() {
        return;
    }

    let active_id = active.0.as_ref().map(|id| id.0.as_str());

    match active_id {
        Some(LAY_TRAIL_MODE_ID) => {
            if matches!(state.phase, BreadcrumbPhase::Recording) {
                return;
            }

            // Start recording — clear any previous trail and dots
            for entity in &dot_query {
                commands.entity(entity).despawn();
            }
            state.trail = BreadcrumbTrail::default();
            state.sample_count = 0;
            state.sample_timer = 0.0;
            state.phase = BreadcrumbPhase::Recording;

            // Take the first sample immediately
            if let Ok((tf, heading)) = player_query.single() {
                state.trail.samples.push(Breadcrumb {
                    position: tf.translation.truncate(),
                    heading: heading.0,
                    elapsed: 0.0,
                });
            }

            info!("Breadcrumb: recording started");
        }

        Some(WALK_TRAIL_MODE_ID) => {
            if matches!(state.phase, BreadcrumbPhase::Playing) {
                return;
            }

            // Handle direct transition from recording
            if matches!(state.phase, BreadcrumbPhase::Recording) {
                state.phase = BreadcrumbPhase::Idle;
                info!(
                    "Breadcrumb: recording stopped — {} samples, {:.1}s",
                    state.trail.samples.len(),
                    state.trail.duration()
                );
            }

            if !state.trail.is_valid() {
                info!("Breadcrumb: no trail to walk");
                return;
            }

            // Start replay — cursor at end, walking backward
            state.replay_cursor = state.trail.duration();
            state.replay_backward = true;
            state.phase = BreadcrumbPhase::Playing;

            // Update dot colors for replay
            for entity in &dot_query {
                commands.entity(entity).insert(Sprite {
                    color: DOT_REPLAY_COLOR,
                    custom_size: Some(Vec2::splat(DOT_SIZE)),
                    ..default()
                });
            }

            info!("Breadcrumb: replay started");
        }

        _ => {
            // Mode deactivated or switched to something else
            match state.phase {
                BreadcrumbPhase::Recording => {
                    state.phase = BreadcrumbPhase::Idle;
                    if state.trail.is_valid() {
                        info!(
                            "Breadcrumb: recording stopped — {} samples, {:.1}s",
                            state.trail.samples.len(),
                            state.trail.duration()
                        );
                    } else {
                        info!("Breadcrumb: recording discarded (too short)");
                        state.trail = BreadcrumbTrail::default();
                        for entity in &dot_query {
                            commands.entity(entity).despawn();
                        }
                    }
                }
                BreadcrumbPhase::Playing => {
                    state.phase = BreadcrumbPhase::Idle;
                    // Restore dot colors
                    for entity in &dot_query {
                        commands.entity(entity).insert(Sprite {
                            color: DOT_COLOR,
                            custom_size: Some(Vec2::splat(DOT_SIZE)),
                            ..default()
                        });
                    }
                    info!("Breadcrumb: replay stopped");
                }
                BreadcrumbPhase::Idle => {}
            }
        }
    }
}

// ── Recording ───────────────────────────────────────────────────────────────

fn record_samples(
    time: Res<Time>,
    mut state: ResMut<BreadcrumbState>,
    player_query: Query<(&Transform, &PlayerHeading, &Elevated), With<Player>>,
    mut commands: Commands,
) {
    if !matches!(state.phase, BreadcrumbPhase::Recording) {
        return;
    }

    // Don't record while on walls
    if player_query.single().is_ok_and(|(_, _, e)| e.0) {
        return;
    }

    state.sample_timer += time.delta_secs();

    if state.sample_timer < SAMPLE_INTERVAL {
        return;
    }

    if state.trail.samples.len() >= MAX_SAMPLES {
        return;
    }

    // Drain accumulated intervals (handles frame spikes) but record one sample
    let pending = (state.sample_timer / SAMPLE_INTERVAL) as u32;
    state.sample_timer -= pending as f32 * SAMPLE_INTERVAL;
    state.sample_count += pending;

    let Ok((tf, heading, _)) = player_query.single() else {
        return;
    };

    let pos = tf.translation.truncate();
    let elapsed = state.sample_count as f32 * SAMPLE_INTERVAL;

    state.trail.samples.push(Breadcrumb {
        position: pos,
        heading: heading.0,
        elapsed,
    });

    // Spawn a visual dot at this sample point
    commands.spawn((
        BreadcrumbDot,
        Sprite {
            color: DOT_COLOR,
            custom_size: Some(Vec2::splat(DOT_SIZE)),
            ..default()
        },
        Transform::from_xyz(pos.x, pos.y, 0.3),
    ));
}

// ── Replay ──────────────────────────────────────────────────────────────────

fn replay_trail(
    time: Res<Time>,
    mut state: ResMut<BreadcrumbState>,
    mut player_query: Query<
        (
            &mut Position,
            &mut LinearVelocity,
            &mut PlayerHeading,
            &Elevated,
        ),
        With<Player>,
    >,
) {
    if !matches!(state.phase, BreadcrumbPhase::Playing) {
        return;
    }

    // Don't replay while on walls
    if player_query.single().is_ok_and(|(_, _, _, e)| e.0) {
        return;
    }

    let dt = time.delta_secs();
    let duration = state.trail.duration();

    if duration < f32::EPSILON {
        return;
    }

    // Advance cursor with clamped bounce at trail ends
    if state.replay_backward {
        state.replay_cursor -= dt;
        if state.replay_cursor <= 0.0 {
            state.replay_cursor = (-state.replay_cursor).min(duration);
            state.replay_backward = false;
        }
    } else {
        state.replay_cursor += dt;
        if state.replay_cursor >= duration {
            state.replay_cursor = 2.0f32.mul_add(duration, -state.replay_cursor).max(0.0);
            state.replay_backward = true;
        }
    }

    let (pos, hdg) = state.trail.sample_at(state.replay_cursor);

    // When walking backward, reverse the heading
    let effective_heading = if state.replay_backward { -hdg } else { hdg };

    let Ok((mut position, mut velocity, mut heading, _)) = player_query.single_mut() else {
        return;
    };

    position.0 = pos;
    velocity.0 = Vec2::ZERO;
    heading.0 = effective_heading.normalize_or(Vec2::Y);
}
