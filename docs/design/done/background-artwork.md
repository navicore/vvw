# Background Artwork

## Intent

Allow the artist to place a custom image (photography, abstract art, composites) as the visual background of a maze, replacing the flat colored tiles with their own artwork. The maze geometry still exists for physics and lighting, but visually the player walks through the artist's image.

A CLI tool exports the maze layout as a mask image. The artist takes this into GIMP/Photoshop and uses it to add texture — relief bumps on walls, smooth corridors, imperfect hand-crafted edges — with full creative control over depth and roughness. The processed image is then loaded as the game background at runtime.

This turns each album into a unique visual experience tied to the artist's aesthetic.

## Constraints

- Lighting (lantern + flashlight) must continue to work — the lightmap overlay at z=90 must still modulate the background image
- Physics must be unchanged — wall colliders stay, player can't walk through walls
- Player sprite and track icons must remain visible above the background
- Must not regress WASM bundle size significantly (image loaded at runtime, not baked in)
- The artist's handmade process is the point — no procedural/automated texture generation

## Approach

**This is viable with minimal engine changes.** Key insight: the lighting system is purely grid-based (raycasts against `LightOccluderGrid` from maze wall data). It never inspects sprite visuals. A background image won't affect lighting at all.

### Maze mask export (dev-time CLI tool)

- New `vvw-deploy` subcommand: `export-maze <album> --output maze-template.png`
- Renders the maze grid as a mask image: white for corridors, dark for walls, at a configurable pixels-per-tile resolution (e.g., 8x or 16x)
- Artist takes this into GIMP, layers their artwork, uses the maze mask to add wall texture (bump mapping, relief, roughness — whatever they want)
- The mask gives the artist precise corridor boundaries while leaving creative decisions entirely in their hands
- Artist saves the result as a single image (e.g., `background.png`)

### Runtime rendering

- Background image stored alongside audio in R2 (same upload workflow as cover art)
- `project.ron` gets an optional `background_url: Option<String>` field on `AlbumMetadata`
- At startup, if present, spawn a single `Sprite` entity at z=-1.0 sized to cover the full maze
- Wall and floor tile sprites become fully transparent (or are not spawned)
- Player (z=1-2) and track icons (z=1.0) render above the background
- Lightmap overlay (z=90.0) still applies darkness/brightness on top — corridors glow, walls stay dark

### Why lighting just works

The lightmap is computed per-tile via Bresenham raycasting against the `LightOccluderGrid`. The grid is populated from `maze.is_wall(x,y)`. The visual appearance of tiles is irrelevant — the grid doesn't change. The lightmap texture is rendered as a single overlay sprite with per-tile alpha, which will modulate the background image exactly as it modulates colored tiles today.

## Domain Events

- **New CLI event**: `export-maze` produces a PNG from the maze grid (read-only, no side effects)
- **New manifest field**: `background_url` on `AlbumMetadata` (serde default, backwards compatible)
- **Startup**: if `background_url` is set, load image, spawn background sprite, suppress tile sprite colors
- No changes to lighting systems, physics, audio, or spatial tracking

## Checkpoints

1. `export-maze` produces a recognizable maze mask at correct pixel dimensions
2. Artist can use the mask in GIMP to texture their image against the maze layout
3. Processed image loads at runtime and displays behind the lightmap
4. Lighting (both lantern and flashlight) looks correct over the artwork
5. Player and track icons are visible and properly lit
6. Physics unchanged — player cannot walk through walls
7. Albums without a background image behave exactly as before

## Future: runtime texture generation

The handmade mask workflow assumes a static maze. If maze-sculpting (see `maze-sculpting.md`) is enabled for art-background albums, the artist's pre-baked image would no longer match the modified maze. That scenario would require runtime texture processing — applying the artist's style/texture rules procedurally to newly carved or filled tiles. This is a separate design problem to revisit if/when sculpting and background artwork need to coexist.
