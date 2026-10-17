# audioviz

A novel desktop music visualizer for Spotify listening. Windows first (RTX-class GPU), macOS second.

Aesthetic direction: spacey, line-based, post-modern / minimal generative art. Not a MilkDrop clone.

## Status

**Phase 3 landed (2026-10-17).** Rust + wgpu, Windows per-process loopback on Spotify with auto-reconnect, engine thread with the full v1 analysis (bands, loudness, onsets, predictive beat clock, chroma), phosphor XY oscilloscope with bloom, AgX tonemapping, auto-exposure, frame cap, monitor selection and persisted settings. See `docs/phase1-spike.md` to build and run, `docs/phase2-analysis.md` for the feature set, `docs/phase3-polish.md` for the render pipeline and ambient behavior; research in `docs/research/00-synthesis.md`.

```
cargo run --release -p audioviz
```
Tab panel · F11 fullscreen · F5 reload shaders · Esc quit.

## Layout

- `docs/research/` — research briefs
  - `00-synthesis.md` — read first: findings, proposed architecture, phased plan, open decisions
  - `01-audio-ingest.md` — getting audio and metadata out of Spotify (loopback, taps, librespot, SMTC, Web API, latency)
  - `02-rendering-stack.md` — stack comparison, GPU techniques, shell concerns, open-source peers
  - `03-audio-analysis.md` — real-time DSP, beat tracking, descriptors, libraries, mapping principles
  - `04-prior-art-and-aesthetics.md` — existing visualizers, the gap, reference board, concepts, trust/distribution
- `docs/decisions/` — architecture decision records, numbered
- `crates/audio` — `AudioSource` trait + WASAPI backend
- `crates/analysis` — `Analyzer` → `FeatureFrame`
- `crates/app` — winit + wgpu app, scenes, egui overlay, WGSL shaders in `shaders/`
- `scripts/snapshot.ps1` — dev tool: run for N seconds and capture the window to PNG
