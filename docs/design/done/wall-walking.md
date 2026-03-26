# Wall Walking — Exploration

## Intent

Let the player jump onto walls and traverse the maze from above. On the wall, the player hears all tracks at 20% gain (no wall occlusion) — enough to survey the soundscape but not enough to replace the corridor listening experience. Moving over a wall edge drops the player back to the floor. Works in both 2D and 3D.

**Why.** The maze is a mixing instrument. Wall walking adds a "balcony" perspective — a lo-fi preview of the whole album that rewards exploration without undermining the immersive corridor experience. It also creates a navigation shortcut with a deliberate audio tradeoff. The reduced volume incentivizes returning to the floor and using pipes to build the ultimate mix at ground level.

## Constraints

- **Same spatial math.** Gain curve and pan formula stay the same. The only change is: (1) LOS ignores walls when player is elevated, and (2) a 0.2 gain multiplier applies.
- **No new physics engine.** Avian2d stays. Movement on walls is still 2D plane movement with modified collision rules.
- **No new tile types.** Wall tiles remain walls. Elevation is player state, not map state.
- **Out of scope:** double-jump, variable height, wall-to-wall jumping, multiplayer sync, new audio effects (reverb, muffling).
- **Must not break:** existing albums, flashlight mode, sound visuals, piping, breadcrumbs, mute mode.
- **Album opt-in.** `wall_walking: bool` in `AlbumMetadata` (default false). Existing albums unaffected.

## Approach

### Player state

New component `Elevated(bool)` on the player entity. When true:

- **Collision layer swap.** Player collides with floor tiles (preventing falling into corridors) instead of wall tiles. Avian2d collision layers make this a bitmask toggle — no collider respawning needed.
- **Audio modifier.** `compute_spatial_targets` skips the `blocks_sight` check and multiplies final gain by 0.2.
- **Visual.** 2D: player sprite gets a drop shadow or slight scale-up to indicate elevation. 3D: player camera rises to `WALL_HEIGHT + EYE_HEIGHT`, looking down over cubicle walls.

### Jump / mount

- **Desktop:** Spacebar while adjacent to a wall = mount wall. Player position snaps to the wall tile center.
- **Mobile (primary):** Swipe up on the D-pad up button (~30px vertical drag within ~300ms). Distinct from a normal press (which just moves the player). Feels natural: "flick up to jump."
- **Mobile (bonus):** Device shake on Android. `DeviceMotionEvent` fires without a permission prompt on Android Chrome. iOS Safari requires `requestPermission()` from a user gesture, so shake is silently unavailable there — falls back to swipe-up. `vvw-web` registers a `devicemotion` listener (only if the API exists without a permission gate), computes acceleration magnitude, and sends a `ShakeDetected` event into the Bevy world.
- **Mobile (last resort):** Double-tap the D-pad up button within ~400ms. Available if swipe-up proves awkward in testing.

Implementation: a system checks (1) mount input active (spacebar, swipe-up, shake, or double-tap), (2) player is adjacent to a wall tile. If both true, set `Elevated(true)`, snap position to the nearest adjacent wall tile center, swap collision layer. Desktop spacebar does not require a directional input — proximity to any adjacent wall is sufficient.

### Movement on walls

Standard D-pad / arrow key movement. Player can move to any adjacent wall tile. If the player moves toward a tile that is NOT a wall, they fall: `Elevated(false)`, collision layer reverts, player lands in the corridor tile. In 3D, a brief camera drop animation.

### Audio when elevated

The `has_line_of_sight` function in `spatial.rs` currently walks tiles via Bresenham and fails on any wall tile. When `Elevated` is true, the spatial audio system bypasses LOS entirely (elevated player sees over all walls) and applies a flat `0.2` multiplier to the distance-based gain. This means:

- Close tracks are audible but quiet (preview quality).
- Distant tracks still fall off normally (just 5x quieter).
- The corridor remains the premium listening position.

No changes to pan calculation — horizontal angle is the same regardless of elevation.

### Collision layer design

Two layers: `FLOOR_LAYER` (existing wall colliders) and `WALL_LAYER` (new colliders on floor/corridor tiles, spawned at setup but disabled by default). When `Elevated` toggles:

- `Elevated(true)`: disable `FLOOR_LAYER` colliders, enable `WALL_LAYER` colliders.
- `Elevated(false)`: reverse.

Alternative (simpler): skip the second collider set entirely. When elevated, just clamp movement to wall tiles in a system that runs after physics. If the player's tile position resolves to a non-wall tile, force a fall. This avoids doubling collider count.

**Recommendation:** Start with the clamp approach. It's fewer entities, simpler to debug, and wall-walking movement doesn't need physics-grade collision (no bouncing off wall edges).

## Domain Events

| Event | Producer | Consumer |
|-------|----------|----------|
| `PlayerElevated` | Jump system (input + adjacency check) | Audio modifier, visual indicator, 3D camera, collision swap |
| `PlayerFell` | Movement clamp system (non-wall tile detected) | Same consumers in reverse |

No new audio events. `TrackAudioState` pipeline is unchanged — only the gain input is modified.

## Decided

- **Pipes not audible from walls.** Pipe speakers are floor-level entities. Elevated player does not hear them. This reinforces the floor as the mixing surface.
- **Breadcrumbs disabled while elevated.** No recording or replaying trails on walls. Avoids elevation-transition complexity and keeps breadcrumbs a floor activity.
- **Sound visuals suppressed while elevated.** Arc animations are floor-level; showing them from above would need rework for little benefit.
- **No fall penalty.** This is a music explorer, not a platformer. Falling is just a state transition back to corridor listening.

## Open Questions

- **Swipe-up threshold tuning.** ~30px / ~300ms is a starting guess. May need adjustment after device testing to avoid false positives during normal D-pad use.
- **Shake sensitivity.** Acceleration magnitude threshold TBD. Must be high enough to avoid false triggers from walking/transit, low enough to feel responsive.

## Checkpoints

- [ ] `Elevated` component toggles on spacebar+up when adjacent to wall (desktop)
- [ ] Player moves freely across connected wall tiles
- [ ] Player falls when moving to non-wall tile edge
- [ ] Audio gain is ~20% of floor-level gain at same distance (LOS bypassed)
- [ ] 3D camera rises to wall height on mount, drops on fall
- [ ] Existing albums with `wall_walking: false` (or absent) show no behavioral change
- [ ] Swipe-up on D-pad up button mounts wall (mobile)
- [ ] Shake mounts wall on Android (silently no-op on iOS)
- [ ] Double-tap up available as fallback if swipe-up is awkward
- [ ] All 26 existing tests still pass
