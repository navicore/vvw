# Wall Walking — Exploration

## Intent

Let the player jump onto walls and traverse the maze from above. On the wall, the player hears all tracks at 20% gain (no wall occlusion) — enough to survey the soundscape but not enough to replace the corridor listening experience. Moving over a wall edge drops the player back to the floor. Works in both 2D and 3D.

**Why.** The maze is a mixing instrument. Wall walking adds a "balcony" perspective — a lo-fi preview of the whole album that rewards exploration without undermining the immersive corridor experience. It also creates a navigation shortcut with a deliberate audio tradeoff.

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

- **Desktop:** Spacebar = jump (small visual bounce, no state change). Spacebar + Up arrow while adjacent to a wall = mount wall. Player position snaps to the wall tile center.
- **Mobile:** Hold Up on D-pad + tap wall with one finger = mount. Requires touch target on the adjacent wall tile.

Implementation: a system checks (1) jump input active, (2) player is adjacent to a wall tile, (3) directional input points toward that wall. If all true, set `Elevated(true)`, snap position to wall tile, swap collision layer.

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

## Open Questions

- **Should breadcrumb recording work while elevated?** Replay would need to handle elevation transitions. Simplest: disable breadcrumb recording while on walls.
- **Should pipe placement work from walls?** Could be powerful (pipe from above to a corridor below). Could also be confusing. Start with: modes that suppress movement are unavailable while elevated.
- **Sound visuals from above?** Arc animations currently radiate from track to player at floor level. From the wall, arcs would need to angle upward or be suppressed. Simplest: suppress while elevated.
- **Fall damage / penalty?** Probably not — this is a music explorer, not a platformer. Fall is just a state transition.

## Checkpoints

- [ ] `Elevated` component toggles on spacebar+up when adjacent to wall (desktop)
- [ ] Player moves freely across connected wall tiles
- [ ] Player falls when moving to non-wall tile edge
- [ ] Audio gain is ~20% of floor-level gain at same distance (LOS bypassed)
- [ ] 3D camera rises to wall height on mount, drops on fall
- [ ] Existing albums with `wall_walking: false` (or absent) show no behavioral change
- [ ] Mobile touch input mounts wall correctly
- [ ] All 26 existing tests still pass
