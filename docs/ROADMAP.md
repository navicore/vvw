# Roadmap

## Current State

The web player is deployed and functional on Cloudflare Pages. Albums are created via the CLI (`vvw-deploy create`) with an interactive `$EDITOR` workflow for metadata. Audio streams from Cloudflare R2. The WASM binary is size-optimized to stay under Cloudflare's 25 MiB limit.

26 tests pass. CI runs on Linux via `just ci`.

Desktop app (`vvw-app`) and kira audio wrapper (`vvw-audio`) have been removed. The project is web-only.

Recent additions:
- **Touch controls** — D-pad overlay for mobile browsers (Android, iPad). Visible on first touch, hidden on desktop.
- **Build version display** — Compile-time timestamp in album header for deploy diagnostics.
- **Audio resume on wake** — Detects suspended/interrupted `AudioContext` (after backgrounding, device sleep, iOS Safari interruptions) and resumes on next user gesture.

## Next Up

- **Playback controls**: Start/pause/mute button overlay (see `docs/design/playback-controls.md`)
- **Lightmap viewport culling**: Clip lightmap computation to camera viewport (see `docs/design/lightmap-viewport-culling.md`)
- **Player avatar**: Improve the player sprite rendering

## Known Limitations

- No visual maze editor — layout is fully procedural
- No real-time lighting preview — tuning requires deploy-and-check cycles
- Large audio uploads to R2 can time out (wrangler retries help)
