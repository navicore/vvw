# Playback Controls — Mute Mode

## Intent

Users need a way to quickly silence all audio without closing the tab — take a call, listen to something else, or just pause the soundscape. Today there's no control for this.

**Why.** The maze is an ambient experience. Users leave it running. They need a fast mute/unmute toggle that doesn't disrupt their position or the visual state.

## Constraints

- **Use the interaction modes framework.** Mute is registered as a mode. When active, all audio is silenced. When deactivated, audio resumes. The control surface (two-finger tap / right-click / Tab) provides the toggle.
- **Don't break spatial audio.** `TrackAudioState` continues to be computed normally. Only the platform layer's volume output is zeroed. When unmuted, audio resumes at the correct spatial levels immediately — no fade-in delay.
- **Don't break other modes.** Mute can coexist with piping, breadcrumbs, etc. `suppresses_movement: false`.
- **No HTML/JS changes.** This is a pure Bevy + platform-layer feature. The existing overlay click flow for initial audio activation is unchanged.
- **Out of scope:** Per-track mute, volume slider, seek, skip. The overlay play button redesign (replacing "Click anywhere to start" with a styled play button) is separate work.

## Approach

### Mute mode plugin (vvw-game)

Register a "Mute" mode (`ModeDescriptor` with `suppresses_movement: false`). No game-layer systems needed — the mode's active/inactive state is the only signal.

### Platform layer (vvw-web)

A system checks `ActiveMode` for the mute mode ID. When active, set a `muted: bool` flag on a resource. `web_audio_sync` reads this flag: when muted, set all track volumes to 0.0 regardless of `TrackAudioState`. When unmuted, resume normal behavior.

This approach:
- Keeps the `AudioContext` running (no `suspend()`/`resume()` gesture issues)
- Preserves `TrackAudioState` computation so unmute is instant
- Avoids touching `<audio>` elements (no play/pause race conditions)

### Alternative considered: `AudioContext.suspend()`

Suspending the context is cleaner but requires a user gesture to resume on some browsers. Since the mode toggle already happens via gesture (tap/click), this could work — but zeroing gain is simpler and avoids platform-specific resume behavior.

## Domain Events

| Event | Producer | Consumer |
|-------|----------|----------|
| `ActiveMode` changes to mute | Mode framework (user gesture) | Platform layer: zero all gains |
| `ActiveMode` changes away from mute | Mode framework (user gesture) | Platform layer: restore normal gains |

No new messages or Bevy events needed.

## Checkpoints

- [ ] "Mute" mode registers in the interaction mode framework and cycles correctly
- [ ] Activating mute silences all audio immediately
- [ ] Spatial audio state continues updating while muted (player can walk around)
- [ ] Deactivating mute restores audio at correct spatial levels — no pop or fade delay
- [ ] Mute works alongside other active features (pipes, breadcrumbs)
- [ ] No browser autoplay issues on unmute (no `suspend()`/`resume()` needed)
