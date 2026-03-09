# Roadmap

## Current State

The web player is deployed and functional on Cloudflare Pages. Albums are created via the desktop app (`vvw-app`), then deployed with `just deploy-album`. Audio streams from Cloudflare R2. The WASM binary is size-optimized to stay under Cloudflare's 25 MiB limit.

22 tests pass. CI runs on Linux via `just ci`.

## In Progress

### Remove Desktop App (planned)
Replace `vvw-app` (Bevy GUI) and `vvw-audio` (kira) with a CLI subcommand `vvw-deploy create`. See `docs/remove-desktop-plan.md` for the full plan. Key steps:
- Add `Create` subcommand to `vvw-deploy` (scan audio dir, generate maze, write `project.ron`)
- Strip `TrackHandles` from `vvw-game` (simplify to state-only audio bridge)
- Delete `vvw-app` and `vvw-audio` crates
- Clean up workspace config and CI

## Next Up

- **Web UI overlay**: Render album title, artist, and track info around the Bevy canvas
- **Player avatar**: Improve the player sprite rendering
- **Add `.wrangler/` to `.gitignore`**

## Known Limitations

- No visual maze editor once desktop app is removed — layout is fully procedural
- No real-time lighting preview — tuning requires deploy-and-check cycles
- Large audio uploads to R2 can time out (wrangler retries help)
- macOS CI may be unnecessary once desktop app is removed
