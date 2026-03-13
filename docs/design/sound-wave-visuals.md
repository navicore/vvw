# Sound Wave Visuals

## Intent

Visually depict the audio the player is hearing as they move through the maze. Each track source emits styled visual "sound lines" that convey volume, direction, and mixing — like voice lines in Nancy comics or motion lines in anime. As the player walks between sources, the visuals blend and shift, making the spatial audio experience visible.

This is an experimental/aesthetic feature. It makes the invisible (audio mixing) tangible and gives artists another creative dimension for their albums.

Depends on: background-artwork.md (artist overlays provide the visual context these lines live on top of).

## Constraints

- Must not obscure gameplay — visuals should enhance, not obstruct the maze
- Must not affect audio behavior — purely visual, reads `TrackAudioState` but doesn't modify it
- Must not regress frame rate — needs to be lightweight (sprites or simple meshes, not per-pixel shader work)
- Lighting must still work correctly over/under the visuals
- Must be optional per-album (some artists won't want it)
- Out of scope: frequency analysis, beat detection, waveform rendering from actual audio data

## Approach

### Data source

`TrackAudioState` already provides per-track `current_gain` (0.0–1.0) and `current_pan` (-1.0–1.0) every frame. `TrackIcon` provides the world position of each source. The player position is known. This is everything needed — no audio analysis required.

### Three rendering styles

Configured per-album in `project.ron` via a new optional field (e.g., `sound_visual_style: Option<SoundVisualStyle>`):

1. **Lines** — Comic-style emanation lines radiating from track icons toward the player. Thickness and opacity scale with gain. Sparse at low volume, dense at high. Think Nancy/Peanuts.

2. **Ripples** — Concentric arcs expanding outward from track icons. Ring spacing and alpha driven by gain. Subtle at distance, pronounced up close. Think anime impact rings.

3. **Particles** — Small dots or dashes drifting from source toward player along the line between them. Speed and density scale with gain. Think dust motes or pollen caught in sound.

### Rendering approach

- Spawn Bevy `Sprite` entities (or simple `Mesh2d` shapes) as children of each `TrackIcon` entity
- Z-layer between track icons (z=1) and lightmap (z=90) — e.g., z=5
- A system running `.after(SpatialAudioSet)` reads `TrackAudioState` and updates sprite opacity, scale, and count each frame
- Sprites use the track's light color (already set per-track) for visual consistency
- Below a gain threshold (e.g., 0.05), visuals are fully hidden — no clutter from distant sources

### Intensity

Each style supports a configurable intensity/subtlety level, defaulting to moderate. The artist can tune how prominent the visuals are relative to their background artwork.

## Domain Events

- **Consumed**: `TrackAudioState` (gain, pan), `TrackIcon` (position, track_id), player `Transform`
- **Produced**: visual-only sprite entities — no game state changes
- No new custom events needed

## Checkpoints

1. Walk toward a track source — visuals appear and intensify with volume
2. Walk away — visuals fade and disappear below threshold
3. Stand between two sources — both emit visuals, blending naturally
4. Switch styles in `project.ron` — each of the three looks distinct
5. Disable (`sound_visual_style: None`) — no visuals, no performance cost
6. Frame rate unchanged on a 10-track maze with visuals enabled
