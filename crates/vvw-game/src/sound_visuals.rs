//! Sound wave visuals — pulsing arcs radiating from audible track sources
//!
//! When enabled (`sound_visuals: true` in album metadata), each track icon
//! spawns up to 3 child arc meshes shaped like `)))`. The arcs pulse outward,
//! orient toward the player, and scale their count/opacity with the track's gain.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::audio::{SpatialAudioSet, TrackAudioState};
use crate::maze::TrackIcon;
use crate::player::Player;
use crate::tiles::TILE_SIZE;

/// Resource controlling whether sound visuals are enabled for this album.
#[derive(Resource, Default)]
pub struct SoundVisualsEnabled(pub bool);

/// Maximum arcs per track source.
const MAX_ARCS: usize = 3;

/// Gain below which visuals are fully hidden.
const GAIN_THRESHOLD: f32 = 0.05;

/// Maximum reach of arcs from source, in world units (~0.75 tiles).
const MAX_REACH: f32 = TILE_SIZE * 0.75;

/// Arc radius (distance from center to the band's midpoint).
const ARC_RADIUS: f32 = 6.0;

/// Thickness of the arc band.
const ARC_THICKNESS: f32 = 4.0;

/// Arc sweep angle in radians (~60°).
const ARC_SWEEP: f32 = std::f32::consts::FRAC_PI_3;

/// Number of segments to approximate the arc curve.
const ARC_SEGMENTS: usize = 12;

/// Base alpha for the arcs (modulated by gain and pulse).
const BASE_ALPHA: f32 = 0.85;

/// Pulse speed (radians per second).
const PULSE_SPEED: f32 = 10.0;

/// How much each arc vibrates around its fixed position (fraction of spacing).
const VIBRATE_AMPLITUDE: f32 = 0.08;

/// Marker component for sound visual arcs.
#[derive(Component)]
struct SoundArc {
    /// Which index (`0..MAX_ARCS`) this arc is within its parent track.
    index: usize,
}

/// Marker on `TrackIcon` entities that have had their arc children spawned.
#[derive(Component)]
struct SoundVisualsSpawned;

pub struct SoundVisualsPlugin;

impl Plugin for SoundVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoundVisualsEnabled>().add_systems(
            Update,
            (spawn_arcs, update_arcs.after(SpatialAudioSet))
                .run_if(|enabled: Res<SoundVisualsEnabled>| enabled.0),
        );
    }
}

/// Build an arc band mesh centered on the +Y axis.
///
/// The arc is a thin band between `radius - half_thickness` and
/// `radius + half_thickness`, sweeping `sweep` radians symmetrically
/// around the +Y direction. Looks like `)` when viewed from the right.
fn build_arc_mesh(sweep: f32, radius: f32) -> Mesh {
    let inner = radius - ARC_THICKNESS / 2.0;
    let outer = radius + ARC_THICKNESS / 2.0;
    let half_sweep = sweep / 2.0;

    let mut positions = Vec::with_capacity((ARC_SEGMENTS + 1) * 2);
    let mut uvs = Vec::with_capacity((ARC_SEGMENTS + 1) * 2);
    let mut indices = Vec::with_capacity(ARC_SEGMENTS * 6);

    for i in 0..=ARC_SEGMENTS {
        let t = i as f32 / ARC_SEGMENTS as f32;
        // Sweep from -half_sweep to +half_sweep around +Y (π/2)
        let angle = t.mul_add(-sweep, std::f32::consts::FRAC_PI_2 + half_sweep);

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        // Inner vertex
        positions.push([inner * cos_a, inner * sin_a, 0.0]);
        uvs.push([t, 0.0]);

        // Outer vertex
        positions.push([outer * cos_a, outer * sin_a, 0.0]);
        uvs.push([t, 1.0]);
    }

    for i in 0..ARC_SEGMENTS {
        let base = (i * 2) as u32;
        // Two triangles per segment
        indices.extend_from_slice(&[base, base + 1, base + 3]);
        indices.extend_from_slice(&[base, base + 3, base + 2]);
    }

    let vertex_count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; vertex_count])
    .with_inserted_indices(Indices::U32(indices))
}

/// Spawn arc children for any `TrackIcon` that doesn't have them yet.
#[allow(clippy::needless_pass_by_value)]
fn spawn_arcs(
    mut commands: Commands,
    query: Query<Entity, (With<TrackIcon>, Without<SoundVisualsSpawned>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    if query.is_empty() {
        return;
    }

    // Each arc gets a progressively longer sweep and larger radius
    // so curvature flattens naturally as waves radiate outward
    // Each arc gets a progressively longer sweep and larger radius
    // so curvature flattens naturally as waves radiate outward
    let arc_meshes: Vec<Handle<Mesh>> = (0..MAX_ARCS)
        .map(|i| {
            let scale = i as f32 + 1.0; // 1, 2, 3
            let radius = ARC_RADIUS * scale;
            meshes.add(build_arc_mesh(ARC_SWEEP * scale, radius))
        })
        .collect();
    let material_handle = materials.add(ColorMaterial::from(Color::srgba(0.0, 0.0, 0.0, 0.0)));

    for entity in &query {
        commands.entity(entity).insert(SoundVisualsSpawned);

        for (i, arc_mesh) in arc_meshes.iter().enumerate() {
            commands.entity(entity).with_child((
                Mesh2d(arc_mesh.clone()),
                MeshMaterial2d(material_handle.clone()),
                Transform::from_xyz(0.0, 0.0, 5.0),
                Visibility::Hidden,
                SoundArc { index: i },
            ));
        }
    }
}

/// Update arc transforms, visibility, and alpha each frame.
#[allow(clippy::needless_pass_by_value, clippy::type_complexity)]
fn update_arcs(
    time: Res<Time>,
    player_query: Query<&Transform, (With<Player>, Without<SoundArc>)>,
    track_query: Query<
        (&Transform, &TrackAudioState),
        (With<TrackIcon>, Without<Player>, Without<SoundArc>),
    >,
    mut arc_query: Query<
        (
            &SoundArc,
            &ChildOf,
            &mut Transform,
            &mut Visibility,
            &MeshMaterial2d<ColorMaterial>,
        ),
        Without<Player>,
    >,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();
    let t = time.elapsed_secs();

    for (arc, child_of, mut tf, mut vis, mat_handle) in &mut arc_query {
        let parent = child_of.parent();

        let Ok((track_tf, audio_state)) = track_query.get(parent) else {
            continue;
        };

        let gain = audio_state.current_gain;

        // How many arcs to show: 0 below threshold, 1/2/3 scaling with gain
        let active_count = if gain < GAIN_THRESHOLD {
            0
        } else if gain < 0.3 {
            1
        } else if gain < 0.6 {
            2
        } else {
            MAX_ARCS
        };

        if arc.index >= active_count {
            *vis = Visibility::Hidden;
            continue;
        }

        *vis = Visibility::Visible;

        let track_pos = track_tf.translation.truncate();
        let to_player = player_pos - track_pos;
        let distance = to_player.length();

        if distance < 0.01 {
            *vis = Visibility::Hidden;
            continue;
        }

        let dir = to_player / distance;

        // Rotate arc so its +Y axis (the open side of the `)`) faces the player
        let angle = dir.y.atan2(dir.x) - std::f32::consts::FRAC_PI_2;
        tf.rotation = Quat::from_rotation_z(angle);

        // Each arc has a fixed lane position; vibrates in place
        let phase = t.mul_add(PULSE_SPEED, arc.index as f32 * std::f32::consts::TAU / 3.0);
        let vibrate = phase.sin(); // -1..1

        // Fixed spacing: arc 0 at 1/3, arc 1 at 2/3, arc 2 at 3/3 of MAX_REACH
        let spacing = MAX_REACH / MAX_ARCS as f32;
        let lane_center = (arc.index as f32 + 1.0) * spacing;
        let offset_distance = (vibrate * spacing).mul_add(VIBRATE_AMPLITUDE, lane_center);
        let clamped_offset = offset_distance.min(distance * 0.8);

        tf.translation.x = dir.x * clamped_offset;
        tf.translation.y = dir.y * clamped_offset;

        // Subtle scale vibration
        let pulse01 = (vibrate + 1.0) * 0.5;
        let scale_factor = 0.1f32.mul_add(pulse01, 0.9);
        tf.scale = Vec3::splat(scale_factor);

        // Alpha: modulate with gain, slight flicker from vibration
        let alpha = BASE_ALPHA * gain.min(1.0) * 0.15f32.mul_add(pulse01, 0.85);
        if let Some(mat) = materials.get_mut(mat_handle.id()) {
            mat.color = Color::srgba(0.0, 0.0, 0.0, alpha);
        }
    }
}
