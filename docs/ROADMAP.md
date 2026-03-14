# Roadmap

## Current State

The web player is deployed and functional on Cloudflare Pages. Albums are created via the CLI (`vvw-deploy create`) with an interactive `$EDITOR` workflow for metadata. Audio streams from Cloudflare R2. The WASM binary is size-optimized to stay under Cloudflare's 25 MiB limit.

26 tests pass. CI runs on Linux via `just ci`.

Desktop app (`vvw-app`) and kira audio wrapper (`vvw-audio`) have been removed. The project is web-only.

Recent additions:
- **Touch controls** — D-pad overlay for mobile browsers (Android, iPad). Visible on first touch, hidden on desktop.
- **Build version display** — Compile-time timestamp in album header for deploy diagnostics.
- **Audio resume on wake** — Detects suspended/interrupted `AudioContext` (after backgrounding, device sleep, iOS Safari interruptions) and resumes on next user gesture.
- **Lightmap viewport culling** — Lighting computation clipped to camera viewport, reducing raycast work by ~97% on large mazes.
- **Directional flashlight** — Per-album configurable cone light mode (`player_light_mode: Flashlight` in `project.ron`). Left/Right rotate heading, Up/Down move along heading. Soft-edge falloff on cone boundary.
- **Album & track info panels** — Clickable album title drops down cover art and description over the canvas. Track info bar shows per-track artwork on click. Chevron indicators signal interactivity. Image deployment workflow scans for cover art and track artwork during album creation.
- **Canvas focus indicator** — CSS-only dim overlay with arrow key hint when canvas loses focus. Clears on click/tap.

## Next Up

- **Playback controls**: Start/pause/mute button overlay (see `docs/design/playback-controls.md`)
- **Player avatar**: Improve the player sprite rendering
- **Background artwork**: Artist-provided image as maze background (see `docs/design/background-artwork.md`)
- **Audio download resilience**: Detect and retry zero-byte audio loads (see `docs/design/audio-download-resilience.md`)

## Known Limitations

- No visual maze editor — layout is fully procedural
- No real-time lighting preview — tuning requires deploy-and-check cycles
- Large audio uploads to R2 can time out (wrangler retries help)
