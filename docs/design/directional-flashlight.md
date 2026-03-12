# Directional Flashlight Mode

## Intent

The current radial lantern lights equally in all directions. In a 2D maze
this removes tension — the player can see everything nearby without
committing to a direction. A narrow flashlight cone forces the player to
aim their light, making exploration feel more deliberate and atmospheric.
Audio becomes the primary discovery sense since tracks behind the player
are invisible.

This should be a per-album choice in `project.ron`, not a replacement for
the lantern. Some albums benefit from open visibility; others want darkness.

## Constraints

- **Both modes must coexist.** A config flag in `LightingConfig` selects
  lantern (default, backwards-compatible) or flashlight. Existing albums
  are unaffected.
- **Don't break spatial audio.** Gain/pan uses distance + LOS to track
  positions — independent of the player's light direction.
- **Touch controls must work.** The D-pad currently maps to cardinal
  directions for movement. In flashlight mode, Left/Right rotate and
  Up/Down move forward/backward relative to heading. The D-pad needs to
  drive the same actions.
- **vvw-light changes must be minimal.** Add cone support to the renderer
  (direction + half-angle on `PointLight2d`), not a separate light type.
- **No unsafe code.**

**Out of scope:** Variable cone width at runtime, multiple flashlights,
flashlight toggle (on/off), mouse/touch aiming.

## Approach

### Config (`vvw-core/src/lighting.rs`)

Add to `LightingConfig`:
```
player_light_mode: LightMode,  // Lantern (default) or Flashlight
flashlight_half_angle: f32,    // ~15° = 30° total spread
```

`#[serde(default)]` for backwards compat.

### Light component (`vvw-light/src/components.rs`)

Add optional fields to `PointLight2d`:
```
direction: Option<Vec2>,       // None = omnidirectional (lantern)
half_angle_cos: Option<f32>,   // cos(half_angle), precomputed
```

### Renderer (`vvw-light/src/render.rs`)

In the per-tile brightness loop, after distance and LOS checks, if
`direction` is `Some`, compute the angle between the light-to-tile vector
and the light direction. Skip tiles outside the cone (dot product <
`half_angle_cos`). Smooth falloff near cone edges via lerp between
`half_angle_cos` and a slightly wider cutoff.

### Player heading (`vvw-game/src/player.rs`)

Add a `PlayerHeading(Vec2)` component (unit vector, default `Vec2::Y`).

**Lantern mode (current):** Left/Right/Up/Down map to world-space
cardinal directions. `PlayerHeading` is unused by movement but still
updated to match the last nonzero velocity direction (so the light
points where you're going).

**Flashlight mode:** Left/Right rotate `PlayerHeading` (angular velocity,
e.g. 3 rad/s). Up applies velocity in `heading` direction. Down applies
velocity in `-heading` direction. The heading drives the light's
`direction` field.

### Player light sync (`vvw-game/src/player.rs`)

A system writes `PlayerHeading` into the child `PointLight2d.direction`
each frame. In lantern mode, `direction` stays `None`.

### Touch controls (`vvw-game/src/touch.rs`)

Same physical buttons (Up/Down/Left/Right). In flashlight mode the
meaning changes (rotate vs strafe) but the `DPadButton.direction` values
and `Interaction` handling stay the same. The interpretation happens in
`handle_player_input` / `handle_touch_input` based on the active mode.

### Rotation display

Rotate the player sprite to match `PlayerHeading` so the player visually
faces the flashlight direction. `Transform.rotation` already exists on
the player entity.

## Domain Events

| Event | Producer | Consumer |
|-------|----------|----------|
| `PlayerHeading` change | `handle_player_input` / `handle_touch_input` | Light sync system, sprite rotation |
| `PointLight2d.direction` | Light sync system | `update_lightmap` renderer |
| `LightingConfig` read | Startup / `apply_lighting_config` | Player plugin (selects input mode) |

No new Bevy events. Heading is a component, read each frame.

## Checkpoints

- [ ] Lantern mode unchanged — existing albums look and play identically
- [ ] Flashlight mode: cone visible, dark outside the beam
- [ ] Left/Right rotate the beam smoothly, Up/Down move forward/backward
- [ ] D-pad on mobile works in both modes
- [ ] Cone respects occluders (walls block the beam)
- [ ] Bilinear filtering smooth at cone edges (no hard pixel steps)
- [ ] `project.ron` with `player_light_mode: Flashlight` activates the mode
- [ ] `just ci` passes
