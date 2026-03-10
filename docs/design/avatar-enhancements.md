# Design: Avatar Enhancements

## Intent

The player avatar is a solid-colored square that's 80% of a tile
(`TILE_SIZE * 0.8 = 25.6` units). Corridors can be 1 tile wide (32 units),
leaving only 3.2 units clearance per side. The result: the avatar constantly
jams in narrow corridors and feels like pushing a crate, not exploring a maze.

Goals:
1. **Smaller, character-shaped avatar** that fits comfortably in 1-tile corridors
2. **Running animation** — the character should visibly run when moving
3. **Art pipeline** — sprite sheet asset loading that works on both desktop and WASM

## Constraints

- **Don't touch vvw-game plugin interface.** `Player`, `PlayerMovement`,
  `PlayerLight` components stay. Physics config stays in `player.rs`.
- **WASM size budget.** A small sprite sheet (a few KB) is fine. Don't pull in
  a sprite animation crate if Bevy's built-in `TextureAtlas` suffices.
- **Collider must shrink with the visual.** Currently both sprite and collider
  are `TILE_SIZE * 0.8`. If the visual shrinks, the collider must match.
- **Out of scope:** NPC characters, customizable skins, multiplayer avatars.

## Approach

### Phase 1: Shrink the avatar (no art needed)

Reduce the player size factor from `0.8` to `0.5` (16 units in a 32-unit tile).
Update both `custom_size` and `Collider::rectangle` in `spawn_player`. This
alone fixes the corridor-jamming problem and can ship immediately.

Consider using a circle collider (`Collider::circle(radius)`) instead of a
rectangle — circles slide along walls naturally and won't catch on corners.

### Phase 2: Sprite sheet character

Replace the colored square with a sprite sheet:
- **Asset format:** Single PNG sprite sheet with idle + 4-direction run frames
- **Size:** 16×16 or 32×32 pixel character, scaled to the player size
- **Animation:** Bevy `TextureAtlas` + `AnimationIndices` component, frame
  advance driven by velocity magnitude (standing still = idle frame,
  moving = cycle run frames)
- **Direction:** Flip sprite horizontally based on `LinearVelocity.x` sign;
  optionally use separate up/down rows for 4-directional movement

**Art sourcing options:**
- Free pixel art packs (e.g., kenney.nl, itch.io CC0 assets)
- Commission a small sprite sheet (idle + 4-dir walk/run, ~32 frames total)
- Procedurally generate a simple silhouette (stretch goal)

**Asset loading:** Bevy's `AssetServer` loads images the same way on desktop
and WASM (via Trunk's `dist/` copying). Place the sprite sheet in
`crates/vvw-web/assets/` (or a shared assets dir) and reference via
`asset_server.load("player.png")`.

### Phase 3: Polish

- Dust particles on direction change
- Slight camera lead in movement direction
- Footstep sound (optional, could use a very short sample)

## Checkpoints

- [ ] Phase 1: Avatar at `0.5` scale navigates 1-tile corridors without jamming
- [ ] Phase 1: Circle collider slides past wall corners smoothly
- [ ] Phase 2: Sprite sheet loads and displays on both desktop and WASM
- [ ] Phase 2: Run animation plays when moving, idle when stopped
- [ ] Phase 2: Character faces correct direction
- [ ] No regression in spatial audio, lighting, or track click detection
