# 3D Toggle Feedback — Visual Indicator During View Switch

## Intent

The first time the player toggles to 3D (via `V` or three-finger tap), there's a ~2 second delay while the GPU compiles shaders and uploads mesh/texture data. During this time nothing visible happens — the player doesn't know their gesture was recognized. They may tap again (triggering a double-toggle back to 2D) or assume the feature is broken.

**Why.** The delay only happens on the first toggle (GPU resources are cached after that). But first impressions matter. A brief visual acknowledgment bridges the gap.

## Constraints

- **No new HTML/JS.** Pure Bevy UI — same rendering path as the control surface.
- **No loading screen.** The 2D view stays live during the switch. Audio continues. The hint is lightweight.
- **Disappears automatically.** The indicator clears itself once the 3D view is active — no manual dismissal.
- **Works in both directions.** Show feedback on 3D→2D toggle too (though that switch is typically faster).
- **Out of scope:** Progress bars, percentage indicators, animated spinners. Just a text hint.

## Approach

When `toggle_3d_view` fires:

1. **Immediately** spawn a small UI text node ("Switching to 3D..." or "Switching to 2D...") at the bottom center of the screen, styled like the mode label (subtle, semi-transparent).
2. Tag it with a `MorphFeedback` marker component.
3. On **every frame**, check if the camera swap has taken effect (i.e., the `Camera3d` rendered at least one frame). The simplest proxy: despawn the feedback node one frame after `Morph3dActive` changes — the toggle system already set the new state, and by next frame the GPU work is done.

This is ~15 lines: spawn text on toggle, despawn next frame. The text appears instantly (Bevy UI renders before the 3D pipeline stalls), giving the player confirmation that their gesture landed.

## Domain Events

| Event | Producer | Consumer |
|-------|----------|----------|
| `Morph3dActive` changes | `toggle_3d_view` | Feedback system: spawn text on change, despawn next frame |

No new events — reads the existing `Morph3dActive` resource.

## Checkpoints

- [ ] Text appears immediately on `V` press / three-finger tap
- [ ] Text disappears once the new view renders
- [ ] No text flicker on subsequent (cached) toggles — it appears and vanishes within 1-2 frames
- [ ] Text does not interfere with control surface or other UI
