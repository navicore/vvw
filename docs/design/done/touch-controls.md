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

Leafwing `VirtualDPad` was initially considered but its `DualAxislike` type
doesn't map to discrete `Buttonlike` actions without refactoring the input
system. Instead, a separate `TouchControlsPlugin` in `vvw-game/src/touch.rs`
uses Bevy's built-in `Interaction` component for reliable touch/click hit
testing.

### Implementation

- **D-pad overlay**: Four Bevy UI `Node` buttons (Up/Down/Left/Right)
  positioned absolutely in the bottom-left corner. Semi-transparent with
  pressed-state visual feedback via `BackgroundColor`/`BorderColor`.
- **Touch detection**: `detect_touch_device` watches `Touches` resource.
  The overlay starts hidden and becomes visible on first touch event.
- **Input handling**: `handle_touch_input` reads `Interaction::Pressed`
  states on D-pad buttons, accumulates directions (supporting multi-touch
  diagonals), normalizes, and applies velocity — same `velocity.0 +=`
  pattern as keyboard input. Gated on `touch_detected` to prevent
  double-velocity on desktop.
- **System ordering**: `handle_touch_input.after(handle_player_input)`
  to avoid Bevy's ambiguity detector on the shared `LinearVelocity` access.
  Internal chain: `detect_touch_device → handle_touch_input → update_dpad_visuals`.
- **Phone clearance**: Bottom margin (80px) clears phone browser
  navigation/gesture bars. Left margin (20px) for thumb reach.

## Domain Events

| Event | Producer | Consumer |
|-------|----------|----------|
| Touch start/move/end | Browser → Bevy `Touches` | `detect_touch_device` (show overlay) |
| `Interaction::Pressed` | Bevy UI hit testing | `handle_touch_input` (apply velocity) |
| Touch detected flag | `detect_touch_device` | `handle_touch_input` (gate) |

No leafwing integration — touch input is a parallel path to keyboard input,
both writing to `LinearVelocity` via the same physics model.

## Checkpoints

- [x] Touch D-pad zones move the player (uses Bevy `Interaction`, not `PlayerAction`)
- [x] Player moves on iPad and Android via touch — same physics as keyboard
- [x] Keyboard input still works on desktop (no regression)
- [x] D-pad overlay visible only on touch devices
- [x] Overlay doesn't block track-info foldout or album header
- [x] No new crate dependencies beyond what leafwing already provides
