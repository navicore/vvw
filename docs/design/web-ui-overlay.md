# Design: Web UI Overlay

## Intent

Wrap the WASM game canvas in a proper web page. The game currently fills the
entire viewport with no context. We want:

- Album header (title, artist, cover art) above the canvas
- Centered canvas that doesn't fight the page layout
- Track info foldout that slides down when you click a track icon in the game
- Foundation for future track art and richer album presentation

The player should feel like a music experience, not a raw game embed.

## Constraints

- **Don't touch vvw-game.** All UI lives in HTML/CSS + vvw-web glue code.
- **WASM size budget.** No new Rust crate dependencies for UI — use DOM via web-sys.
- **Autoplay policy.** Click-to-start must remain synchronous in the gesture handler.
- **Metadata already available.** `AlbumMetadata` and `TrackMetadata` are in the manifest.
  No backend changes needed.
- **Out of scope for phase 1:** track art images, album cover upload pipeline,
  responsive mobile layout, playlist/queue controls.

## Approach

**HTML/CSS layer (index.html):**
- Replace full-viewport canvas with a page layout: header + centered canvas container + hidden foldout panel
- Header: `#album-title`, `#album-artist`, placeholder for cover art
- Canvas container: fixed max-width, centered, dark background
- Foldout: `#track-info` div below canvas, hidden by default, slides open with CSS transition

**Bevy → DOM bridge (vvw-web):**
- When player clicks a `TrackIcon` entity in the game, fire a custom DOM event
  with the `track_id`
- Add Bevy picking/interaction on `TrackIcon` entities (they already have `Component`)
- JS listener on the custom event populates `#track-info` with that track's
  `TrackMetadata` (title, artist, description, lyrics, links) and toggles it open
- Clicking the same track or a close button collapses the foldout

**Data flow:**
1. `ui::populate_album_info()` — already runs at startup, extend to populate header
2. `ui::populate_track_list()` — new: serialize track metadata into a JS-accessible
   structure (hidden `<script type="application/json">` or data attributes)
3. Bevy system watches for `TrackIcon` click → calls `web_sys` to dispatch
   `CustomEvent("track-select", { detail: track_id })`
4. Inline JS listener reads track data, updates foldout DOM, toggles visibility

**Phasing:**
1. Layout + header (canvas no longer full-viewport, album info visible during play)
2. Track foldout (click track icon → info panel)
3. Cover art + track art (requires image upload pipeline in vvw-deploy)

## Checkpoints

- [ ] Album title and artist visible above the canvas during gameplay
- [ ] Canvas is centered with dark surround, not stretched to viewport edges
- [ ] Clicking a track icon in the maze opens a foldout with track title, artist, description
- [ ] Clicking again (or another track) closes/swaps the foldout
- [ ] Start screen overlay still works (click-to-start, autoplay unlock)
- [ ] Deployed to Cloudflare Pages, no layout breakage
