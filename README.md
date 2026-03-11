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

## Album Creation

```sh
# Create an album from a directory of audio files
just create-album ./my-audio-files/

# Deploy: upload audio to R2, build WASM, assemble, deploy to Pages
just deploy-album "My Album"

# Local preview
just assemble-local "My Album"
just preview
```

### Controls

| Key | Action |
|-----|--------|
| W / Arrow Up | Move up |
| S / Arrow Down | Move down |
| A / Arrow Left | Move left |
| D / Arrow Right | Move right |

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
