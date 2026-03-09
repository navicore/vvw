# Plan: Remove Desktop App, CLI-Only Album Creation

## Goal

Eliminate `vvw-app` (Bevy desktop GUI) and `vvw-audio` (kira backend). Album creation
becomes a CLI command. The only runtime is the web player.

## New Command: `vvw-deploy create`

Add a `Create` subcommand to the existing `vvw-deploy` CLI:

```sh
just create-album --title "Cognology" --artist "Ed" ./audio-files/
```

Which runs:

```sh
cargo run -p vvw-deploy --release -- create ./audio-files/ \
    --title "Cognology" --artist "Ed" --name cognology
```

### What it does

1. Scan the directory for audio files (wav/mp3/ogg/flac)
2. Sort files alphabetically (deterministic track ordering)
3. Generate a maze via `vvw_core::mazegen` — one room per track
4. Copy each file as `{track_id}.audio` into the project's `audio/` dir
5. Derive track metadata from filenames (or accept a metadata JSON/RON file)
6. Write `project.ron` manifest
7. Print summary: track count, maze size, project path

### Optional flags

- `--name` — project name (default: derived from `--title`)
- `--title` / `--artist` — album metadata
- `--room-size-min` / `--room-size-max` — maze gen params (sensible defaults)
- `--corridor-length-min` / `--corridor-length-max`
- `--metadata tracks.ron` — per-track title/artist overrides
- `--regenerate` — regenerate maze for an existing project (keep audio)

## Crates to Delete

### `vvw-app`
- `crates/vvw-app/` — entire crate
- Contains: AdminPlugin (egui UI), kira audio setup, file drag-and-drop,
  project save/load, maze regen
- `project.rs` save/load logic already duplicated in `vvw-deploy`

### `vvw-audio`
- `crates/vvw-audio/` — entire crate
- Contains: `GameAudioManager`, `GameTrack` (kira wrappers)
- Only used by `vvw-app`

## Code to Clean Up

### `vvw-core`
- Remove `audio` module (`TrackHandle` trait) — only existed for desktop/web abstraction
- Keep: `maze`, `mazegen`, `project`, `spatial`, `tiles`

### `vvw-game`
- Remove `TrackHandles` resource (HashMap of `Box<dyn TrackHandle>`)
- Remove `TrackHandle` import
- `reset_new_tracks` — no longer needs to call `track.pause()` / `track.set_volume()`
  on handles; only web uses audio and it has its own sync system
- `interpolate_and_send` — remove the `TrackHandles` interaction entirely;
  rename to `interpolate_audio_state` since it only updates `TrackAudioState` components
- `SpatialAudioPlugin` — drop `TrackHandles` resource init
- Keep everything else: `SpatialAudioSet`, `TrackAudioState`, `TrackIdCounter`,
  spatial targets, lighting config, maze, player, camera

### `vvw-web`
- Remove `TrackHandles` import if present
- No other changes — `web_audio_sync` already reads `TrackAudioState` directly

### Workspace `Cargo.toml`
- Remove `vvw-audio` from members and workspace dependencies
- Remove `vvw-app` from members and default-members
- Remove desktop-only deps: `kira`, `bevy_egui`, `dirs` (if not used by vvw-deploy)
- Change `default-members` to `["crates/vvw-deploy"]`

### CI/CD
- `ci-linux.yml` — remove any desktop-specific test steps
- `ci-macos.yml` — may no longer be needed (desktop was the reason for macOS CI);
  or keep for cross-platform WASM validation
- Remove system dependency installs (`libasound2-dev` etc.) if no longer needed

## Justfile Changes

### Remove
- `run` / `run-debug` (desktop app)

### Add
- `create-album` recipe wrapping `vvw-deploy create`

### Keep as-is
- All web build/deploy/test recipes
- `ci`, `build`, `test`, `lint`, `fmt` (workspace-level)

## Migration Steps

1. **Add `Create` subcommand to `vvw-deploy`** — implement album creation CLI
2. **Test**: create an album via CLI, deploy it, verify it plays identically
3. **Strip `TrackHandles` from `vvw-game`** — simplify audio to state-only
4. **Delete `vvw-app`** and `vvw-audio`
5. **Clean up `vvw-core`** — remove `TrackHandle` trait
6. **Update workspace config** — members, default-members, deps
7. **Update CI** — remove desktop build/test steps, macOS runner decision
8. **Update docs** — album-workflow.md, design.md

## Risk

- **No visual maze editor**: Maze layout becomes fully procedural with CLI params.
  Users can't manually place rooms. This is acceptable — the maze gen is good
  enough and deterministic per track set.
- **No real-time lighting preview**: Lighting params set via CLI flags or defaults.
  Tuning requires deploy-and-check cycles. Could add a `just preview-local` that
  builds and serves instantly.
- **Existing saved projects**: Still loadable — `project.ron` format unchanged.
  The `create` command writes the same format.
