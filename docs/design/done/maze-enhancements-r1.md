# Design: Maze Enhancements Round 1

## Intent

Make the maze feel more atmospheric and physically responsive.

1. **Remove track lights.** The maze should be dark — the only illumination comes
   from the player's light and ambient. Tracks become invisible until you're
   close enough to hear them; discovery is by ear, not by sight.

2. **Softer wall collisions.** Hitting a wall should produce a subtle bounce
   instead of a dead stop. The player already has `Restitution(0.3)` and walls
   have `Restitution(0.4)`, but high `LinearDamping(5.0)` kills the bounce
   before it's visible. Tuning damping (or applying a brief impulse) would let
   a small bounce register.

3. **(Future) Directional flashlight.** Replace the radial lantern with a
   cone-shaped light that follows the player's heading. Out of scope for round 1
   but the lighting system should stay compatible with it.

## Constraints

- **Don't break spatial audio.** Gain/pan still depends on distance and LOS to
  track positions — those calculations don't use lights.
- **Don't touch vvw-light internals.** Changes should be in vvw-game config
  (LightingConfig, physics params) not in the rendering pipeline.
- **Per-album tuning.** `LightingConfig` is already serialized in `project.ron`.
  Track light removal should be a config flag, not hard-coded deletion, so
  albums can opt in or out.
- **Out of scope for round 1:** directional/cone lights, track light pulsing,
  fog-of-war, minimap.

## Approach

### Track light toggle

Add a `track_lights_enabled: bool` field to `LightingConfig` (default `true`
for backwards compat). In `spawn_maze_tiles` / the track icon spawning path in
`maze.rs`, skip the `PointLight2d` child when the flag is false. The
`apply_lighting_config` system already iterates `TrackLight` entities — it can
despawn them at runtime if the flag flips, or simply never spawn them.

Alternatively: set `track_intensity` to `0.0` in `project.ron`. This already
works with current code (the light exists but contributes nothing). Simpler,
no code change, but wastes a light entity per track. Good enough for
experimentation; a proper flag can come later if needed.

### Wall bounce tuning

Current player physics (`player.rs`):
- `LinearDamping(5.0)` — high, kills momentum fast
- `Restitution(0.3)` — low bounce coefficient
- Wall `Restitution(0.4)`

To get a visible micro-bounce:
- Lower `LinearDamping` to ~2.0–3.0 (player still stops quickly without input
  but retains enough post-collision velocity for a bounce frame or two)
- Raise player `Restitution` to ~0.4–0.5
- Keep speed impulse the same; terminal velocity will increase slightly with
  lower damping — compensate by reducing `speed` proportionally

These are pure number tweaks in `player.rs`. Test iteratively in the web build.

### Ambient tuning

With track lights gone the maze will be very dark. May want to bump
`ambient_brightness` from 0.15 to ~0.08–0.12 (darker = more dramatic) or
increase `player_radius` from 100 to 120–150 to compensate. All tunable in
`LightingConfig` without code changes.

## Checkpoints

- [ ] Set `track_intensity: 0.0` in a test album's `project.ron` — verify maze
      is dark, only player light visible, audio still works
- [ ] Lower `LinearDamping` and raise `Restitution` — verify visible bounce
      off walls without feeling floaty or uncontrolled
- [ ] Gameplay still feels navigable (not too dark, not too bouncy)
- [ ] Existing albums with default lighting unchanged (backwards compat)
