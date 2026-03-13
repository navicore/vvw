# Audio Download Resilience

## Intent

On some corporate networks, TLS-intercepting proxy software silently limits concurrent outbound connections. When the player creates all `<audio>` elements at once (each with `preload="auto"`), the proxy may truncate or drop some streams, resulting in zero-byte audio that the browser caches as a valid response. The affected track is then permanently silent until the user manually purges their cache.

The goal is to detect zero-byte or failed audio loads and recover gracefully, without requiring user intervention.

## Constraints

- Must not slow down audio loading on healthy networks — parallel downloads should remain the default
- Must not break the existing activation flow (overlay click → play → createMediaElementSource)
- Safari ordering requirement must be preserved (play before capture)
- Must not add perceptible delay to playback start on normal connections
- Out of scope: diagnosing or working around the proxy itself

## Approach

**Detection + sequential retry for failed tracks.**

### Current flow (all parallel)

All `<audio>` elements are created in `add_track()` with `preload="auto"`. The browser opens N connections simultaneously. On activation, `play()` is called on all of them.

### Proposed flow

1. **Keep parallel preloading as-is** — this is fast on healthy networks
2. **After activation, monitor each audio element** for load health:
   - Listen for `error` events on each `<audio>` element
   - After a short delay post-activation (e.g., 3-5 seconds), check `buffered.length === 0` or `networkState === HTMLMediaElement.NETWORK_EMPTY` on each element — these indicate the browser got nothing usable
3. **Retry failed tracks sequentially:**
   - Create a fresh `<audio>` element with a cache-busting query param (e.g., `?retry=1`) to bypass the cached zero-byte response
   - Wait for `canplay` event before starting the next retry
   - Swap the new element into the Web Audio graph (disconnect old source, connect new)
   - Sequential retry avoids re-triggering the proxy's connection limit
4. **Log retries to console** so the problem is visible during debugging

### Graph reconnection

Replacing an `<audio>` element in a live Web Audio graph requires:
- Disconnecting the old `MediaElementAudioSourceNode`
- Creating a new source from the new element
- Reconnecting: `source → gain → panner → dest`
- The existing `GainNode` and `Panner` can be reused — only the source changes

### Alternative considered: serialize all initial downloads

Could set `preload="none"` and load tracks one at a time from the start. Rejected because it penalizes the 99% of users on healthy networks with slower load times. Detect-and-retry is the right tradeoff.

## Domain Events

- **`error` event on `<audio>`** — browser fires this on failed loads (consumed)
- **Health check timer** — fires once after activation to scan for zero-byte loads (new)
- **Console warning** — logged when retry is triggered (produced)
- No new Bevy systems — this is entirely within `WebAudioEngine`

## Checkpoints

1. On a healthy network, all tracks load in parallel as before — no behavior change
2. Simulate a failed track (e.g., serve a zero-byte response for one file) — player detects and retries
3. Retried track loads successfully and plays with correct spatial audio
4. Console shows a warning message identifying which track was retried
5. Multiple failed tracks are retried sequentially, not in parallel
