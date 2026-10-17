# audioviz

A novel desktop music visualizer for Spotify listening. Windows first (RTX-class GPU), macOS second.

Aesthetic direction: spacey, line-based, post-modern / minimal generative art. Not a MilkDrop clone.

## Status

**Phase 0, research complete (2026-10-16).** Awaiting direction decision in `docs/decisions/ADR-0001-stack-and-direction.md`.

Start with `docs/research/00-synthesis.md`.

## Layout

- `docs/research/` — research briefs
  - `00-synthesis.md` — read first: findings, proposed architecture, phased plan, open decisions
  - `01-audio-ingest.md` — getting audio and metadata out of Spotify (loopback, taps, librespot, SMTC, Web API, latency)
  - `02-rendering-stack.md` — stack comparison, GPU techniques, shell concerns, open-source peers
  - `03-audio-analysis.md` — real-time DSP, beat tracking, descriptors, libraries, mapping principles
  - `04-prior-art-and-aesthetics.md` — existing visualizers, the gap, reference board, concepts, trust/distribution
- `docs/decisions/` — architecture decision records, numbered
