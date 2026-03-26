# 3D Morph Mode — Exploration

## Intent

The 2D maze works as a mixing instrument because spatial relationships (distance, line-of-sight, heading) control what the player hears. A 3D morph mode would preserve these rules while adding verticality, perspective, and immersion — walls become corridors you look down rather than tiles you look at from above. The transition itself (2D flattening into 3D, or vice versa) would be a dramatic moment, triggered by the player or by the album at a defined point.

**Why consider this.** The maze-as-instrument concept doesn't depend on dimensionality. Gain is a function of distance. Panning is a function of angle. LOS is a function of obstacles. All of these translate directly to 3D. If the transition is smooth and the rules stay the same, it becomes a new way to experience the same album — not a different game.

## Feasibility Assessment

### What works without changes

- **Spatial audio**: gain/pan are computed from 2D distance and angle. In 3D, the player still moves on the XY plane — same math, same `TrackAudioState`, same platform-layer sync.
- **Physics**: avian2d operates in 2D. If 3D movement is constrained to the XY ground plane (no jumping), existing colliders and velocity work as-is.
- **Web Audio**: completely decoupled from rendering. Zero impact.
- **Interaction modes**: framework is render-agnostic. Mute, piping, breadcrumbs all read/write game state, not rendering state.

### What needs new work

- **Rendering**: current pipeline is `Sprite` + `Mesh2d` only. Bevy's `3d_bevy_render` feature is not enabled. Walls would need to be extruded to 3D meshes at runtime (procedural box geometry from the `Maze` grid — no asset pipeline needed).
- **Camera**: `Camera2d` → `Camera3d` with first-person or low-angle perspective. The morph transition is a camera animation (orthographic top-down → perspective ground-level).
- **Lighting**: vvw-light is a CPU-side 2D raycast overlay. It cannot operate in 3D. Options:
  - Use Bevy's built-in `PointLight` + `StandardMaterial` in 3D mode (simple, correct, but different visual feel).
  - Keep the 2D lightmap as a minimap overlay and use simple ambient + directional light for the 3D view.
- **Player visual**: currently a colored square sprite. In 3D first-person, the player is invisible (camera IS the player). In third-person, needs a simple 3D placeholder.

### Hard constraints

- **WASM size**: Measured. Enabling `3d_bevy_render` increases WASM from 21 MiB to 24.2 MiB (25,379,670 bytes) — under the 25 MiB Cloudflare Pages limit (26,214,400 bytes) with ~800 KiB to spare. No binary splitting or alternate hosting needed.
- **No 3D asset pipeline**: procedural geometry only (extruded walls, flat floors). This is actually a strength — keeps the CLI-deploy workflow intact.

## Constraints (if we proceed)

- **Same mixing rules.** Gain, panning, LOS, and distance curves must produce identical audio output in both modes for the same player position.
- **No 3D physics.** Movement stays on the XY plane. Avian2d is not replaced.
- **No 3D model assets.** Geometry is procedural from the `Maze` grid. No .gltf files. Textures limited to album artwork (already hosted on R2) applied to wall and track cube surfaces.
- **Always included.** 3D rendering ships in the single binary (no feature gating needed — it fits).
- **Out of scope:** vertical maze levels, jumping, 3D spatial audio (HRTF), VR/AR, multiplayer.

## Approach (sketch)

### Morph as camera transition

The maze geometry doesn't change — the camera does. In 2D mode, `Camera2d` renders sprites top-down. In 3D mode, `Camera3d` renders extruded meshes at ground level. The morph is:

1. Spawn 3D meshes (walls as boxes, floors as planes) mirroring the 2D tile grid — same positions, same colors.
2. Animate camera from orthographic top-down to perspective ground-level over ~1 second.
3. Hide 2D sprites, show 3D meshes (crossfade via alpha or hard switch at midpoint).
4. Switch input handling: WASD/D-pad now controls heading + forward/back rather than cardinal movement.

Reverse morph: animate camera back, swap visibility, restore input.

### Lighting in 3D mode

Use Bevy's built-in `PointLight` at each track icon position (matching the 2D point lights). Player carries a `SpotLight` (flashlight equivalent). No custom raycast lighting — Bevy's renderer handles shadow maps.

### Artwork on surfaces

Walls become gallery surfaces. The album's background artwork (already on R2) tiles or stretches across wall faces — the player walks through the art instead of over it. Track cubes display per-track artwork (`artwork_url` in `TrackMetadata`), giving tracks visual identity at a distance down a corridor. This reuses existing R2-hosted images with no new upload pipeline.

### Rendering budget

One 3D mesh per wall tile (instanced box), one large floor quad. A 40x40 maze has ~800 wall tiles. With instancing, this is well within WASM rendering budget. No LOD needed at this scale. Texture count is low — one album background + one per track.

## Domain Events

| Event | Producer | Consumer |
|-------|----------|----------|
| `MorphRequested { target: Mode2D \| Mode3D }` | Player gesture or album script | Morph orchestrator |
| `MorphComplete { mode }` | Morph orchestrator (after animation) | Input handler swap, camera swap, sprite/mesh visibility toggle |

No new audio events — `TrackAudioState` is unchanged across modes.

## Open Questions

- **Is the visual feel worth the WASM cost?** The 2D top-down view has a distinctive aesthetic. 3D could feel generic (Minecraft-lite corridors) unless art direction is strong.
- **Does first-person perspective change mixing strategy?** In 2D top-down, the player sees the full maze layout and can plan routes to tracks visually. In first-person, the same LOS rules apply (you hear what you have line-of-sight to), but the limited field of view means you can't survey the layout — you discover corridors and track sources as you turn corners. Same rules, different feel.
- **Album-level opt-in or player choice?** If the album author controls when morphing happens, it's a compositional tool. If the player controls it, it's a preference. Different design implications.

## Checkpoints (if implemented)

- [x] `3d_bevy_render` compiles and WASM stays under 25 MiB (24.2 MiB measured)
- [ ] Procedural wall/floor meshes match tile grid positions exactly
- [ ] Camera morph animation is smooth (no pop, no z-fighting)
- [ ] Audio output is identical in 2D and 3D modes for same player position
- [ ] Existing albums without 3D flag see zero behavioral or size change
- [ ] Input handling switches cleanly between top-down and first-person
