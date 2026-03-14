# Canvas Focus Indicator

## Intent

After the start overlay is dismissed, the player must click/tap the canvas to give it keyboard focus before arrow keys work. There is no visual indication of whether the canvas has focus, so users don't know why movement keys aren't responding — especially after interacting with the album/track panels or browser chrome.

The goal is to provide a clear but unobtrusive visual cue showing when the canvas is focused (ready for input) vs unfocused (needs a click).

## Constraints

- Must not interfere with the game rendering or spatial audio
- Must not block clicks — the unfocused state should still be clickable to regain focus
- Must work on both desktop (keyboard focus) and mobile (touch focus)
- Must not add layout shift — purely visual overlay
- The start overlay already handles the initial activation; this is about *re-acquiring* focus after it's lost

## Approach

**CSS `:focus` / `:focus-visible` border glow** — the most idiomatic web pattern for interactive elements:

- When the canvas has focus: no overlay, clean view of the game
- When the canvas loses focus: show a subtle semi-transparent dark veil over the canvas with a small "click to play" or arrow-key icon hint, similar to how YouTube dims an unfocused embed

This is a two-layer approach:
1. **CSS `outline` or `box-shadow` on `canvas:focus`** — a soft colored glow (e.g., `box-shadow: 0 0 0 2px rgba(100,130,200,0.6)`) signals "I have focus." Disappears when focus is lost.
2. **Dim overlay when unfocused** — a `#canvas-container::after` pseudo-element that shows a semi-transparent dark layer with a hint icon/text when the canvas does *not* have focus. Use `:focus-within` on `#canvas-container` to hide it when the canvas is focused.

The dim overlay is stronger feedback than a border alone, and the combination covers both "this is interactive" (border) and "this needs your attention" (dim + hint).

**Implementation is pure CSS + HTML** — no Rust or JS changes needed. The canvas already gets `focus()` called on overlay click, and browsers handle focus/blur natively.

## Domain Events

- **focus / blur** on the canvas element (native browser events, already firing)
- No new custom events needed
- No Bevy system changes — focus is a DOM concern, not a game concern

## Checkpoints

1. Click the start overlay — canvas is focused, no dim, game plays normally
2. Click the album title (opens panel) — canvas loses focus, dim overlay appears with hint
3. Close album panel, click canvas — dim disappears, arrow keys work immediately
4. Tab away from browser and back — focus state is visually correct
5. On mobile: tap track info panel, then tap canvas — same focus/unfocus cycle
