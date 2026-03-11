# Touch Screen Player Controls

## Intent

The player renders and runs on mobile browsers (Android Firefox, iPad Safari)
but there's no way to move — arrow keys don't exist on touchscreens. Add
touch-based movement so mobile users can actually play.

## Constraints

- Must not break keyboard input (WASD / arrow keys stay as-is).
- No native mobile app — this is WASM in a mobile browser.
- Must work with Bevy's `prevent_default_event_handling: true` on the canvas.
- No external asset dependencies (no joystick sprite sheets).
- Keep it in `vvw-game` (platform-independent) — not `vvw-web`.

**Out of scope:** Gamepad support, gesture-based camera control, pinch-to-zoom.

## Approach

We already use `leafwing-input-manager` 0.20. It supports `VirtualDPad` and
`VirtualJoystick` built from touch inputs — no custom touch event handling
needed.

### Option A: Virtual DPad (recommended to start)

Add a `VirtualDPad` mapping to the existing `PlayerAction` input map. Leafwing
translates touch regions into Up/Down/Left/Right actions. This reuses the
existing `handle_player_input` system with zero changes to movement logic.

### Option B: Virtual Joystick

A `DualAxislike` virtual joystick gives analog directional input. More natural
for maze exploration but requires switching from discrete actions to a
continuous axis in the movement system. Could be a follow-up.

### Visual overlay

Render a semi-transparent DPad or joystick zone on the canvas. Options:
1. **Bevy UI nodes** — camera-space overlay, works in WASM.
2. **HTML overlay** — DOM elements over the canvas. Simpler styling but
   may fight with Bevy's touch capture.
3. **No visual** — invisible touch zones. Bad UX but fastest to ship.

Recommend starting with Bevy UI nodes so everything stays in the game layer.

### Touch detection

Only show touch controls when touch is the primary input. Detect via
`Touches` resource having events, or via a startup check on the
`navigator.maxTouchPoints` JS API bridged into a Bevy resource.

## Domain Events

| Event | Producer | Consumer |
|-------|----------|----------|
| Touch start/move/end | Browser → Bevy `Touches` | leafwing `VirtualDPad` |
| `PlayerAction` (Up/Down/Left/Right) | leafwing input manager | `handle_player_input` (unchanged) |
| Touch detected flag | Startup system | UI system to show/hide DPad overlay |

No new Bevy events needed — leafwing abstracts touch into the existing
`PlayerAction` pipeline.

## Checkpoints

- [x] Touch D-pad zones move the player (uses Bevy `Interaction`, not `PlayerAction`)
- [x] Player moves on iPad and Android via touch — same physics as keyboard
- [x] Keyboard input still works on desktop (no regression)
- [x] D-pad overlay visible only on touch devices
- [x] Overlay doesn't block track-info foldout or album header
- [x] No new crate dependencies beyond what leafwing already provides
