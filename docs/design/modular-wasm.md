# Modular WASM — Crate Design vs. Compiled Artifact

## Intent

The WASM binary is 21 MiB today and growing. Adding 3D rendering could push it past Cloudflare's 25 MiB per-file limit. But the size question is really a design question: how do we think about modularity — by crate (how we reason about the system) versus by compiled artifact (what ships to the browser)?

Good crate boundaries help us think. Good artifact boundaries help us ship. They don't have to be the same.

## Current State

**Crate graph (reasoning boundaries):**
```
vvw-core          ← types, no rendering, no bevy (optional)
vvw-light         ← 2D lighting plugin
vvw-game          ← ECS plugins: audio, physics, modes, pipes, breadcrumbs, mute
vvw-web           ← WASM entry point, Web Audio, DOM overlay
vvw-deploy        ← CLI, native-only, never in WASM
```

**Compiled artifact (shipping boundary):**
```
vvw_web_bg.wasm   ← single monolithic binary, everything linked
```

The crate structure is good for reasoning — `vvw-core` is platform-free, `vvw-game` is render-agnostic, `vvw-web` is the platform layer. But it all compiles into one file. Every album ships every feature whether it uses them or not.

## Two Axes of Modularity

### 1. Modularity by crate (design-time)

This is what we have. Crates enforce dependency direction and separation of concerns. Adding a new crate (e.g. `vvw-3d` for 3D rendering) helps us reason about what depends on what, even if it compiles into the same binary.

**What this buys us:**
- Clear ownership: `vvw-light` doesn't know about `vvw-game`
- Testability: `vvw-core` tests run without Bevy
- Compile-time feature gates: `vvw-web` could optionally depend on `vvw-3d`

**What this doesn't solve:**
- Binary size — unused code is only eliminated by LTO dead code removal, which is imperfect
- Load time — browser downloads everything upfront

### 2. Modularity by artifact (deploy-time)

Split the WASM output into multiple files that load independently. The browser fetches the base player immediately and loads extensions on demand.

**Options, from least to most invasive:**

#### A. Cargo feature flags (single binary, conditional compilation)

```toml
[features]
default = []
render-3d = ["bevy/3d_bevy_render"]
```

`vvw-deploy` builds different WASM binaries per album based on `project.ron` flags. An album with `morph_3d: true` gets the 3D-enabled build. Albums without it get the smaller binary.

- **Pros:** No runtime complexity. Each album gets exactly the code it needs. Trunk and wasm-bindgen work as-is.
- **Cons:** Multiple builds per deploy. CI matrix grows. Two albums on the same Pages site can't share a single WASM file if they need different features.
- **Size impact:** Each binary is smaller than a monolith with everything. 3D-enabled build might be 23-24 MiB; 2D-only stays at 21 MiB.

#### B. Per-album WASM builds (multiple binaries, same site)

Extend option A: each album subdirectory gets its own WASM binary instead of sharing one from the site root.

```
deploy/
├── AlbumA/
│   ├── index.html
│   ├── project.ron
│   ├── vvw_web_bg.wasm    ← 2D-only, 21 MiB
│   └── vvw_web.js
├── AlbumB/
│   ├── index.html
│   ├── project.ron
│   ├── vvw_web_bg.wasm    ← 3D-enabled, 24 MiB
│   └── vvw_web.js
```

- **Pros:** Each album is self-contained. Feature set matches the album. No sharing conflicts.
- **Cons:** Duplicated WASM across albums (browser cache helps if filenames match). Build time multiplies. Total site size grows (but Cloudflare Pages limit is per-file, not per-site).
- **Current deploy already supports this:** `assemble` copies WASM to the root, but moving it per-album is a small change to `assemble.rs` + the HTML template's script path.

#### C. Lazy-loaded second WASM module (two binaries, runtime handoff)

Base player loads as 2D-only. When the player triggers a 3D morph, JS fetches and instantiates a second WASM module containing the 3D-enabled Bevy app. Game state serializes across the boundary.

- **Pros:** Base load stays at 21 MiB. 3D code only downloaded when needed. No per-file size limit issue.
- **Cons:** Two full Bevy apps. State serialization is fragile (player position, audio state, maze mutations, active modes, pipe placements). AudioContext and canvas must survive the swap. Significant engineering effort.
- **Browser security:** Not an issue. Multiple WASM modules on the same origin are fully supported. Each is fetched and instantiated independently via `WebAssembly.instantiateStreaming()`.

#### D. WASM component model (future)

The WASM component model (wasm-tools, wit-bindgen) enables true module linking — shared memory, typed interfaces between modules. Rust support is maturing but not production-ready for Bevy-scale apps. Worth tracking but not viable today.

## Recommendation

**Start with A (feature flags), graduate to B (per-album builds) if needed.**

Feature flags are the lowest-risk path. They use existing tooling, produce a single binary per build, and let us measure actual size impact before committing to more complex approaches. The `vvw-deploy` CLI already knows each album's feature set from `project.ron` — extending it to select cargo features at build time is natural.

Per-album builds (B) become worthwhile when the feature matrix grows beyond 3D — if sculpting, multiplayer, or other heavy features each add significant WASM weight, per-album builds let each album pay only for what it uses.

Lazy loading (C) is the escape hatch if a single feature genuinely can't fit in 25 MiB alongside the base. Hold it in reserve.

## Changes Needed (for option A)

### vvw-web/Cargo.toml
```toml
[features]
default = []
render-3d = ["vvw-3d"]

[dependencies]
vvw-3d = { path = "../vvw-3d", optional = true }
```

### vvw-web/src/lib.rs
```rust
#[cfg(feature = "render-3d")]
app.add_plugins(vvw_3d::Morph3dPlugin);
```

### vvw-deploy (build integration)
- Read `morph_3d` from `project.ron`
- Pass `--features render-3d` to trunk when building for albums that need it
- Cache builds by feature set to avoid redundant recompilation

### Justfile
```
build-web-3d:
    trunk build --release --features render-3d
```

## Domain Events

No new domain events. This is a build/deploy concern, not a runtime concern. The feature flag gates plugin registration at app startup — once running, the app doesn't know or care whether it was built with optional features.

## Checkpoints

- [ ] Add `render-3d` feature flag to `vvw-web` — verify 2D-only build produces identical binary to today
- [ ] Build with `--features render-3d` — measure WASM size delta
- [ ] Per-album build path in `vvw-deploy` — verify correct features selected from `project.ron`
- [ ] Two albums on same site with different feature sets — verify both load correctly
