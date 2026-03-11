# Client Observability

## Intent

Detect and understand bad user experiences — especially browser-specific
audio/rendering failures — without requiring users to file bug reports.

## Problems to Solve

1. **Silent failures** — Audio can fail to play (wrong MIME type, CORS,
   autoplay policy) with no visible error.
2. **Browser matrix** — Safari, Firefox, Chrome each have quirks. We need
   to know which browsers fail and why.
3. **Performance** — WASM load time, maze render time, audio buffering
   latency vary by device. No visibility today.

## What to Capture

- **Browser & platform** — UA string or structured UA-CH data
- **Audio lifecycle** — AudioContext state, play() success/failure,
  format errors, time-to-first-audio
- **Rendering milestones** — WASM load, maze render, first frame
- **Errors** — JS exceptions, WASM panics, network failures
- **Session summary** — did the user get sound? how long did they stay?

## Design Constraints

- **Privacy-first** — No PII. No cookies or persistent identifiers.
- **Lightweight** — Fire-and-forget beacons, not blocking requests.
- **Self-hosted** — Own the telemetry stack. No third-party analytics
  services or web metrics platforms.
- **OpenTelemetry** — Use OTLP as the wire format. Collect into a
  self-hosted backend (e.g. Grafana/Loki/Tempo, Jaeger, ClickHouse).
- **Graceful degradation** — If telemetry fails, the player is unaffected.

## Open Questions

- OTLP endpoint: Cloudflare Worker proxy to self-hosted collector, or
  direct from browser to a public OTLP endpoint?
- What's the right granularity? Per-session summary vs per-event spans?
- How to handle ad blockers that strip beacon requests?
- Hosting: where does the collector run? (home lab, cheap VPS, etc.)

## Status

Design phase — not yet scheduled for implementation.
