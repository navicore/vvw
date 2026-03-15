# Design: Sound Piping

## Intent

Let the player route audio from a distant track to a new location in the maze. The player enters pipe mode, starts at or near a track source, walks to a destination room, and stops. A visual pipe is drawn (straight line, ignoring walls) from source to destination. The destination end acts as a virtual speaker — a new audio emitter that participates in the same LOS/proximity/pan rules as any real track. The player can now hear the source track from the destination room, blending it with whatever tracks are already there. Multiple pipes can be placed in a session.

**Why.** The maze constrains which tracks can mix — LOS is the gatekeeper. Piping lets the player override that constraint and compose blends the album author never designed, turning the listener into a sound architect. The pipe is the in-game artifact left behind.

## Feasibility

**Rendering.** A pipe is a single straight-line `Mesh2d` (thin rectangle or two-triangle strip) between two world positions. One mesh per pipe, created once on placement. This is negligible — far cheaper than the arc meshes in sound visuals. No per-frame mesh updates needed after placement.

**Audio.** Web Audio's `MediaElementAudioSourceNode` can connect to multiple `GainNode` destinations simultaneously (`source.connect(gain1); source.connect(gain2)`). No need to duplicate `<audio>` elements or re-download audio. Each pipe speaker gets its own gain/panner chain forked from the source track's existing media element source. Cost: two extra Web Audio nodes per pipe (gain + panner). Browsers handle dozens of these without issue.

**Spatial audio.** `compute_spatial_targets` iterates entities with `TrackIcon + TilePos + TrackAudioState`. A pipe speaker entity with these components participates automatically — LOS, distance gain, and pan all work. However, `web_audio_sync` currently maps `TrackIcon.track_id` to a single `WebTrack` entry. Pipes need a separate audio identity: either a `PipeSpeaker` component with its own gain/pan chain in the engine, or a new `track_id` range for virtual emitters. The latter is simpler — `WebAudioEngine` already uses `HashMap<usize, WebTrack>`, so pipe speakers get IDs from `TrackIdCounter` and register their own `WebTrack` with a forked audio graph.

**Verdict: feasible.** No architectural changes needed in vvw-core or vvw-game spatial math. Changes are confined to a new `SoundPipePlugin` in vvw-game and a `fork_track` method on `WebAudioEngine` in vvw-web.

## Constraints

- **Don't modify existing tracks.** Piping forks the audio graph; the original track is unaffected.
- **LOS from speaker end only.** The pipe itself does not conduct sound along its length. The speaker end is a point emitter — you hear it only when you have LOS to it, same as any track.
- **Session-scoped.** Pipes exist only for the current browser session. No persistence yet (future work: serialize pipe definitions for progressive app state).
- **Movement is normal during pipe mode.** The player walks freely — the pipe extends as they move. No input suppression.
- **Per-album opt-in.** Controlled by a `sound_piping: bool` field in `AlbumMetadata` (same pattern as `sound_visuals`).
- **Pipe limit.** Cap at a reasonable number (e.g., 8) to avoid graph bloat.
- **Out of scope:** curved pipes, pipes that follow corridors, pipe removal UI, persistence, audio attenuation along pipe length.

## Approach

### Pipe mode (vvw-game, via interaction-modes framework)

`SoundPipePlugin` registers a `Pipe` mode with the mode registry. On `ModeActivated`:
- Record the player's current position as `pipe_start`.
- Identify the nearest track to `pipe_start` — this is the source track.
- Spawn a pipe preview entity (thin line mesh from start to player position, updated each frame).

On `ModeDeactivated` (or explicit stop):
- Freeze the pipe at the player's current position as `pipe_end`.
- Spawn a `PipeSpeaker` entity at `pipe_end` with `TrackIcon { track_id: new_id }`, `TilePos`, and `TrackAudioState`.
- Send a `PipePlaced { source_track_id, speaker_track_id, start, end }` event.
- vvw-web handles `PipePlaced` by calling `engine.fork_track(source_track_id, speaker_track_id)`.

### Pipe visual (vvw-game)

A `Mesh2d` rectangle stretched between start and end positions. Semi-transparent, tinted (e.g., blue-ish). Z-layer between floor and player. Spawned once on placement, no per-frame updates.

### Audio graph fork (vvw-web)

`WebAudioEngine::fork_track(source_id, new_id)`:
- Look up the existing `WebTrack` for `source_id`.
- Create a new `GainNode` and `Panner`, connected to `ctx.destination()`.
- The source's `MediaElementAudioSourceNode` is already connected to `source.gain_node`. Web Audio allows adding a second connection: the source audio element's media source connects to the new gain node too.
- Register a new `WebTrack` entry under `new_id` with independent gain/pan nodes but **no element ownership**. The forked entry must not call `pause()`, `play()`, `set_src()`, or `load()` on the shared `HtmlAudioElement` — only the source track's entry controls element lifecycle. `update_streaming` must skip entries that are forks (a `is_fork: bool` flag on `WebTrack`). This ensures distance-based lazy streaming decisions are made only by the source track owner, avoiding conflicting pause/play from divergent distance states.

### Pipe registry (vvw-game)

A `PipeRegistry` resource stores `Vec<PipeDescriptor>` (source track, speaker track, start/end positions). This is the serialization point for future persistence work.

## Domain Events

| Event | Producer | Consumer |
|---|---|---|
| `ModeActivated(Pipe)` | Mode system | `SoundPipePlugin` — begin pipe preview |
| `ModeDeactivated(Pipe)` | Mode system | `SoundPipePlugin` — finalize pipe |
| `PipePlaced { source_track_id, speaker_track_id, start, end }` | `SoundPipePlugin` | vvw-web — fork audio graph; pipe registry — record pipe |

Spatial audio needs no new events — `PipeSpeaker` entities have `TrackAudioState` and participate in `compute_spatial_targets` and `web_audio_sync` automatically.

## Checkpoints

- [ ] Enter pipe mode near a track, walk to another room, stop — pipe visual appears as a straight line
- [ ] Standing near the pipe speaker end — hear the source track mixed with local tracks
- [ ] Walking away from pipe speaker — gain fades by distance, same as any track
- [ ] Wall between player and pipe speaker — no sound (LOS blocked)
- [ ] Place two pipes from different sources to the same room — three-way mix
- [ ] Source track is unaffected — volume/pan at the original location unchanged
- [ ] Album with `sound_piping: false` — no pipe mode, no overhead
- [ ] Cap at 8 pipes — attempting a 9th shows a warning or is silently ignored
