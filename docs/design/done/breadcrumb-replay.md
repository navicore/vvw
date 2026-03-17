# Design: Breadcrumb Recording & Replay

## Intent

Let the player record a timed path through the maze, then walk it as an endless loop. The player lays a breadcrumb trail, then follows it backward to the start — like a real breadcrumb trail. At each end the direction reverses, creating a continuous back-and-forth loop until the player stops. Audio is not recorded; spatial mixing happens live during replay from wherever the tracks happen to be in their playback cycle. Every pass produces a slightly different mix. The path is the composition; the sound is the performance.

**Why.** The maze is an instrument. Recording a path turns an improvised walk into a repeatable piece. The back-and-forth loop creates an ambient installation from a single gesture. The non-determinism (track phase drift) means no two passes are identical — the player authored the structure but not the exact sound.

## Constraints

- **Player input is suppressed during replay.** Movement keys/D-pad are ignored while the avatar follows the breadcrumb path. The user can cancel replay at any time.
- **No audio recording.** Only position + timestamp. This keeps the data tiny and avoids media permissions.
- **No maze mutation during replay.** If maze sculpting (see `maze-sculpting.md`) is implemented, sculpt mode is locked during replay to avoid desyncing the path from the geometry.
- **Don't break spatial audio.** The replay system moves the player entity the same way input does — `TrackAudioState` reacts to position as usual. No changes to LOS, gain, or pan.
- **Two interaction modes.** Trail laying and trail walking are separate registered modes in the interaction modes framework. This keeps the gestures distinct and allows UI to show which mode is active.
- **Out of scope:** saving/loading breadcrumbs to disk, sharing paths between users, speed multiplier, editing recorded paths. Trail deletion and reset are out of scope here — they need a general "undo/delete placed things" solution that also covers pipes.

## Approach

### Data model (vvw-core or vvw-game)

```rust
struct Breadcrumb {
    position: Vec2,     // world coordinates
    heading: Vec2,      // player facing direction (for flashlight)
    elapsed: f32,       // seconds since recording started
}

struct BreadcrumbTrail {
    samples: Vec<Breadcrumb>,
}
```

Sample at a fixed rate (e.g., 10 Hz). Storing position, not input, so replay is independent of physics frame rate. Heading is included so flashlight direction replays correctly.

### Recording — "Lay Trail" mode (vvw-game)

Registered as an interaction mode (`ModeDescriptor` with `suppresses_movement: false`). When the mode is activated:

- State transitions to `Recording`. Each tick, if enough time has passed since last sample, push a `Breadcrumb`.
- A visual trail renders behind the player as crumbs are laid (small dots or markers at sample points).
- When the mode is deactivated, recording stops. Trail is stored. If fewer than 2 samples, discard silently.

### Replay — "Walk Trail" mode (vvw-game)

Registered as a separate interaction mode (`ModeDescriptor` with `suppresses_movement: true`). When the mode is activated:

- State transitions to `Playing`. Player input systems early-return when state is `Playing`.
- **First pass: walk backward.** The cursor starts at the end of the trail (where the player stopped recording) and moves toward the start, retracing the path in reverse. This matches the real-world breadcrumb metaphor — you follow your crumbs back.
- **At each end, reverse direction.** When the cursor reaches the start, it reverses and walks forward. At the end, reverse again. This creates an endless back-and-forth loop.
- **Each frame:** advance cursor by `dt`. Interpolate (lerp) between the two bracketing samples. Write interpolated position to the player's `Transform` and velocity to zero. Write heading to `PlayerHeading`. When walking backward, heading is reversed.
- **Cancel:** deactivating the mode (two-finger tap or cycle button) transitions back to `Idle` and returns control to the player at their current position.

### Visual trail

The breadcrumb trail should be visible during both recording and replay. Small dot markers at each sample point, rendered at a low z-index. During replay, the dots could pulse or fade as the player passes over them. The trail visual persists until the trail is deleted (future work).

### Why position, not input replay

Replaying recorded inputs through physics would accumulate drift (floating-point, frame-rate differences). Recording world positions and lerping guarantees the path is visually identical every time. The player entity is teleported along the path; physics velocity is zeroed to prevent collider interactions from perturbing it.

## Future: Deletion and Reset

Trail deletion, pipe deletion, and any future "placed thing" cleanup should share a general solution — likely a "delete mode" or "undo" gesture that removes the most recent placed object, or a selection-based approach. This is out of scope for this feature but should be designed before either breadcrumbs or pipes ship persistence.

## Domain Events

| Event | Producer | Consumer |
|---|---|---|
| `BreadcrumbRecordStart` | Mode activation ("Lay Trail") | `BreadcrumbState` → `Recording` |
| `BreadcrumbRecordStop` | Mode deactivation | `BreadcrumbState` → `Idle`, trail stored |
| `BreadcrumbPlayStart` | Mode activation ("Walk Trail") | `BreadcrumbState` → `Playing`, input suppressed |
| `BreadcrumbPlayStop` | Mode deactivation or end-of-trail cancel | `BreadcrumbState` → `Idle`, input restored |

No new events needed for audio — spatial systems read player `Transform` as usual.

## Checkpoints

- [ ] "Lay Trail" mode registers in the interaction mode framework and cycles correctly
- [ ] Record a 10-second walk — verify `BreadcrumbTrail` has ~100 samples with monotonically increasing `elapsed`
- [ ] Visual trail dots appear behind the player during recording
- [ ] "Walk Trail" mode registers separately and is only available when a trail exists
- [ ] Replay walks backward first (end → start), matching the breadcrumb metaphor
- [ ] At each end, direction reverses — loop continues indefinitely
- [ ] Audio fades in/out matching position, mix sounds different on each pass (track phase drift)
- [ ] Cancel replay — deactivating the mode immediately restores manual control at current position
- [ ] Lingering — stand still for 5 seconds during recording, verify replay pauses at that spot for 5 seconds
- [ ] Flashlight mode — heading replays correctly (reversed when walking backward)
- [ ] Input suppressed — keyboard/D-pad do nothing during replay (except mode toggle to cancel)
