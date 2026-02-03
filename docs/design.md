# VVW (Visual Virtual World) - Audio Exploration Game

## Vision

VVW is an audio exploration experience where users navigate virtual spaces to discover and interact with audio tracks. Unlike a DAW, there's no editing - just spatial exploration where proximity and visibility affect what you hear.

## Iteration 1: 2D Maze Explorer

### Concept
- Pacman-style 2D maze with top-down view
- Audio tracks represented as icons at various maze locations
- As user clears walls (gains line-of-sight), track volume fades in
- As user moves closer, volume increases further
- Multiple tracks play simultaneously, mixed based on avatar position

### Core Mechanics
1. **Visibility System**: Raycasting or flood-fill to determine which track icons are "visible" to the avatar
2. **Proximity Audio**: Distance from avatar to track icon maps to gain (0.0 to 1.0)
3. **Spatial Mix**: All visible tracks play simultaneously with distance-based volume

---

## Technology Stack (Building on VVDAW)

### Code to Copy from VVDAW

These files will be copied and adapted (not linked as dependencies):

| Source File | Target | What to Copy |
|-------------|--------|--------------|
| `vvdaw-core/src/lib.rs` | `vvw-audio/src/types.rs` | Sample, SampleRate, Frames type aliases |
| `vvdaw-comms/src/lib.rs` | `vvw-audio/src/comms.rs` | AudioCommand, AudioEvent enums, channel setup |
| `vvdaw-plugin/src/lib.rs` | `vvw-audio/src/plugin.rs` | Plugin trait, AudioBuffer, EventBuffer |
| `vvdaw-audio/src/engine.rs` | `vvw-audio/src/engine.rs` | AudioEngine with cpal integration |
| `vvdaw-audio/src/builtin/gain.rs` | `vvw-audio/src/gain.rs` | GainProcessor (atomic gain control) |
| `vvdaw-audio/src/builtin/mixer.rs` | `vvw-audio/src/mixer.rs` | MixerProcessor (N-input mixing) |
| `vvdaw-audio/src/builtin/sampler.rs` | `vvw-audio/src/sampler.rs` | SamplerProcessor (audio playback) |

**Simplifications for VVW:**
- Remove VST3/CLAP plugin hosting (not needed)
- Remove AudioGraph complexity (fixed N-track topology)
- Remove session save/load (not needed for v1)
- Simplify to stereo-only (no multichannel support needed)

### New Components for VVW

| Component | Purpose |
|-----------|---------|
| vvw-game | Bevy-based 2D maze game logic |
| vvw-spatial | Proximity/visibility calculations, distance-to-gain mapping |
| vvw-app | Main binary, CLI args, initialization |

### Dependencies

```toml
[workspace.dependencies]
# Core (from vvdaw)
bevy = "0.17"
cpal = "0.15"
hound = "3.5"
dasp = "0.11"
rtrb = "0.3"

# Game-specific
leafwing-input-manager = "0.17"  # Input handling
rand = "0.8"                      # Maze generation
```

---

## Architecture

### Audio Flow

```
┌─────────────────────────────────────────────────────────────┐
│                      Game Thread (Bevy)                      │
│                                                              │
│  Avatar Position ──→ Distance Calculator ──→ Gain Values    │
│         ↓                                         ↓          │
│  Visibility Check ────────────────────────→ Track Enable    │
│                                                              │
│  Commands: SetParameter(track_id, gain_value)               │
└─────────────────────────────────────────────────────────────┘
                              │
                    [Lock-free Ring Buffer]
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                      Audio Thread (cpal)                     │
│                                                              │
│  ┌─────────┐   ┌─────────┐   ┌─────────┐                    │
│  │Sampler 1│   │Sampler 2│   │Sampler 3│  ... (N tracks)    │
│  └────┬────┘   └────┬────┘   └────┬────┘                    │
│       ↓             ↓             ↓                          │
│  ┌─────────┐   ┌─────────┐   ┌─────────┐                    │
│  │ Gain 1  │   │ Gain 2  │   │ Gain 3  │  (controlled by UI)│
│  └────┬────┘   └────┬────┘   └────┬────┘                    │
│       └─────────────┼─────────────┘                         │
│                     ↓                                        │
│               ┌──────────┐                                   │
│               │  Mixer   │ → System Output                   │
│               └──────────┘                                   │
└─────────────────────────────────────────────────────────────┘
```

### Distance-to-Gain Mapping

```rust
// Options for attenuation curve:

// 1. Linear falloff (simplest)
fn linear_gain(distance: f32, max_distance: f32) -> f32 {
    (1.0 - distance / max_distance).max(0.0)
}

// 2. Inverse square (more realistic)
fn inverse_square_gain(distance: f32, reference_distance: f32) -> f32 {
    let d = distance.max(reference_distance);
    (reference_distance / d).powi(2).min(1.0)
}

// 3. Exponential rolloff (game-friendly)
fn exponential_gain(distance: f32, half_distance: f32) -> f32 {
    0.5_f32.powf(distance / half_distance).min(1.0)
}
```

### Visibility System

```
Two-phase volume control:
1. Visibility (binary): Can avatar see the track icon? (line-of-sight through walls)
   - Not visible: target_gain = 0.0 (muted)
   - Visible: proceed to distance calculation

2. Distance (continuous): How far is the avatar from the track?
   - Distance → target_gain curve mapping

3. Smooth Fade: Interpolate current_gain toward target_gain
   - Fade time: ~0.5 seconds
   - Prevents jarring audio pops when visibility changes
```

### Fade Implementation

```rust
// Per-track state
struct TrackAudioState {
    target_gain: f32,      // From visibility + distance
    current_gain: f32,     // Smoothed value sent to audio
    fade_speed: f32,       // Gain change per second (e.g., 2.0 for 0.5s fade)
}

// In update system (runs every frame)
fn update_track_gains(time: Res<Time>, mut tracks: Query<&mut TrackAudioState>) {
    for mut track in &mut tracks {
        let delta = track.target_gain - track.current_gain;
        let max_change = track.fade_speed * time.delta_secs();
        track.current_gain += delta.clamp(-max_change, max_change);
    }
}
```

---

## Game Design: Iteration 1

### Maze Structure
- Grid-based tilemap (e.g., 20x20 cells)
- Cell types: Wall, Path, TrackIcon, PlayerStart
- Maze can be:
  - Hand-designed (JSON/RON file)
  - Procedurally generated (recursive backtracker, Prim's, etc.)

### Track Icon Placement
- Place 3-8 track icons at dead-ends or interesting locations
- Each icon represents one audio file
- Icon shows waveform thumbnail or genre-based graphic

### Player Mechanics
- Arrow keys or WASD movement
- Grid-snapping movement (Pacman-style):
  - Player occupies exactly one tile at a time
  - Movement input queues next direction
  - Player moves one tile per input (or hold for continuous)
  - Cannot move into wall tiles
- No enemies in v1 - pure exploration
- Optional: smooth animation between tiles (visual only, logic is discrete)

### UI Elements
- Mini-map showing explored areas
- Track list showing discovered tracks (with distance indicator)
- Volume meters for currently audible tracks

---

## Workspace Structure

```
vvw/
├── Cargo.toml              # Workspace manifest
├── crates/
│   ├── vvw-core/           # Shared types (can re-export from vvdaw-core or copy)
│   ├── vvw-audio/          # Audio engine wrapper (wraps vvdaw-audio or copies relevant parts)
│   ├── vvw-spatial/        # Distance/visibility calculations
│   ├── vvw-game/           # Bevy game plugin (maze, player, tracks)
│   └── vvw-app/            # Main binary
├── assets/
│   ├── mazes/              # Maze definition files
│   ├── audio/              # Audio tracks to explore
│   └── sprites/            # Player, track icons, wall tiles
└── docs/
    └── design.md           # This design document
```

---

## Implementation Plan

### Phase 1: Foundation (Minimal Audio)
1. Set up workspace with vvw-app and vvw-game crates
2. Create basic Bevy app with 2D camera
3. Implement simple grid-based maze renderer
4. Add player entity with keyboard movement
5. Implement collision detection with walls

### Phase 2: Audio Integration
1. Add vvw-audio crate (port/wrap vvdaw-audio essentials)
2. Set up audio engine with cpal
3. Create SamplerProcessor for each track
4. Implement command channel for gain control
5. Test: single track plays, gain can be changed

### Phase 3: Spatial Audio
1. Add vvw-spatial crate
2. Implement distance calculation (avatar ↔ track icons)
3. Implement gain mapping function
4. Create Bevy system that sends gain updates to audio thread
5. Test: volume changes as player moves

### Phase 4: Visibility System
1. Implement raycasting or grid-based line-of-sight
2. Add visibility check before distance calculation
3. Implement smooth fade-in when track becomes visible
4. Test: tracks only audible when visible

### Phase 5: Polish
1. Add track icons/sprites
2. Add mini-map
3. Add track discovery UI
4. Implement maze loading from file
5. Add multiple maze levels

---

## Future Iterations

### Iteration 2: 3D Flying World
- Teleport from 2D maze to 3D space
- Tracks as floating objects in 3D
- Fly toward tracks (like vvdaw-ui-3d camera)
- Full 3D spatial audio (pan based on direction)

### Iteration 3: Networked Worlds
- Multiple players exploring same space
- Shared audio discovery
- Unbounded procedural world generation
- Social features (markers, favorites)

---

## Design Decisions

1. **Code Reuse**: Copy relevant code from vvdaw into vvw (clean slate, no repo coupling)
2. **Movement Style**: Grid-snapping (Pacman-style discrete tile movement)
3. **Visibility Fade**: Smooth fade-in (~0.5s) when track becomes visible
4. **Maze Generation**: Hand-designed for v1 (procedural can be added later)
5. **Track Loading**: Pre-load all tracks at startup (simpler for v1)

---

## Verification Plan

### Manual Testing
1. Run `cargo run` and verify maze renders
2. Move player with arrow keys, verify collision with walls
3. Place a track icon, verify audio plays
4. Move toward track, verify volume increases
5. Move behind a wall, verify volume drops to zero
6. Move around corner to see track, verify volume fades in

### Automated Testing
1. Unit tests for distance-to-gain mapping functions
2. Unit tests for visibility raycasting
3. Integration tests for audio command delivery

---

## Files to Create (Phase 1)

```
vvw/
├── Cargo.toml
├── crates/
│   ├── vvw-game/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs         # Plugin exports
│   │       ├── maze.rs        # Maze data structure and rendering
│   │       ├── player.rs      # Player movement and input
│   │       └── tiles.rs       # Tile types and sprites
│   └── vvw-app/
│       ├── Cargo.toml
│       └── src/
│           └── main.rs        # App entry point
└── assets/
    └── mazes/
        └── level1.ron         # First maze definition
```
