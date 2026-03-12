# Design: Maze Generation Improvements

## Intent

With 12 tracks the maze generator produces a world that's too compact. Rooms
cluster and tracks end up near each other with clear lines of sight between
them. The player can hear 4–5 tracks simultaneously. Tracks should feel like
isolated destinations — hearing more than 2 at once should be rare.

The problem is geometry. Audio uses LOS (line-of-sight) through the wall grid,
so what matters is not raw tile distance but **walls between tracks**. A track
20 tiles away down a straight corridor is fully audible. A track 8 tiles away
behind a wall is silent.

## Constraints

- **Don't touch spatial audio.** Gain curves, LOS, fade speeds all stay as-is.
- **Don't break existing albums.** `project.ron` stores the final `Maze` grid.
  Existing manifests are unaffected by generator changes.
- **Backwards-compatible config.** New `MazeGenConfig` fields must use
  `#[serde(default)]` so old configs still deserialize.
- **Out of scope:** hand-authored mazes, multi-level mazes, non-rectangular
  rooms, audio tuning.

## Analysis

Current defaults produce short corridors (4–8 tiles) and allow 20% room
overlap. With 12 tracks this means:
- Adjacent rooms share walls or connect via short straight corridors
- LOS reaches through corridors into neighboring rooms
- Multiple tracks visible from a single position

The key insight: **corridors are the LOS problem**. A straight corridor is a
sight line. Longer corridors make it worse, not better, unless they bend.

## Approach

### 1. Longer corridors with bends

Instead of straight corridors, generate L-shaped connections: horizontal
segment + turn + vertical segment. The bend blocks LOS between connected
rooms. This is the highest-impact change — even short L-corridors break
sight lines.

### 2. Scale corridor length with track count

More tracks → longer corridors to spread rooms further apart. Auto-scale
in `create_album` based on track count. But length alone won't help without
bends (see above).

### 3. Enforce minimum track separation

In `grow_maze`, reject room proposals where the center is too close to any
existing track. The threshold needs experimentation — start generous (maybe
20+ tiles) and tune down. This is a soft constraint (fallback to best
available after N attempts).

### 4. Lower overlap tolerance for larger albums

Reduce `max_overlap_fraction` to near zero for 8+ track albums. Rooms should
not share space.

### 5. Scale-aware config helper

```
fn config_for_track_count(n: usize) -> MazeGenConfig
```

Provide tuned defaults based on album size. Exact values TBD through
experimentation — this design doc intentionally avoids committing to
specific numbers since the interplay of LOS, corridor bends, and room
placement needs hands-on tuning.

### 6. (Optional) Dead-end corridors and extra walls

Add corridors that lead nowhere and extra wall segments inside large rooms.
These create confusion, break sight lines, and make the maze feel larger
without adding tracks.

## Checkpoints

- [ ] Generate a 12-track maze — verify by playing that at most 2 tracks
      are audible from any single position (subjective, LOS-dependent)
- [ ] Corridor bends block LOS between connected rooms
- [ ] Maze is still fully connected (existing flood-fill test passes)
- [ ] Small albums (1–4 tracks) don't produce absurdly large mazes
- [ ] `vvw-deploy create` still works with default and custom maze params
- [ ] Experiment with thresholds — exact values will come from playtesting
