# VVW - Visual Virtual World

An audio exploration game where you navigate a 2D maze to discover and experience spatial audio. As you move through the maze, nearby audio tracks grow louder and distant ones fade away — all driven by line-of-sight and distance.

Built with [Bevy](https://bevyengine.org/) and compiled to WASM. Deployed on Cloudflare Pages with audio streaming from R2.

## How It Works

You control a player navigating a tile-based maze. Scattered throughout the maze are audio track icons, each playing a different track from an album. Volume and stereo panning are determined by line-of-sight and distance — walk toward a track and it gets louder, lose line of sight and it fades to silence.

## Prerequisites

- Rust 1.93+ (edition 2024)
- [just](https://github.com/casey/just) (command runner)
- [Trunk](https://trunkrs.dev/) (WASM build tool)
- [wrangler](https://developers.cloudflare.com/workers/wrangler/) (Cloudflare CLI, for deploy)

## Quickstart

```sh
# 1. Create an album from a directory of audio files (opens $EDITOR for metadata)
just create-album ~/my-audio-files/

# 2. Upload audio to R2 (only needed once, or when audio files change)
just upload-audio my-album

# 3. Deploy all albums to Cloudflare Pages (builds WASM, assembles all projects)
just deploy
```

Each album is a saved project in `~/Library/Application Support/vvw/projects/`.
Every `just deploy` builds and deploys **all** saved projects — Cloudflare Pages
deploys are atomic full-site replacements, so all albums must be included.

Audio lives on R2 (independent per-album uploads). The Pages deploy contains
only the WASM player, album manifests, and config.

### Other commands

```sh
# Edit album config (lighting, physics, etc.)
$EDITOR ~/Library/Application\ Support/vvw/projects/my-album/project.ron

# Local preview (includes audio in deploy dir)
just assemble-local my-album
just preview

# Remove an album from deploy and R2
just delete-album my-album
```

### Controls

| Key | Action |
|-----|--------|
| W / Arrow Up | Move up |
| S / Arrow Down | Move down |
| A / Arrow Left | Move left |
| D / Arrow Right | Move right |

## Background Artwork

You can replace the flat maze tiles with a custom background image — photography, textures, abstract art — while keeping the maze physics and lighting intact. The lighting overlay renders on top of your artwork, so lanterns and flashlights still work.

### Step 1: Export the maze mask

```sh
# Export a grayscale mask (white corridors, dark walls)
# Scale controls pixels-per-tile (8 is good for smaller mazes, 16 for detail)
just export-maze my-album --output maze-mask.png --scale 8
```

### Step 2: Create artwork in GIMP

**Set up layers:**

1. Open your artwork image in GIMP (photography, abstract art, etc.)
2. Open `maze-mask.png` as a new layer (File > Open As Layers). It goes on top.
3. Scale the maze mask layer to match your artwork dimensions if needed (Layer > Scale Layer)

**Apply bump map for wall texture:**

1. Select the **artwork layer** (click it in the Layers panel — it must be the active layer)
2. Go to Filters > Map > Bump Map
3. In the Bump Map dialog, set **Aux Input** to the maze mask layer (click the image picker next to "Aux Input" to select it)
4. Adjust Depth, Azimuth, and Elevation to taste. Higher depth = more pronounced wall relief.
5. Click OK. The artwork now has a 3D textured feel along wall boundaries.

**Add a texture layer (optional):**

1. Open a texture image as a new layer (File > Open As Layers). Place it **between** the artwork and the maze mask:
   - Bottom: artwork (with bump map applied)
   - Middle: texture
   - Top: maze mask (will become the stencil)
2. Select the **texture layer** in the Layers panel
3. Add a layer mask: Layer > Mask > Add Layer Mask, choose "White (full opacity)"
4. Select the **layer mask** (click the white thumbnail next to the texture layer)
5. Copy the maze mask to clipboard: click the maze mask layer, Select > All, Edit > Copy
6. Click back on the texture layer's **mask thumbnail**, then Edit > Paste, then Layer > Flatten Image (or anchor the floating selection)
7. The texture now shows only on wall areas. Adjust the texture layer's opacity/blend mode to taste.
8. Select > None (Ctrl+Shift+A) to clear any selection
9. Hide or delete the maze mask layer — it's served its purpose

**Export the result:**

1. Hide the maze mask layer if still present
2. File > Export As > `background.jpg` (or `.png`). JPEG is fine — the player loads it at runtime.

### Step 3: Upload and configure

```sh
# Put background.jpg in the album's audio directory and upload
just upload-audio my-album

# Edit project.ron and add background_url to the album section:
$EDITOR ~/Library/Application\ Support/vvw/projects/my-album/project.ron
```

In `project.ron`, add to the `album` block:

```ron
background_url: Some("background.jpg"),
```

### Step 4: Deploy

```sh
just build-web && just deploy
```

The background image renders at z=-1 (behind everything), maze tile sprites become transparent, and the lightmap overlay at z=90 modulates brightness over your artwork. Player and track icons remain visible above.

## Development

```sh
# Run the full CI check suite locally (same as GitHub Actions)
just ci

# See all available commands
just
```

## Project Structure

```
crates/
  vvw-core/     # Platform-agnostic types: maze, spatial audio, physics, lighting
  vvw-game/     # Bevy plugin: maze rendering, player, spatial audio interpolation
  vvw-light/    # 2D lighting plugin: point lights, occluders, ambient
  vvw-web/      # WASM web player: Web Audio API, overlay UI
  vvw-deploy/   # CLI: album creation, assembly, R2 upload, Pages deploy
```

## License

MIT
