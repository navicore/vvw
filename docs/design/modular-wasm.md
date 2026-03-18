# Modular WASM — Runtime Module Loading

## Intent

The WASM binary is 21 MiB today. Adding 3D rendering could push it past Cloudflare Pages' 25 MiB per-file limit. We need to load a second WASM module at runtime from within a running Bevy app — not compile-time feature flags (those produce one monolithic binary), but two separate `.wasm` files where the second is fetched on demand.

This must be validated before committing to 3D work.

## The Problem

Rust compiles to a single WASM binary via wasm-bindgen + trunk. There's no dynamic linking, no `dlopen`, no way to add code after the module is instantiated. Bevy's ECS, renderer, and asset server are all statically linked. You can't load a Bevy plugin from a separate `.wasm` file.

Browsers fully support loading multiple WASM modules on the same page — `WebAssembly.instantiateStreaming()` works for any number of modules on the same origin. The constraint is Rust/Bevy, not the browser.

## Approaches

### A. Two Bevy apps with state handoff

Build two separate WASM binaries from two crate targets:

- `vvw-web` (21 MiB) — the current 2D player
- `vvw-web-3d` (24 MiB) — 3D-enabled player with `bevy/3d_bevy_render`

When the player triggers a 3D morph:

1. The running 2D app serializes game state (player position, heading, active modes, pipe placements, mute state, maze mutations) to a JS-accessible buffer
2. JS tears down the 2D WASM module
3. JS fetches and instantiates `vvw-web-3d`
4. The 3D app deserializes state and resumes

**What needs to survive the swap:**
- `AudioContext` and `<audio>` elements — owned by JS, not WASM. They survive if we don't destroy them. The new WASM module reconnects via `createMediaElementSource()`.
- Canvas — same `<canvas>` element, new Bevy app takes it over.
- Game state — serialized via `serde` through `wasm-bindgen`. `vvw-core` types are already `Serialize`/`Deserialize`.

**What breaks:**
- Bevy's ECS is gone. All entities, components, resources rebuilt from serialized state.
- Any in-flight audio (gain ramps, streaming state) resets. Brief audio glitch during swap.
- The swap is not a smooth morph — it's a hard cut with a loading screen.

**Effort:** Medium-high. Two build targets, state serialization contract, JS orchestration layer.

### B. WASM module as computation sidecar

Keep the main Bevy app running. Load a second WASM module that does 3D mesh generation as a pure computation — it takes maze data in, returns vertex buffers out. The main app creates Bevy meshes from those buffers.

- Second module is small (no Bevy, no ECS, just geometry math)
- Main module stays under 25 MiB
- No state handoff — main app stays alive
- 3D rendering still needs `bevy/3d_bevy_render` in the main binary

**Problem:** This doesn't solve the size constraint. The 3D renderer is in Bevy, not in the sidecar. The sidecar only helps if the heavy code is custom (e.g., a large procedural generation library), not if it's Bevy's built-in 3D pipeline.

### C. Web Worker with OffscreenCanvas

Run the 3D Bevy app in a Web Worker with `OffscreenCanvas`. The main thread keeps the 2D app or just handles audio/UI. The worker loads its own WASM module.

- Each WASM module is under 25 MiB independently
- No state serialization needed if they communicate via `postMessage`
- `OffscreenCanvas` support: Chrome/Edge yes, Safari 16.4+, Firefox yes

**Problem:** Bevy doesn't support `OffscreenCanvas` yet. The Bevy WASM renderer assumes it owns the main thread's canvas. This is a Bevy upstream issue, not something we can work around easily.

### D. Don't use Cloudflare Pages for WASM hosting

Host the WASM binary on R2 (no file size limit) and load it via `fetch()` + `WebAssembly.instantiateStreaming()` from a custom JS loader. The HTML page on Cloudflare Pages is tiny; it just bootstraps the WASM from R2.

- No 25 MiB constraint at all
- Single monolithic binary, no splitting needed
- R2 is already set up for audio streaming
- Custom loader replaces trunk's generated JS (moderate effort)

## Proof of Concept — What to Build

To validate that runtime WASM module loading works in this project, build the simplest possible version of approach A:

### Spike: load a second WASM module from JS and call into the running app

1. Create a tiny second crate (`crates/vvw-probe`) that compiles to WASM via wasm-bindgen. It exports one function: `probe() -> String` that returns a version string.
2. Add JS in `index.html` that, on a button click, fetches `vvw_probe_bg.wasm`, instantiates it, and calls `probe()`.
3. Display the result in the DOM.
4. Deploy both `.wasm` files to Cloudflare Pages and verify both load.

This proves:
- Two `.wasm` files on the same page, same origin
- Lazy loading (second module fetched on demand, not at page load)
- Both files under 25 MiB individually
- wasm-bindgen works for both modules independently
- The deploy pipeline can handle multiple WASM artifacts

It does NOT prove state handoff, canvas sharing, or AudioContext survival — those are approach A concerns for a later spike if the basic loading works.

### Alternative spike: host WASM on R2 (approach D)

1. Upload the existing `vvw_web_bg.wasm` to R2 alongside audio files
2. Replace trunk's JS loader with a custom `<script>` that fetches WASM from R2
3. Deploy the HTML-only site to Pages, verify the app loads WASM from R2

This proves the size constraint is irrelevant and avoids the two-module complexity entirely.

## Recommendation

**Try approach D first.** If we can serve WASM from R2, the 25 MiB limit disappears and we never need to split modules. The WASM binary is already content-hashed; R2 has no file size limit; the infrastructure is already in place. One afternoon of work to rewrite the JS loader vs. weeks of work for state serialization.

If R2 hosting doesn't work (CORS, latency, caching issues), fall back to the two-module spike (approach A).

## Checkpoints

- [ ] Upload existing WASM to R2, load from custom JS — does the app start?
- [ ] Measure load time delta (R2 vs Pages for WASM delivery)
- [ ] If R2 works: update deploy pipeline, close the modular WASM question
- [ ] If R2 fails: build the two-crate spike, validate lazy loading on Pages
