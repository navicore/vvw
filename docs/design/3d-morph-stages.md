# 3D Morph Mode — Staged Implementation Plan

## Intent

Implement the 3D morph mode from `3d-morph-mode.md` in stages that are individually mergeable and safe to abandon. Each stage is additive — no existing behavior changes until the player explicitly triggers the morph.

## Safety Model

- **Stages 1–2**: Invisible. 3D entities and camera exist but are hidden/inactive. The 2D game is unchanged. Safe to merge with no feature flags.
- **Stage 3**: Activates the morph. Gated behind `morph_3d: bool` in `project.ron` (default false). Albums opt in.
- **Abandon at any point**: stages 1–2 leave dormant code. Stage 3's flag defaults to off. No user-facing regression to unwind.

## Stages

### Stage 1 — Enable 3D renderer + spawn hidden meshes

**Goal:** 3D wall and floor geometry exists in the ECS, matching the 2D tile grid exactly, but is never rendered.

- Add `3d_bevy_render` to bevy features in workspace `Cargo.toml`
- New `Morph3dPlugin` in `vvw-game` (new crate `morph3d.rs`)
- In a `PostStartup` system (after `spawn_maze_tiles`), iterate the `Maze` resource:
  - Wall tiles → spawn `Mesh3d` box (TILE_SIZE x TILE_SIZE x TILE_SIZE), matching tile color, `Visibility::Hidden`
  - Floor tiles → spawn `Mesh3d` flat quad at y=0, `Visibility::Hidden`
  - Track icons → spawn `Mesh3d` smaller cube, track color, `Visibility::Hidden`
- Tag all 3D entities with a `Mesh3dTile` marker component
- Player and camera unchanged

**Checkpoint:** `cargo clippy`, `just build-web`, WASM under 25 MiB, existing albums behave identically.

### Stage 2 — Add inactive Camera3d + Bevy 3D lights

**Goal:** A 3D camera and light sources exist but are inactive. The 2D camera remains primary.

- Spawn `Camera3d` with `GameCamera3d` marker, `IsDefaultUiCamera(false)`, positioned at player location, ground-level height (TILE_SIZE * 0.4), looking along player heading
- Set `Camera3d` order lower than `Camera2d` so it doesn't render (or use `is_active: false`)
- Spawn `PointLight` at each track icon position (matching 2D `PointLight2d` intensity/radius)
- Spawn `SpotLight` as child of player entity (flashlight equivalent), inactive
- Add a `follow_player_3d` system that updates `Camera3d` position/rotation to match player, gated by a `Morph3dActive` resource (default false) — runs but writes to an invisible camera

**Checkpoint:** 2D game unchanged. 3D camera entity exists. No visual difference.

### Stage 3 — Morph trigger + camera swap

**Goal:** Player can trigger the morph. 2D sprites hide, 3D meshes show, camera switches.

- Add `morph_3d: bool` to `AlbumMetadata` (`#[serde(default)]`, default false)
- Register "3D" mode in `ModeRegistry` (order: 50, between Mute and breadcrumbs), only when `morph_3d: true`
- When mode activates:
  - Set `Morph3dActive(true)`
  - Hide all `MazeTile` sprites (`Visibility::Hidden`)
  - Show all `Mesh3dTile` meshes (`Visibility::Inherited`)
  - Deactivate `Camera2d`, activate `Camera3d`
  - Hide vvw-light lightmap overlay
  - Enable 3D `PointLight` and `SpotLight`
- When mode deactivates: reverse all of the above
- Input: in 3D mode, WASD/D-pad drives heading rotation + forward/back impulse instead of cardinal movement

**Checkpoint:** Toggle morph via control surface. Audio identical in both modes. Walk through 3D corridors. Revert to 2D cleanly.

### Stage 4 — Polish (post-merge, iterative)

- Camera morph animation (smooth transition over ~1s instead of hard swap)
- Artwork on wall surfaces (album background texture)
- Track artwork on track cubes
- Tune 3D lighting (intensity, shadows, ambient)
- First-person vs third-person camera option

## Constraints

- Audio output must be identical in 2D and 3D for the same player position
- Physics stays 2D (avian2d, XY plane)
- No .gltf or external 3D assets — procedural geometry only
- Existing albums with `morph_3d: false` (or missing) see zero change

## Domain Events

No new messages or events in stages 1–2. Stage 3 reuses `ActiveMode` from the interaction modes framework — no new event types needed.
