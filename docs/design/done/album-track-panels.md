# Design: Album & Track Info Panels

## Intent

Make the album header clickable to reveal album cover art and the artist's description, overlaying the top of the game canvas. The existing track info bar (bottom) gains optional per-track artwork. Both panels float over the game without pushing it around — the canvas never resizes or shifts.

**Why.** The music is the point. Album and track context (art, liner notes, descriptions) should be one tap away. Current UI shows only title/artist in a thin bar. Artists want to present their work with visual identity.

## Constraints

- **No layout shift.** Both panels use `position: absolute` with `z-index` to overlay the canvas. The canvas size and position never change. We already do this for the track info bar — same pattern.
- **WebGL canvas stacking is solved.** Canvas has `z-index: 1`, overlays use `z-index: 2+`. Touch passthrough via `pointer-events: none` on panel backgrounds, `pointer-events: auto` on interactive elements. Already proven on mobile.
- **Album art is external.** `AlbumMetadata.cover_art_url` already exists in the data model. Track art needs a new `artwork_url: Option<String>` on `TrackMetadata`.
- **No WASM changes for layout.** Panels are pure HTML/CSS/JS in `index.html`. WASM only dispatches events with metadata payloads (already done for tracks).
- **Don't break D-pad.** Panels must not consume touch events meant for movement controls.
- **Image sizing.** CSS constrains artwork to a fixed box (`object-fit: cover`) so any image dimension works. For sharp rendering on retina/high-DPI displays, recommend source images at least 160x160px. Smaller images work but may appear soft on high-DPI screens.
- **Out of scope:** image upload/hosting (user provides URLs), image cropping/resizing, animation beyond slide transitions.

## Approach

### Album panel (drops down from header)

- Click/tap on album title in `#album-header` toggles an `#album-detail` panel.
- Panel: `position: absolute; top: <header-height>; z-index: 3;` — slides down over canvas.
- Contains: cover art `<img>` (if `cover_art_url` set), description text, links.
- WASM injects album metadata into a hidden `#album-data` div at startup (same pattern as `#track-data`).
- JS reads from `#album-data` on click — no new WASM events needed.

### Track panel (folds up from bottom)

- Already exists as `#track-info`. Expand it to include artwork.
- Add `<img>` to `#track-info-inner`, shown only if `data-artwork-url` attribute is set on the track's metadata div.
- Art displays left of text, constrained to ~80px square.
- No new events — `track-select` already carries the track ID; JS reads artwork URL from `#track-data`.

### Data model change

Add to `TrackMetadata` in `vvw-core/src/project.rs`:
```rust
#[serde(default)]
pub artwork_url: Option<String>,
```

`AlbumMetadata.cover_art_url` already exists.

### WASM changes

- `ui::inject_track_metadata` — add `data-artwork-url` attribute to track div.
- `ui::set_album_info` — inject album description, cover art URL, and links into `#album-data`.

## Domain Events

No new Bevy events. Album panel is pure JS toggle on click. Track panel already uses `track-select` / `track-hide` custom DOM events.

| Interaction | Handler | Effect |
|---|---|---|
| Click album title | JS click listener | Toggle `#album-detail.open` class |
| `track-select` event | Existing JS | Show track info + artwork if available |
| Click outside panel | JS click listener | Close album panel |

## Checkpoints

- [ ] Click album title — panel drops down over canvas with cover art and description
- [ ] Click again or outside — panel closes
- [ ] Track with `artwork_url` — artwork shows in track info bar
- [ ] Track without `artwork_url` — track info bar unchanged (no broken image)
- [ ] Canvas never shifts or resizes when panels open/close
- [ ] D-pad still works while album panel is open
- [ ] Mobile: panels render correctly, touch targets work
- [ ] Albums without `cover_art_url` — album panel shows description only, no broken image
