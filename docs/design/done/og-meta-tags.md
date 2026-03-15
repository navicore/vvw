# Open Graph Meta Tags

## Intent

Add Open Graph and Twitter Card meta tags to per-album `index.html` so that album links render as rich cards (title, artist, cover art, description) when shared on social media, chat apps, and link previews. Currently all albums share identical `index.html` with a generic "VVW Player" title — crawlers see nothing album-specific because metadata is injected at runtime by WASM.

## Constraints

- OG crawlers do not execute JavaScript — tags must be in the static HTML at deploy time
- Must not break existing index.html structure (Trunk-generated JS/WASM references)
- Cover art URL must resolve to the R2 public URL, not a relative path
- Albums without cover art should still get title/description tags (just no `og:image`)
- No new dependencies in vvw-deploy
- Out of scope: oEmbed endpoint, embedded player iframe, `twitter:player` card, `og:audio`

## Approach

Modify `assemble.rs` to inject OG meta tags into each album's `index.html` at assembly time:

1. Read `project.ron` for each album (already verified to exist)
2. Deserialize `AlbumMetadata` (title, artist, description, cover_art_url)
3. Read the Trunk-built `index.html` as a string
4. Build an OG tag block and insert it before `</head>`
5. Replace `<title>VVW Player</title>` with `<title>{album title} — {artist}</title>`
6. Write the modified HTML to the album output directory

Tags to inject:

```html
<meta property="og:title" content="{title} — {artist}">
<meta property="og:description" content="{description}">
<meta property="og:image" content="{cover_art_url}">
<meta property="og:url" content="https://vvw-2c3.pages.dev/{album}/">
<meta property="og:type" content="website">
<meta name="twitter:card" content="summary_large_image">
```

Cover art URL resolution: if `cover_art_url` is relative (e.g. `cover.jpg`), prefix with `audio_base_url` from the deploy command. If absolute, use as-is. If `None`, omit `og:image` and use `summary` instead of `summary_large_image` for the Twitter card.

The album's canonical URL (`og:url`) uses the Pages domain + album slug. The base domain could be passed as a CLI arg or derived from an existing config.

## Domain Events

- **Consumed**: `ProjectManifest` (album metadata, cover art URL), `audio_base_url` (from CLI), Trunk dist `index.html`
- **Produced**: per-album `index.html` with OG meta tags injected
- No runtime changes — vvw-web and vvw-game are untouched

## Checkpoints

1. Deploy an album with cover art — paste URL into Twitter Card Validator, Discord, or Slack — card shows title, artist, description, and cover image
2. Deploy an album without cover art — card shows title and description, no broken image
3. View page source of deployed album `index.html` — OG tags present in `<head>`
4. WASM player still loads and functions normally after HTML modification
5. `<title>` shows album name instead of generic "VVW Player"
