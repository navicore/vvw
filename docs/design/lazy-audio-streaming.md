# Lazy Audio Streaming

## Intent

All `<audio>` elements stream continuously from R2 once activated, even when gain is zero (no LOS, beyond max distance). On a 10-track album over a constrained connection, that's 10 concurrent streams competing for bandwidth — most delivering audio nobody hears. Spotify-like apps handle low bandwidth better because they stream one track at a time.

Pause inaudible tracks to free bandwidth for the 1-3 tracks that are actually audible. Pre-fetch tracks that are *about to become* audible so the listener never notices the stream was paused.

## Constraints

- Audible tracks must never glitch, pop, or have perceptible delay when they become audible
- `MediaElementAudioSourceNode` capture must remain intact — cannot destroy and recreate the Web Audio graph per pause/play cycle
- Safari ordering requirement (play before createMediaElementSource) only applies at initial activation — subsequent play() calls on an already-captured element are fine
- Looping must continue seamlessly — `loop=true` stays on the element
- Must not interfere with `resume_suspended_audio` (browser backgrounding recovery)
- Out of scope: preloading entire files into memory (that's the Cache API design doc)

## Approach

### Two thresholds: audible and pre-fetch

The spatial audio system already computes `target_gain` and `current_gain` per track each frame. Use these to define streaming zones:

| Zone | Condition | `<audio>` state |
|------|-----------|-----------------|
| **Audible** | `current_gain > 0` | Playing (as today) |
| **Pre-fetch** | `current_gain == 0` but distance < `MAX_DISTANCE + margin` | Playing (warm up buffer) |
| **Silent** | `current_gain == 0` and distance >= `MAX_DISTANCE + margin` | Paused |

The **margin** (e.g., 5 tiles beyond `MAX_DISTANCE` of 15 = 20 tile radius) gives several seconds of buffer time at typical player speed. A track starts streaming before it could possibly become audible, so by the time the player rounds a corner and gets LOS, the browser already has audio data buffered.

### Implementation

Add a method to `WebAudioEngine`:

```rust
pub fn set_streaming(&self, id: usize, should_stream: bool)
```

- `should_stream == true` → if paused, call `play()`
- `should_stream == false` → if playing and not paused, call `pause()`

Call this from `web_audio_sync` (which already runs each frame after `SpatialAudioSet`), using distance from `TrackAudioState` or a new field.

### Distance awareness

`web_audio_sync` currently only sees `TrackAudioState` (gain + pan). It doesn't know distance. Two options:

**Option A: Add distance to `TrackAudioState`** — `compute_spatial_targets` already computes `player_pos.distance(tile_pos)`. Store it on `TrackAudioState` as `pub distance: f32`. The web layer reads it to decide streaming state. This keeps the decision in the platform layer where the `<audio>` elements live.

**Option B: Use gain as proxy** — If `current_gain == 0` and `target_gain == 0` for N consecutive frames (debounce), pause. Resume when `target_gain > 0`. Simpler but no pre-fetch margin — the track starts streaming only when LOS is established.

**Recommendation: Option A.** The pre-fetch margin is the whole point — we want audio streaming *before* it's audible. Distance is cheap to store and gives precise control.

### Debounce

Don't pause immediately when a track crosses the threshold — the player might be dancing at the boundary. Require the track to stay in the silent zone for ~2 seconds before pausing. Resume is immediate (no debounce).

### What happens to a paused looping `<audio>`?

When `pause()` is called on a looping `<audio>` element, the browser stops fetching but remembers the playback position. On `play()`, it resumes from where it left off and continues fetching. The Web Audio graph connection (`MediaElementAudioSourceNode` → `GainNode` → `Panner`) stays intact — `pause()`/`play()` don't affect the node wiring. The gain is already 0 when we resume streaming, so any buffering hiccup is inaudible.

## Domain Events

- **`TrackAudioState.distance` written** — by `compute_spatial_targets` each frame (new field)
- **`set_streaming(id, bool)` called** — by `web_audio_sync` based on distance threshold (new)
- **`<audio>.pause()` / `<audio>.play()`** — browser stops/starts network fetch (existing API)
- No new Bevy systems — distance is added to existing state, streaming control added to existing `web_audio_sync`

## Checkpoints

1. On a healthy connection, no audible difference from today — tracks fade in/out identically
2. DevTools Network tab shows paused tracks stop fetching (no new byte ranges requested)
3. Walk toward a distant track — streaming begins before it becomes audible (pre-fetch margin)
4. Rapidly move back and forth at boundary — debounce prevents rapid pause/play toggling
5. Throttle network to 3G — fewer concurrent streams means audible tracks get more bandwidth, less choppiness
6. `resume_suspended_audio` still works after backgrounding (paused-for-distance tracks don't interfere)
