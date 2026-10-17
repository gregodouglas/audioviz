# audioviz

A novel desktop music visualizer for Spotify listening. Windows first (RTX-class GPU), macOS second.

Aesthetic direction: spacey, line-based, post-modern / minimal generative art. Not a MilkDrop clone.

## Status

**Phase 4 landed (2026-10-17).** Rust + wgpu native app. Three scenes with cross-fades: a phosphor XY oscilloscope, chroma-steered flow-field ribbons (GPU compute), and a gravitationally lensed starfield with a Swiss-grid title overlay fed by Windows now-playing metadata (track, artist, album, art). Per-process Spotify loopback with auto-reconnect, full v1 analysis (bands, loudness, onsets, predictive beat clock, chroma), bloom + AgX + auto-exposure post, frame cap, monitor selection, persisted settings, optional auto scene director. Docs: `docs/phase1-spike.md` (build/run), `docs/phase2-analysis.md`, `docs/phase3-polish.md`, `docs/phase4-scenes.md`, `docs/phase5-ship.md` (next: installer, signing, winget); research in `docs/research/00-synthesis.md`.

```
cargo run --release -p audioviz
```
Tab panel · 1-3/Space scenes · A auto · T title overlay · F11 fullscreen · F5 reload shaders · Esc quit.

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
- `crates/metadata` — `MetadataSource` trait + Windows SMTC backend (now playing + art)
- `crates/app` — winit + wgpu app: `scene.rs`/`director.rs`/`post.rs`, `scenes/` (scope, ribbons, starfield), `overlay.rs`, egui panel, WGSL in `shaders/`
- `scripts/snapshot.ps1` — dev tool: run for N seconds and capture the window to PNG
