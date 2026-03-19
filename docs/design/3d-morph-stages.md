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
- `V` key (desktop) / three-finger tap (mobile) toggles between 2D and 3D — independent of mode framework, so modes (Mute, Pipe, Breadcrumbs) work in both views
- `Morph3dEnabled` resource gates the toggle listener; `Morph3dActive` tracks current state
- When toggling to 3D:
  - Hide all `MazeTile` sprites, show all `Mesh3dTile` meshes
  - Deactivate `Camera2d`, activate `Camera3d`, swap `IsDefaultUiCamera`
  - Hide vvw-light lightmap overlay
  - Show 3D `PointLight` and `SpotLight`
  - Force heading-relative controls (both keyboard and D-pad)
- When toggling to 2D: reverse all of the above
- Cubicle-height walls (`TILE_SIZE * 0.3`) — player looks over walls, track cubes visible as landmarks

**Checkpoint:** Toggle via V key. Audio identical in both views. Walk through 3D cubicle maze. All modes work in both views. Revert to 2D cleanly.

### Stage 4 — Polish (post-merge, iterative)

**Lighting** (priority — 3D scene is too dark with cubicle walls):
- Add `AmbientLight` for global baseline brightness
- Tune `PointLight` intensity/range at track positions for the cubicle scale
- Tune `SpotLight` on player (flashlight feel at eye level)
- Consider directional overhead light for even illumination

**Visual polish:**
- Camera morph animation (smooth transition over ~1s instead of hard swap)
- Artwork on wall surfaces (album background texture)
- Track artwork on track cubes
- Configurable wall height via `project.ron` (currently hardcoded cubicle-height)

**Future (separate from stage 4):**
- Third-person camera option (offset behind player)
- 3D visual representations for pipes and breadcrumb dots (functional in both views today, just invisible in 3D)

## Constraints

- Audio output must be identical in 2D and 3D for the same player position
- Physics stays 2D (avian2d, XY plane)
- No .gltf or external 3D assets — procedural geometry only
- Existing albums with `morph_3d: false` (or missing) see zero change

## Domain Events

No new messages or events. The 3D toggle writes `Morph3dActive` directly — no mode framework involvement, no new event types.
