# Architecture

## Context & Scope

VVW is a spatial audio exploration game. Players navigate a 2D maze; audio tracks placed in rooms fade in/out based on line-of-sight and distance. The game runs as a WASM web app on Cloudflare Pages. Audio streams from Cloudflare R2. Albums are authored on the desktop app (to be replaced by a CLI tool — see `remove-desktop-plan.md`).

External boundaries:
- **Cloudflare Pages** — hosts the WASM player, per-album `index.html`, `project.ron` manifests, and `_config.json`
- **Cloudflare R2** (`vvw-audio` bucket) — streams audio files via `<audio>` elements with CORS
- **Browser** — WebGL2 rendering (Bevy), Web Audio API for spatial mixing
- **Local filesystem** — saved projects under platform data dir (`~/Library/Application Support/vvw/projects/`)

## Solution Strategy

- **Rust + Bevy 0.18** — game engine for both desktop and WASM targets. Shared `VvwGamePlugin` runs identically on both.
- **Avian2d** — 2D physics (zero-gravity, collision-only for maze walls).
- **vvw-light** — custom sprite-based 2D lighting with point lights and occluder grid. Not Bevy's built-in lighting.
- **RON** — project manifest format (`project.ron`). Backward-compatible via `#[serde(default)]`.
- **Web Audio API** — `MediaElementAudioSourceNode` for streaming playback. No full download before play.
- **Trunk** — WASM build toolchain. `wasm-opt` post-processing with `release-wasm` cargo profile to stay under Cloudflare's 25 MiB file limit.
- **No unsafe code** — `unsafe_code = "forbid"` workspace-wide.

## Building Blocks

```
vvw-core          Platform-agnostic types and algorithms (no Bevy)
  maze            Grid storage, tile queries, track_ids map
  tiles           TileKind, TilePos, coordinate ↔ world conversion
  spatial         Bresenham LOS, distance-to-gain curve, stereo panning
  mazegen         Procedural maze generation (room + corridor growth)
  project         ProjectManifest serde, AlbumMetadata, TrackMetadata

vvw-light         2D lighting plugin for Bevy
  PointLight2d    Per-entity point light with intensity, radius, falloff
  AmbientLight2d  Global ambient brightness
  LightOccluder   Per-wall-tile occluder
  OccluderGrid    Grid-level LOS cache for lighting

vvw-game          Platform-independent Bevy plugin (VvwGamePlugin)
  MazePlugin      Tile rendering, wall colliders, occluder grid sync
  PlayerPlugin    Avian2d body, leafwing-input-manager, movement
  SpatialAudio    compute_spatial_targets → interpolate → TrackAudioState
  CameraPlugin    Dead-zone follow camera + Lighting2dPlugin

vvw-web           WASM web player (cdylib)
  WebAudioEngine  <audio> → MediaElementSource → GainNode → StereoPanner
  project.rs      Fetch project.ron + _config.json via Fetch API
  ui.rs           Overlay (click-to-start), album info display

vvw-app           Desktop admin app (binary) — BEING REMOVED
  AdminPlugin     Egui UI, kira audio, file drag-and-drop, project save/load

vvw-audio         Kira audio wrapper (desktop) — BEING REMOVED
  GameAudioManager, GameTrack (StaticSoundHandle)

vvw-deploy        CLI tool for assembly and deployment
  create          (planned) Album creation from audio directory
  assemble        Copy trunk dist + manifests into deploy dir
  upload-audio    Push audio files to R2 via wrangler
  deploy          wrangler pages deploy
  preview         Local dev server via wrangler pages dev
```

**Core domain entities:**
- `Maze` — 2D grid of `TileKind` (Wall, Floor, PlayerStart, TrackIcon) + `track_ids` map. Resource inserted by platform layer before game starts.
- `TrackAudioState` — per-track component with target/current gain and pan, interpolated each frame. The bridge between spatial logic and audio backend.
- `ProjectManifest` — serialized album: maze, lighting config, track list, album metadata.

**Key invariant:** The game plugin never touches audio backends directly. It writes `TrackAudioState` components. The platform layer reads them (`web_audio_sync` on web, `interpolate_and_send` on desktop via `TrackHandles`).

## Crosscutting Concepts

**Error handling:** `anyhow` in CLI tools (vvw-deploy). `thiserror` for typed errors in vvw-audio. `JsValue` in WASM. Game systems use silent fallbacks (skip missing tracks, default to zero gain).

**Serialization:** RON everywhere except `_config.json` (minimal JSON, hand-parsed to avoid serde_json in WASM). Backward compatibility via `#[serde(default)]` on all manifest fields added after v1.

**System ordering:** `SpatialAudioSet` exported from vvw-game. All spatial audio systems chained: `reset_new_tracks → compute_spatial_targets → interpolate_and_send → apply_lighting_config`. Web layer runs `web_audio_sync.after(SpatialAudioSet)`.

**WASM constraints:** Single-threaded. `NonSend` for web-sys types. `#![allow(clippy::future_not_send)]`. Only `vvw-web` targets `wasm32-unknown-unknown`; other crates are native-only or platform-agnostic.

**Linting:** Clippy pedantic + nursery + cargo as warnings. Selective allows for game-domain patterns (float_cmp, cast_precision_loss, module_name_repetitions).

**Testing:** Unit tests inline (`#[cfg(test)]`). 22 tests across workspace. WASM tests via `wasm-pack test --node`. CI runs `just ci` (format check, clippy, tests, build).

**Build:** Justfile is the single source of truth. CI/CD workflows only call `just` recipes.
