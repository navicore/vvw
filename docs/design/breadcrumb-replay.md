# Design: Breadcrumb Recording & Replay

## Intent

Let the player record a timed path through the maze, then replay it — once or looped. The player avatar retraces the exact route at the exact pace, lingering where the user lingered, rushing where they rushed. Audio is not recorded; spatial mixing happens live during replay from wherever the tracks happen to be in their playback cycle. Every replay produces a slightly different mix. The path is the composition; the sound is the performance.

**Why.** The maze is an instrument. Recording a path turns an improvised walk into a repeatable piece. Looping creates an ambient installation from a single gesture. The non-determinism (track phase drift) means no two listens are identical — the player authored the structure but not the exact sound.

## Constraints

- **Player input is suppressed during replay.** Movement keys/D-pad are ignored while the avatar follows the breadcrumb path. The user can cancel replay at any time.
- **No audio recording.** Only position + timestamp. This keeps the data tiny and avoids media permissions.
- **No maze mutation during replay.** If maze sculpting (see `maze-sculpting.md`) is implemented, sculpt mode is locked during replay to avoid desyncing the path from the geometry.
- **Don't break spatial audio.** The replay system moves the player entity the same way input does — `TrackAudioState` reacts to position as usual. No changes to LOS, gain, or pan.
- **Out of scope:** saving/loading breadcrumbs to disk, sharing paths between users, visual trail rendering, speed multiplier, editing recorded paths.

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

### Recording (vvw-game)

`BreadcrumbState` resource with an enum: `Idle | Recording { trail, timer } | Playing { trail, cursor, looping }`.

- **Start recording:** UI button or key. State transitions to `Recording`. Each tick, if enough time has passed since last sample, push a `Breadcrumb`.
- **Stop recording:** Same button. State transitions to `Idle`. Trail is stored.
- **Empty trail guard:** If fewer than 2 samples, discard silently.

### Replay (vvw-game)

- **Start replay:** UI button. State transitions to `Playing`. Player input systems early-return when state is `Playing`.
- **Each frame:** advance `cursor` by `dt`. Interpolate (lerp) between the two bracketing samples. Write interpolated position to the player's `Transform` and velocity to zero. Write heading to `PlayerHeading`.
- **End of trail:** if `looping`, reset cursor to 0. Otherwise transition to `Idle`, return control to player.
- **Cancel:** any movement input during replay transitions back to `Idle`.

### Why position, not input replay

Replaying recorded inputs through physics would accumulate drift (floating-point, frame-rate differences). Recording world positions and lerping guarantees the path is visually identical every time. The player entity is teleported along the path; physics velocity is zeroed to prevent collider interactions from perturbing it.

## Domain Events

| Event | Producer | Consumer |
|---|---|---|
| `BreadcrumbRecordStart` | UI / keybind | `BreadcrumbState` → `Recording` |
| `BreadcrumbRecordStop` | UI / keybind | `BreadcrumbState` → `Idle`, trail stored |
| `BreadcrumbPlayStart { looping: bool }` | UI / keybind | `BreadcrumbState` → `Playing`, input suppressed |
| `BreadcrumbPlayStop` | End-of-trail or user cancel | `BreadcrumbState` → `Idle`, input restored |

No new events needed for audio — spatial systems read player `Transform` as usual.

## Checkpoints

- [ ] Record a 10-second walk — verify `BreadcrumbTrail` has ~100 samples with monotonically increasing `elapsed`
- [ ] Replay oneshot — avatar retraces path at correct pace, audio fades in/out matching the original walk
- [ ] Replay loop — seamless restart, mix sounds different on second pass (track phase drift)
- [ ] Cancel replay — movement input immediately restores manual control
- [ ] Lingering — stand still for 5 seconds during recording, verify replay pauses at that spot for 5 seconds
- [ ] Flashlight mode — heading replays correctly, cone direction matches original recording
- [ ] Input suppressed — keyboard/D-pad do nothing during replay (except cancel)
