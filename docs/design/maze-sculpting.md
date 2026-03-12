# Design: Maze Sculpting — User-Drawn Corridors & Walls

## Intent

Let the player reshape the maze at runtime to create new audio mixing possibilities.

**Part A — Carve corridors.** The player draws a path (finger or mouse) between two points, converting wall tiles to floor. This opens line-of-sight between tracks that were previously isolated, creating new spatial audio blends that the album author never explicitly designed. The creative act shifts from passive listening to active sound sculpting.

**Part B — Place walls.** The player taps/clicks a floor tile to convert it to a wall, blocking sound propagation. This lets the player isolate a track or cut off a corridor to focus a mix.

**Volume control.** Long carved corridors attenuate sound heavily (distance-to-gain curve). A global mix volume slider lets the player boost overall gain when blending distant tracks through a long custom corridor.

**Why.** The maze is currently read-only — the album author decides what mixes are possible. Sculpting makes the player a co-author of the mix. This is the core differentiator: a music player where the listener shapes the soundscape.

## Constraints

- **Don't break existing spatial audio.** `has_line_of_sight` and distance gain already work on the `Maze` grid. Sculpting mutates the grid; spatial audio reacts automatically. No changes needed to LOS or gain math.
- **Don't break physics.** Wall colliders must be added/removed to match tile changes. Avian2d bodies for affected tiles need despawn/respawn or toggling.
- **Don't break lighting.** `OccluderGrid` must be updated when tiles change. Light will naturally flow through carved corridors.
- **Structural tiles are sacred.** `PlayerStart` and `TrackIcon` tiles cannot be overwritten. Outer boundary walls cannot be removed.
- **No networking.** This is single-player, local-only for now.
- **Out of scope:** maze save/load (noted as future work), undo history, multiplayer shared sculpting, procedural corridor suggestions.

## Approach

### Maze mutation API (vvw-core)

Add to `Maze`:
- `carve_tile(x, y)` — set tile to `Floor` if currently `Wall` and not a boundary tile. Returns bool (changed).
- `place_wall(x, y)` — set tile to `Wall` if currently `Floor` (not `PlayerStart` or `TrackIcon`). Returns bool.

These are the only two mutation primitives. All higher-level features (draw-a-corridor, erase-a-wall) compose on top.

### Draw interaction (vvw-game)

New `SculptPlugin`:
- **Carve mode:** touch-drag or mouse-drag. On each frame, raycast from pointer into the tile grid. For each wall tile under the drag path, call `maze.carve_tile()`. Emit a `TileChanged { pos, new_kind }` event.
- **Wall mode:** tap/click a floor tile. Call `maze.place_wall()`. Emit `TileChanged`.
- Mode toggle via UI button or gesture (e.g., long-press to switch).

### Reactive systems (vvw-game)

Systems that respond to `TileChanged` events:
- **Tile sprite sync** — update the sprite/color of the changed tile entity.
- **Collider sync** — spawn a wall collider (or despawn one) on the affected entity.
- **Occluder sync** — update `OccluderGrid` so lighting recomputes.

Spatial audio needs no changes — `has_line_of_sight` already reads the `Maze` resource each frame. Carving a corridor immediately opens LOS; placing a wall immediately blocks it.

### Volume control

Add a `mix_volume: f32` field to a new `MixConfig` resource. The `interpolate_audio_state` system multiplies final gain by this value. Expose via a simple slider in the web overlay (similar to the existing track info bar).

## Domain Events

| Event | Producer | Consumer |
|---|---|---|
| `TileChanged { pos, new_kind }` | `SculptPlugin` (on draw/tap) | Tile sprite sync, collider sync, occluder sync |
| `MixVolumeChanged(f32)` | UI slider | `interpolate_audio_state` (gain multiplier) |

No new events needed for spatial audio — LOS is recomputed from the live `Maze` grid automatically.

## Checkpoints

- [ ] `Maze::carve_tile` / `place_wall` unit tests — verify boundary protection, TrackIcon protection, idempotency
- [ ] Carve a wall tile in-game — verify sprite updates, collider removed, player can walk through, light passes through
- [ ] Carve a corridor between two isolated tracks — verify audio fades in as LOS opens
- [ ] Place a wall in a corridor — verify audio cuts off, player blocked, light occluded
- [ ] Volume slider — verify gain boost lets distant blended tracks become audible
- [ ] Existing albums with no sculpting interaction — verify zero behavioral change
