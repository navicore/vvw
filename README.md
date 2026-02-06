# VVW - Visual Virtual World

An audio exploration game where you navigate a 2D maze to discover and experience spatial audio. As you move through the maze, nearby audio tracks grow louder and distant ones fade away.

Built with [Bevy](https://bevyengine.org/) and [cpal](https://github.com/RustAudioGroup/cpal).

## How It Works

You control a player navigating a tile-based maze. Scattered throughout the maze are audio track icons, each emitting a different tone. Audio volume is determined by your distance to each track -- walk toward a track and it gets louder, walk away and it fades to silence.

The audio engine runs on its own thread using cpal, communicating with the game thread via lock-free ring buffers (rtrb). No unsafe code.

## Prerequisites

- Rust 1.93+ (edition 2024)
- [just](https://github.com/casey/just) (command runner)
- **Linux only:** `libasound2-dev` (ALSA headers for cpal)

## Quick Start

```sh
# Run the game
just run

# Run with debug logging
just run-debug
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
# Format, build, and test
just dev

# Run the full CI check suite locally (same as GitHub Actions)
just ci

# See all available commands
just
```

## Project Structure

```
crates/
  vvw-app/      # Binary entry point, window setup, camera
  vvw-game/     # Bevy plugin: maze, player, tiles, audio bridge
  vvw-audio/    # Audio engine: cpal output, looping samplers, ring buffer comms
```

| Crate | Description |
|-------|-------------|
| `vvw-app` | Application entry point -- window, camera, logging |
| `vvw-game` | Game plugin -- maze rendering, player movement, audio integration |
| `vvw-audio` | Audio engine -- fixed N-track topology, looping samplers, lock-free communication |

## Architecture

```
Game Thread (Bevy)                    Audio Thread (cpal callback)
─────────────────                     ──────────────────────────
update_track_gains:                   Process commands:
  for each track:                       SetTrackGain -> track.gain = gain
    distance = player <-> track         Start / Stop -> toggle playback
    gain = linear_falloff(distance)
    push SetTrackGain command         Mix tracks:
                                        for each track:
         ─── rtrb ring buffer ──>         sampler.generate(L, R)
                                          output += samples * gain
         <── rtrb ring buffer ───
poll_audio_events:
  log Started / Stopped / Error
```

## License

MIT

