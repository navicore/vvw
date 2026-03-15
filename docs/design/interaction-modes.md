# Design: Interaction Modes

## Intent

Several planned features — breadcrumb recording, maze sculpting, sound piping — require the player to enter a temporary mode, perform actions that differ from normal movement, and leave behind an in-game artifact. Today there is no concept of "mode" — the player always moves. Each feature implementing its own activation UX would produce inconsistent controls and conflicting gestures.

This design introduces a shared interaction mode system: a registry that features plug into, a control surface for activation, and a protocol for entering/exiting modes with start/stop events.

**Why.** Breadcrumbs need record/play. Sculpting needs carve/place. Sound piping will need draw/anchor. All follow the same pattern: activate, do something spatially, deactivate. A shared framework avoids N independent control schemes competing for the same input space.

## Constraints

- **Normal play is unaffected.** When no mode is active, movement and audio work exactly as today. Zero cost when no modes are registered.
- **Must not break touch controls.** The mode control surface coexists with the existing D-pad overlay. Gestures must not conflict.
- **Per-album opt-in.** Modes only appear if the album's `project.ron` enables the corresponding feature (e.g., `breadcrumbs: true`, `sculpting: true`). Albums with no modes enabled see no UI change.
- **One mode active at a time.** Entering a mode exits any currently active mode.
- **Out of scope:** the specific behavior of each mode (breadcrumbs, sculpting, sound piping). Those are separate design docs. This covers only the shared activation/deactivation framework and control surface.

## Approach

### Mode registry (vvw-game)

A `ModeRegistry` resource holding a `Vec<ModeDescriptor>`:

```rust
struct ModeDescriptor {
    id: ModeId,           // enum variant or string key
    label: &'static str,  // shown on control surface
    icon: ModeIcon,       // enum: Record, Carve, Pipe, etc.
}
```

Feature plugins register their mode during app setup (e.g., `BreadcrumbPlugin` inserts a `ModeDescriptor` if `breadcrumbs` is enabled in album config). The registry is read-only after startup.

### Active mode state (vvw-game)

`ActiveMode` resource: `Option<ModeId>`. Systems for each feature use a `run_if` guard checking whether their mode is active.

### Control surface (vvw-game, rendered via Bevy UI)

- **Activation gesture:** long-press (mouse or touch) on a non-corridor area (wall tile) toggles the control surface visibility. Same gesture on both desktop and mobile.
- **Mode cycling:** a cycle button rotates through registered modes. The current mode's icon/label is displayed.
- **Start/stop:** a primary action button sends `ModeActivated(ModeId)` or `ModeDeactivated(ModeId)` events. Visual state (tinted, pulsing border) indicates when a mode is active.
- **Dismissal:** long-press on wall again, or pressing Escape, hides the control surface and deactivates any active mode.
- **Appearance:** semi-transparent tinted overlay buttons, visually similar to the existing D-pad but positioned separately (e.g., bottom-right). Hidden when no modes are registered.

### Mode lifecycle events (vvw-game)

Each feature plugin listens for `ModeActivated` / `ModeDeactivated` with its own `ModeId` and runs its enter/exit logic (e.g., breadcrumbs starts recording, sculpting enables draw gestures). The mode system does not know what each mode does — it only manages activation state and UI.

### Input suppression

When a mode is active, normal movement input may be suppressed or modified depending on the mode. Each mode's plugin decides this via its own systems — the framework provides `ActiveMode` as the coordination point but does not enforce suppression.

## Domain Events

| Event | Producer | Consumer |
|---|---|---|
| `ModeActivated(ModeId)` | Control surface button | Feature plugin (enter mode logic) |
| `ModeDeactivated(ModeId)` | Control surface button / Escape / mode switch | Feature plugin (exit mode logic, leave artifact) |
| `ControlSurfaceToggled(bool)` | Long-press gesture detector | UI visibility system |

Feature-specific events (e.g., `BreadcrumbRecordStart`, `TileChanged`) are produced by each feature plugin in response to `ModeActivated`/`ModeDeactivated` — not by the mode system itself.

## Checkpoints

- [ ] Register two mock modes — control surface shows cycle button with both labels
- [ ] Long-press wall tile — control surface appears; long-press again — it hides
- [ ] Cycle through modes — label/icon updates, no mode is activated yet
- [ ] Activate a mode — `ModeActivated` event fires, visual indicator shows active state
- [ ] Activate a different mode — previous mode receives `ModeDeactivated`, new one receives `ModeActivated`
- [ ] Dismiss control surface — active mode deactivated, normal play resumes
- [ ] Album with no modes enabled — no control surface, no long-press handler, zero overhead
- [ ] Touch device — control surface works alongside D-pad without gesture conflicts
