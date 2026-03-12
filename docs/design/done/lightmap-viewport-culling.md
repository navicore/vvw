# Lightmap Viewport Culling

## Intent

`update_lightmap` in `vvw-light/src/render.rs` recomputes brightness for **every tile in the maze** every frame, even though only a small viewport is visible. With a player light radius of ~10 tiles the bounding box is manageable, but the pixel-write loop always touches the full `w*h` texture. When maze dimensions grew past ~150 tiles wide during maze generation tuning, frame rate dropped noticeably (jerky movement in WASM).

**Goal:** Make lightmap cost proportional to the **visible screen area** (constant) rather than **total maze size** (variable), so maze dimensions no longer gate performance.

## Constraints

- Must not change visual output — lighting should look identical.
- Bilinear filtering on the overlay sprite must still work (no seams or edge artifacts).
- `LightOccluderGrid`, `PointLight2d`, and `AmbientLight2d` APIs unchanged.
- No unsafe code (workspace forbids it).
- Keep it simple — this is a single-file change in `render.rs`.

**Out of scope:** Multi-light optimization (spatial index, chunked updates), GPU-side lighting, dynamic occluder updates. Those are future work if needed.

## Approach

1. **Pass camera into `update_lightmap`** — query `Camera` + `GlobalTransform` to compute the visible tile rect.
2. **Clamp light bounding boxes** — intersect each light's tile-space AABB with the camera's visible tile rect. Skip lights entirely outside the viewport.
3. **Clamp pixel-write loop** — only write RGBA data for tiles within the visible rect. Fill the rest with the ambient darkness value (or skip if the texture retains prior values — needs testing).
4. **One-tile border padding** — expand the visible rect by 1 tile on each side so bilinear filtering at screen edges samples correct neighbors.

The brightness buffer can also be scoped to the visible rect, but the current `Vec<f32>` reuse pattern is already cheap — this is optional.

## Checkpoints

- [ ] Generate a large maze (20+ tracks with long corridors) and confirm smooth 60fps in WASM
- [ ] Visual diff: screenshot before/after with same maze seed — lighting identical
- [ ] No regression on current maze sizes (12 tracks, compact corridors)
- [ ] `just ci` passes (clippy, tests, WASM build)
