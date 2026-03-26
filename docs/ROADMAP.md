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
- **Sound wave visuals** — Per-album `)))` arc animations radiating from audible track sources toward the player. Count scales with gain (1–3 arcs), arcs vibrate in place, outer arcs are longer/flatter than inner. Enabled via `sound_visuals: true` in album metadata.
- **Background artwork** — Artist-provided image rendered behind maze tiles at z=-1 under the lightmap. Tiles set to `Color::NONE`. Configured via `background_url` in album metadata.
- **Lazy audio streaming** — Tracks beyond prefetch distance have src cleared and load() called to stop downloads. 2s debounce before pausing; immediate resume when within range.
- **R2 direct upload** — rust-s3 with native-tls, 10 retries with exponential backoff. Falls back to wrangler if S3 env vars not set.
- **OG meta tags** — Open Graph and Twitter Card meta tags injected into per-album `index.html` at assembly time. Cover art, title, artist, description. `--site-url` CLI arg for canonical URLs.
- **Solid track icons** — Track icons are physics colliders (60% tile size). Player bounces off them with configurable `track_restitution`.
- **Interaction modes framework** — Tab/two-finger tap cycles through registered modes. `ModeRegistry` + `ActiveMode` resource. Modes can suppress movement.
- **Sound piping** — Route audio from distant tracks to new locations via dashed construction lines. `sound_piping: true` in album metadata.
- **Breadcrumb replay** — Record player path at 10 Hz, replay as endless back-and-forth loop. `breadcrumbs: true` in album metadata.
- **Mute mode** — Always registered. Zeros all gains when active. Spatial audio continues computing so unmute is instant.
- **3D view toggle** — `V` key (desktop) / three-finger tap (mobile) switches between 2D top-down and 3D first-person. `morph_3d: true` in album metadata. Cubicle-height walls, heading-relative controls, background art textures floor and walls.
- **Wall walking** — Spacebar (desktop) / swipe-up or double-tap D-pad (mobile) mounts adjacent wall. `wall_walking: true` in album metadata. `GameLayer` collision layers (Floor/Elevated) for physics filtering. Elevated = 20% gain, LOS bypassed, pipes silent. Outer edge walls block both layers.

## Next Up

- **Player avatar**: Improve the player sprite rendering
- **Audio download resilience**: Detect and retry zero-byte audio loads (see `docs/design/audio-download-resilience.md`)
- **World boundary**: Fall sequence and respawn when player escapes the maze (see `docs/design/world-boundary.md`)

## Known Limitations

- No visual maze editor — layout is fully procedural
- No real-time lighting preview — tuning requires deploy-and-check cycles
- Large audio uploads to R2 can time out (rust-s3 retries with exponential backoff; wrangler fallback also retries)
