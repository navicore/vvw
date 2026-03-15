# Sound Wave Visuals

## Intent

Visually depict the audio the player is hearing as they move through the maze. Each audible track source emits subtle pulsing arcs — like `)))` — that radiate toward the player, conveying volume and direction. As the player walks between sources, the visuals blend and shift, making the spatial audio experience visible.

This is an experimental/aesthetic feature. It makes the invisible (audio mixing) tangible and gives artists another creative dimension for their albums.

Depends on: background-artwork.md (artist overlays provide the visual context these arcs live on top of).

## Constraints

- Must not obscure gameplay — visuals should enhance, not obstruct the maze
- Must not affect audio behavior — purely visual, reads `TrackAudioState` but doesn't modify it
- Must not regress frame rate — lightweight custom meshes only
- Lighting must still work correctly over/under the visuals
- Must be optional per-album (boolean toggle in album metadata)
- Subtle by default — avoid filling the screen with waves
- Out of scope: frequency analysis, beat detection, waveform rendering from actual audio data
- Out of scope (future): multiple rendering styles, per-track style assignment, intensity tuning

## Approach

### Data source

`TrackAudioState` already provides per-track `current_gain` (0.0–1.0) and `current_pan` (-1.0–1.0) every frame. `TrackIcon` provides the world position of each source. The player position is known. This is everything needed — no audio analysis required.

### Configuration

A single boolean field on `AlbumMetadata`:

```rust
#[serde(default)]
pub sound_visuals: bool,
```

Default `false` — existing albums are unaffected. Set `true` in album section of `project.ron` to enable.

### Rendering: pulsing arcs `)))`

Each audible track source emits up to 3 concentric arc bands that radiate outward from the source toward the player:

- **Shape**: Each arc is a thin curved band (custom `Mesh2d` triangle strip), not a filled shape. Sweep and radius grow with distance from source: inner 60°/r6, middle 120°/r12, outer 180°/r18 — so curvature flattens naturally like real sound waves.
- **Color**: Black, high opacity (base alpha 0.85) for strong visibility against maze backgrounds.
- **Count scales with proximity**: ~1 arc when the player is far (low gain), up to ~3 when close (high gain). Louder = more visible activity, but never overwhelming.
- **Reach**: Arcs stay within ~0.75 tiles of the source. The player remains outside the arc formation even when close.
- **Vibration**: Fast in-place vibration (10 rad/s) with tight amplitude (8% of lane spacing). Each arc vibrates in its own lane without crossing into neighbors.
- **Direction**: Arcs rotate to face the player, so the open side of each `)` always points toward the listener.
- **Visibility gate**: Only rendered when `current_gain > 0.05`. Below threshold, arcs are fully hidden.

### Rendering details

- Custom triangle-strip mesh: inner/outer radius vertices along an arc, 12 segments per arc
- `Mesh2d` + `ColorMaterial` entities spawned as children of each `TrackIcon` entity
- All 3 arcs pre-spawned at maze init; visibility toggled per frame (no spawn/despawn churn)
- Z-layer between track icons (z=1) and player (z=2): local z=0.5 on parent at z=1 gives world z=1.5
- A system running `.after(SpatialAudioSet)` reads `TrackAudioState` and updates arc visibility, opacity, scale, rotation, and position each frame
- When `sound_visuals` is `false`, the `run_if` guard prevents systems from running and no meshes are spawned — zero cost

## Domain Events

- **Consumed**: `TrackAudioState` (gain, pan), `TrackIcon` (position, track_id), player `Transform`
- **Produced**: visual-only mesh entities — no game state changes
- No new custom events needed

## Checkpoints

1. Walk toward a track source — 1 pulsing arc appears, grows to 2–3 as volume increases
2. Walk away — arcs reduce in count and fade, disappearing below gain threshold
3. Stand between two sources — both emit `)))` arcs toward the player, blending naturally
4. Arcs rotate to track the player's position relative to each source
5. Outer arcs are longer and flatter than inner arcs (radiating wave effect)
6. Disable (`sound_visuals: false` or omit) — no visuals, no performance cost
7. Frame rate unchanged on a 10-track maze with visuals enabled
