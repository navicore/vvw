# World Boundary — Exploration

## Intent

When the player steps off the outer edge of the maze, there is no ground — they fly into the void. This is most jarring in 3D where the player floats in empty space with no visual or mechanical consequence.

Stepping off the world should be a fatal fall. The game freezes input, the 3D camera pulls back to show the maze platform receding above as the player falls infinitely, and then the game resets to the starting position. In 2D the visual can be simpler (fade-out + respawn). This applies to both floor-level and elevated players.

**Why.** The maze is a contained world. Leaving it should feel significant — a boundary with consequences, not a bug. The infinite-fall camera makes the world feel like a floating island, which reinforces the surreal atmosphere.

## Constraints

- **No new tile types.** Boundary detection uses existing maze dimensions (`width`, `height`), not special tiles.
- **Out of scope:** invisible walls, rubber-banding back to the edge, partial damage, lives/health system.
- **Must not break:** wall walking, flashlight, sound piping, breadcrumbs, mute, 3D toggle.
- **Audio during fall:** all track gains fade to zero over ~1s. No abrupt cut.

## Approach

### Detection

A system checks whether the player's world position is outside the maze rect (`0..width*TILE_SIZE`, `0..height*TILE_SIZE`). This catches both floor-level escapes (currently impossible without wall-walking, but defensive) and elevated escapes off the outer wall edge.

When detected, fire `PlayerFellOffWorld`.

### Fall sequence

1. **Freeze input.** Set a `Falling` state resource. Movement systems check it and bail (same pattern as `suppresses_movement`).
2. **Audio fade.** Multiply all `TrackAudioState.target_gain` by a factor that decays from 1.0 to 0.0 over ~1s.
3. **3D camera.** Detach from player. Pull back and up, keeping the maze platform centered, while the player entity continues to "fall" (translate Y downward each frame). The maze shrinks into the distance.
4. **2D camera.** Zoom out slightly + fade to black.
5. **Reset.** After ~2-3s, respawn the player at `PlayerStart`, restore camera, restore `Falling` state, restore audio.

### Collision layer note

The outer wall ring has `GameLayer::Floor` colliders, so floor-level players can't escape. Elevated players pass through walls (that's the feature). The boundary system is the safety net for elevated players on the outer edge — and future-proofs against any other escape vectors.

## Domain Events

| Event | Producer | Consumer |
|-------|----------|----------|
| `PlayerFellOffWorld` | Boundary check system | Fall sequence (input freeze, camera, audio fade, reset timer) |
| `PlayerRespawned` | Reset system (after timer) | Restore camera, unfreeze input, restore audio |

## Decided

- **No invisible walls.** The fall is the consequence. Walls at the edge would break the "open rooftop" feel of wall walking.
- **Respawn, not game-over.** This is a music explorer. Death is a brief interruption, not a punishment. No score loss, no menu screen.
- **Same behavior in 2D and 3D.** Detection is identical. Only the camera animation differs.

## Open Questions

- **Fall duration.** 2-3s feels right but needs testing. Too short = jarring. Too long = boring.
- **Visual during 2D fall.** Fade-to-black is simple. Alternatives: zoom out to show the maze shrinking, or a brief "falling" sprite animation.

## Checkpoints

- [ ] Player walking off outer wall edge in 3D triggers fall sequence
- [ ] Camera pulls back to show maze receding during fall
- [ ] Audio fades to silence during fall
- [ ] Player respawns at start position after fall completes
- [ ] Input is frozen during fall, restored after respawn
- [ ] Floor-level player cannot escape (outer wall colliders block)
- [ ] 2D fall has visual feedback (fade or zoom)
- [ ] All existing tests pass
