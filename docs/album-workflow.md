# Album Workflow

How to create and deploy an album to the web player.

## Prerequisites

- Desktop app built: `cargo build --release`
- Trunk installed: `cargo binstall trunk`
- Wrangler authenticated: `wrangler login`
- Audio files converted to FLAC (or wav/mp3/ogg)

## 1. Create an Album (Desktop App)

```sh
just run
```

- Drag audio files onto the window to add tracks
- Each track gets a room in the procedurally generated maze
- Use the settings panel to:
  - Set album title and artist
  - Edit per-track title/artist metadata
  - Adjust maze generation parameters (room size, corridor length)
  - Tune lighting (ambient, player lantern, track lights)
- Enter a project name and click **Save**

## 2. Verify the Project

```sh
just list-projects
```

Saved projects are stored in:
- macOS: `~/Library/Application Support/vvw/projects/<name>/`
- Linux: `~/.local/share/vvw/projects/<name>/`

Each project directory contains `project.ron` (manifest) and `audio/` (track files as `{id}.audio`).

## 3. Local Preview

Test the web player locally before deploying:

```sh
just assemble-local MyAlbum
just preview
```

This copies audio files into the deploy directory and starts a local wrangler dev server. Open the URL printed by wrangler, then navigate to `/MyAlbum/`.

## 4. Deploy

Single command to upload audio, build WASM, assemble, and deploy:

```sh
just deploy-album MyAlbum
```

This runs three steps:
1. **upload-audio** — pushes audio files to the `vvw-audio` R2 bucket
2. **assemble** — builds the WASM player, copies `project.ron` + `index.html` into the album directory, writes `_config.json` with the R2 URL
3. **deploy-pages** — deploys to Cloudflare Pages

The album is live at `https://vvw-2c3.pages.dev/MyAlbum/`

## 5. Deploy Additional Albums

Each `deploy-album` call adds to the existing deploy directory. Multiple albums coexist:

```sh
just deploy-album SecondAlbum
```

Now both `/MyAlbum/` and `/SecondAlbum/` are live.

To remove an album:

```sh
just clean-album MyAlbum
just deploy-pages
```

## Troubleshooting

**R2 upload fails**: Large audio files (>80 MiB) can time out. Retry `just upload-audio AlbumName` — wrangler uploads are resumable per-file.

**WASM too large for Pages**: The `release-wasm` profile + wasm-opt keeps the binary under Cloudflare's 25 MiB limit. If it grows past that, check for new dependencies pulling in large code.

**Album page shows "Loading..."**: Check the browser console. Common causes:
- Typo in the album URL path (case-sensitive)
- `project.ron` missing from deploy — re-run `just assemble AlbumName`
- R2 audio CORS error — verify R2 bucket has public access enabled

## Just Recipes Reference

| Command | Description |
|---------|-------------|
| `just list-projects` | List saved desktop projects |
| `just deploy-album ALBUM` | Full deploy: R2 upload + assemble + Pages deploy |
| `just assemble-local ALBUM` | Assemble with audio for local testing |
| `just preview` | Local dev server (run assemble-local first) |
| `just upload-audio ALBUM` | Upload audio to R2 only |
| `just assemble ALBUM` | Build WASM + assemble for R2 (no audio copy) |
| `just deploy-pages` | Deploy to Cloudflare Pages only |
| `just clean-album ALBUM` | Remove an album from deploy dir |
| `just build-web` | Build WASM player only |
