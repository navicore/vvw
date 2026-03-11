# Playback Controls — Start / Pause / Mute

## Intent

Two UX problems to solve:

1. **Why won't it move?** — New users land on the game after clicking the overlay, but the canvas doesn't have keyboard focus. Nothing visually tells them the game is waiting for interaction. The current "Click anywhere to start" overlay resumes audio but doesn't guarantee canvas focus.

2. **Boss key** — Users need a way to quickly mute/pause audio without closing the tab. There's no control for this today.

**Goal:** Replace the ambiguous overlay flow with a clear play/pause button that (a) doubles as the initial start action, (b) gives the canvas focus, and (c) lets users mute/pause at any time.

## Constraints

- The canvas must receive keyboard focus after clicking play — arrow keys must work immediately.
- Browser autoplay policy: `AudioContext.resume()` and `<audio>.play()` must happen inside a user gesture (click). The play button satisfies this.
- Must not break existing spatial audio, track foldout, or album header.
- Keep the overlay for the initial album title display — it's good branding. The play button lives on the overlay and replaces "Click anywhere to start."
- Minimal visual footprint during gameplay — the button should be unobtrusive once playing.

**Out of scope:** Seek, skip, volume slider, per-track mute. Those are future work.

## Approach

### HTML/CSS (index.html)

- Add a play/pause button to the album header bar (visible during gameplay).
- On the start overlay, replace "Click anywhere to start" with a green play triangle button. Clicking it runs the existing `setup_overlay_click` logic (resume AudioContext, play all tracks, hide overlay, show header) **and** focuses the canvas.
- Button states: green play arrow (paused/initial) ↔ red pause icon (playing).
- CSS-only icons (Unicode or border-trick triangles) — no image assets.

### JS (index.html script)

- `togglePlayback()`: if playing → `AudioContext.suspend()`, swap to play icon; if paused → `AudioContext.resume()`, swap to pause icon, re-focus canvas.
- The overlay click calls `togglePlayback()` + hides overlay + shows header.
- The header button calls `togglePlayback()`.

### WASM (lib.rs)

- `setup_overlay_click` passes the `AudioContext` reference to a global JS variable (or attaches it to the button's dataset) so the header button's JS can call `suspend()`/`resume()`.
- Alternatively, expose `AudioContext` via a small JS bridge function set during `setup_overlay_click`.

### Canvas focus

- After play is clicked (overlay or header), call `document.getElementById('game-canvas').focus()`. Bevy's `prevent_default_event_handling: true` already captures keys once focused.

## Domain Events

| Event | Producer | Consumer |
|-------|----------|----------|
| Overlay play click | User | JS: resume AudioContext, play tracks, hide overlay, show header, focus canvas |
| Header pause click | User | JS: suspend AudioContext, swap icon to play |
| Header play click | User | JS: resume AudioContext, swap icon to pause, focus canvas |

No new Bevy-side events — `AudioContext.suspend()` freezes all audio nodes automatically. The game loop keeps running (avatar still moves), only sound stops.

## Checkpoints

- [ ] First visit: overlay shows album title + green play button (not "click anywhere")
- [ ] Clicking play: audio starts, overlay hides, header appears with pause button, avatar responds to arrow keys immediately
- [ ] Clicking pause in header: audio stops, icon swaps to play
- [ ] Clicking play in header: audio resumes, canvas re-focused
- [ ] Mobile: buttons are tap-friendly (min 44px touch target)
- [ ] No regression on track foldout, spatial audio, or album info display
