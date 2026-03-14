# Audio Caching & Bandwidth Awareness

## Intent

Audio streaming from R2 is highly sensitive to bandwidth. On constrained connections (5G fallback, hotel WiFi, mobile data) audio becomes choppy even when the game runs smoothly — the WASM/Bevy rendering is fully local but audio streams continuously from the network.

Two goals:
1. **Warn the user** when bandwidth is degrading audio quality, so they understand it's network — not the player
2. **Cache audio locally** so repeat visits never re-download — album tracks change only at dev time, making them ideal cache-once candidates

## Constraints

- Must not break existing activation flow (overlay click → play → createMediaElementSource)
- Must not delay first-play on good connections — streaming start should remain instant
- Safari ordering requirement preserved (play before capture)
- Cache invalidation must work when the artist re-deploys updated tracks
- Storage quota varies by browser/device — must degrade gracefully if quota is exceeded
- Out of scope: offline-first PWA shell (service worker for HTML/WASM), background sync

## Approach

### Phase 1: Bandwidth Warning

Detect degraded playback and show a non-intrusive toast/banner.

- Monitor `<audio>` element `waiting` events (fires when playback stalls for buffering)
- Track stall frequency per rolling window (e.g., >2 stalls in 10 seconds = degraded)
- Show a dismissible banner: "Audio may be choppy — slow connection detected"
- Auto-dismiss when stalls stop (connection recovers)
- CSS-only banner, toggled by a class on a DOM element — no Bevy changes needed

### Phase 2: Cache API Storage

Use the browser Cache API to store audio files after first successful download.

- **On activation**, for each track URL:
  1. Check `caches.match(url)` — if cached, create `<audio>` from cached blob
  2. If not cached, stream from R2 as today, then `cache.put(url, response.clone())` in background
- **Cache key**: the audio URL includes the track_id, so re-deployed tracks with new content get new URLs only if the file changes. Add a `?v=<hash>` query param from a manifest field to enable cache-busting on re-deploy.
- **Manifest field**: `audio_version` (or per-track hash) in `project.ron` — bumped by `vvw-deploy upload-audio` when file content changes
- **Quota exceeded**: catch `QuotaExceededError`, log it, continue streaming from network — caching is best-effort
- **Cache name**: `vvw-audio-v1` — versioned so future format changes can start fresh

### Why Cache API over Service Worker

A service worker would cache more aggressively but adds complexity (registration, update lifecycle, scope rules). The Cache API is available from the main thread, requires no registration, and gives explicit control over what's cached. Audio files are the only large assets worth caching — WASM/HTML are content-hashed and cached by the CDN already.

### Why not IndexedDB

IndexedDB could store audio blobs, but the Cache API is purpose-built for request/response caching and integrates naturally with fetch. Less impedance mismatch.

## Domain Events

- **`waiting` event on `<audio>`** — browser fires when playback stalls (consumed, Phase 1)
- **`playing` event on `<audio>`** — playback resumes after stall (consumed, Phase 1)
- **Bandwidth warning shown/hidden** — DOM class toggle (produced, Phase 1)
- **Cache hit/miss** — logged to console (produced, Phase 2)
- **`QuotaExceededError`** — storage full, skip caching (consumed, Phase 2)
- No new Bevy systems — both phases are entirely within `WebAudioEngine` and DOM/JS interop

## Checkpoints

### Phase 1
1. On a good connection, no warning appears
2. Throttle network in DevTools to 3G — warning appears after repeated stalls
3. Restore network speed — warning auto-dismisses
4. Warning is dismissible by click

### Phase 2
1. First visit: audio streams from R2 (network tab shows requests), cached in background
2. Second visit: audio loads from cache (network tab shows no R2 requests for audio)
3. Re-deploy with changed audio: new version param busts cache, fresh download occurs
4. Fill storage quota: caching silently skipped, streaming continues as fallback
5. Clearing browser cache removes cached audio — next visit re-downloads
