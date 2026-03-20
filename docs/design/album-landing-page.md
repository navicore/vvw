# Album Landing Page — Design

## Intent

Every maze should have a public-facing album page — a Bandcamp-style landing page with cover art, track listing with inline players, artist info, and a prominent call-to-action to enter the maze. This is the URL you share. The maze is the experience you enter from it.

**Why.** The album page is the artist's definitive home for new work. A visitor can listen end-to-end with zero cognitive load — just press play. The maze (or any future interactive experience) is one click away at `/play/`, but the album page stands on its own as a complete listening destination. Think Bandcamp: the page IS the release, and the maze is a bonus experience.

## URL Structure

**Recommendation: album page at root, maze one level deeper.**

```
navicore.tech/catastrophe-coin/          → album landing page (static HTML)
navicore.tech/catastrophe-coin/play/     → maze (WASM player, current index.html)
```

Rationale:
- The album page is the shareable URL. It gets the OG tags, the social preview, the search indexing. Root path is cleanest for this.
- `/play/` serves whatever interactive experience the artist chose for this album (maze today, potentially other WASM experiences in the future). One game type per album — `/play/` is the single entry point.
- Single domain, single Cloudflare Pages deployment. No new TLD, no DNS changes, no CORS complexity.
- Existing OG tag injection moves from the maze `index.html` to the album page `index.html`. The maze page gets minimal meta (or inherits via og:url pointing to the album page).

**Alternative considered: separate subdomain** (`albums.navicore.tech` vs `maze.navicore.tech`). Rejected — two deployments to keep in sync, two Pages projects, more operational overhead for no user benefit. The maze and album page share the same data (`project.ron`, R2 audio). Co-locating them is simpler.

**Alternative considered: album page as a subdirectory** (`/catastrophe-coin/album/`). Rejected — the album page is the primary shareable artifact, not the maze. Putting the more discoverable content at the root is conventional (Bandcamp, Spotify, SoundCloud all do this).

## Constraints

- **Static HTML only.** No server, no JS framework. Generated at `assemble` time from `project.ron` + `AlbumMetadata` + `TrackMetadata`. Same as current OG tag injection, but producing a full page.
- **No per-track pages.** Single page per album. Track players are inline `<audio>` elements.
- **Full sequential playback.** The page player should support playing through the full tracklist — a visitor can hear the whole album without interacting beyond pressing play once.
- **Audio from R2.** Same `<audio>` streaming as the maze. Same CORS, same `.audio` extension, same `_config.json` URL resolution.
- **No backward-compatibility concern.** All existing albums are owned by the developer. Existing URLs (`/Album/`) will change from maze to landing page. No redirects needed.
- **Out of scope:** purchase/download, user accounts, comments, analytics, custom CSS per album.

## Approach

### Page generation in `assemble`

The `assemble` command already parses `project.ron` and injects OG tags into a template. Extend this to:

1. Generate `{album}/index.html` — the landing page (new HTML template, static, no WASM).
2. Move the WASM player to `{album}/play/index.html` (current template, unchanged).
3. OG tags go on the landing page. The maze page gets a minimal `<meta>` pointing `og:url` back to the landing page.

The landing page template lives in `crates/vvw-web/` alongside `index.html` (e.g. `album.html`). It's a single self-contained HTML file with inline CSS — same pattern as the current player template. `assemble` interpolates album/track data into it.

### Page content

- **Header:** Album title, artist, release date.
- **Cover art:** Large image from `cover_art_url` (resolved via R2 same as OG image).
- **Description:** Album description text.
- **Track listing:** Ordered list. Each track shows: number, title, artist (if different from album artist), duration. Two playback gestures (Bandcamp model):
  - **Big play button** — plays from the artist-designated default track (usually first, tagged in metadata) through the end of the tracklist. This is the "play the album" action.
  - **Per-track play button** — plays that single track only, then stops.
  - No shuffle, repeat, or queue manipulation. The maze is the deep listening control surface.
  - Audio src resolves same as maze (R2 base URL + album + filename).
  - Default track specified via `default_track` in `AlbumMetadata` (index into tracklist, default 0).
- **Enter the Maze:** Prominent button/link to `/Album/play/`. Framed as the interactive listening experience. Visible but not competing with the player — the page works as a standalone destination even if the visitor never enters the maze.
- **Links with Open Graph:** Album-level external links (Bandcamp, Spotify, artist site — already in `AlbumMetadata.links`). The landing page's OG tags promote these links so that social previews, link unfurling, and search results surface the album's external presence. `og:see_also` for linked platforms where applicable.
- **Future-proofing `/play/`:** The "Enter the Maze" CTA links to `/Album/play/`. Today that's the maze. In the future, the artist could choose a different WASM experience per album. The landing page doesn't need to know what `/play/` serves — it just links to it.

### Existing URLs

Current albums live at `/Album/` and serve the maze directly. After this change, `/Album/` serves the landing page and `/Album/play/` serves the maze. All existing albums are owned by the developer — no redirect machinery needed. The landing page is a strictly better first impression than the raw WASM canvas.

## Domain Events

| Event | Producer | Consumer |
|-------|----------|----------|
| `assemble` generates landing page HTML | `assemble` command | Cloudflare Pages deploy |
| `assemble` moves maze to `/play/` subdir | `assemble` command | `_routes.json`, `_headers` updates |

No runtime events. This is entirely a build-time change in `vvw-deploy`.

## Checkpoints

- [ ] `just assemble` produces `{album}/index.html` (landing page) and `{album}/play/index.html` (maze)
- [ ] Landing page renders cover art, track list, artist info, description, links
- [ ] Each track has a working `<audio>` inline player streaming from R2
- [ ] "Enter the Maze" link navigates to `/Album/play/`
- [ ] OG tags on landing page match current social preview quality
- [ ] `just preview` serves both pages locally
- [ ] `_headers` caching rules cover the new path structure
- [ ] Maze at `/Album/play/` loads `project.ron` correctly (relative path adjustment)
