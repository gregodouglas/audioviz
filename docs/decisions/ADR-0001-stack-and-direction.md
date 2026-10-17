# ADR-0001 — Stack, ingest path and first scene

- **Status:** Accepted 2026-10-16 (all items confirmed by Greg).
- **Context:** Phase 0 research (`docs/research/00-synthesis.md`) is complete. The project needs a rendering stack, an audio ingest path and a first scene to start a spike. Targets: Windows 11 + RTX 2070 Super first, then macOS on a MacBook Pro M4 Max.

## Decisions

### 1. Rendering stack and iteration workflow — accepted

**Rust + wgpu + winit as the single production codebase**, egui for a debug/settings overlay, WGSL shaders. Linear HDR internally, AgX tonemap and blue-noise dither at output.

Fast visual iteration is handled by an **engine-first hybrid** rather than a separate prototype stack:

- The Rust engine (capture + analysis + feature frame) is written once, from the phase 1 spike onward. It broadcasts feature frames over a localhost WebSocket (also a debugging tool).
- WGSL hot reload (file watch, revalidate, rebuild pipeline) and egui sliders on every tunable parameter are built into the Rust app in phase 1, so shader and parameter edits need no recompile.
- An optional `sketchbook/` (Vite + TypeScript + WebGPU) subscribes to the engine's WebSocket and iterates new scenes in the browser with hot module reload. It shares one `features.wgsl` struct file with Rust, so finished shaders port with zero edits; only the thin TypeScript host is discarded.
- The sketchbook is **developer tooling only**, never a product dependency: the shipped app is a native wgpu window with no browser or web view. Default is to iterate inside the Rust app (hot reload + egui); the sketchbook is stood up only if that proves too slow when scene 2 (flow-field ribbons) begins. Confirmed with Greg 2026-10-16 that the product must be its own application window.
- Drift guard: the WGSL feature struct is the single source of truth, with a Rust byte-layout test.

### 2. First scene — accepted

Phosphor-physics XY oscilloscope of the real stereo signal.

### 3. v1 scope — accepted

Windows ambient window(s) only: borderless, per-monitor. Screensaver (.scr) mode deferred, possibly never.

### 4. License and name — accepted

MIT. "audioviz" remains the working title.

### 5. Audio ingest and analysis — accepted as proposed

Windows per-process WASAPI loopback on `Spotify.exe` via the `wasapi` crate, falling back to endpoint loopback. Metadata and album art via Windows SMTC. Both behind `AudioSource` / `MetadataSource` traits. Classical DSP v1 per `docs/research/03-audio-analysis.md`; beat trackers and ML deferred to v2. User-calibrated audio delay from day one. librespot optional, later, personal use only.

### 6. Portability principles — accepted (implied by the macOS target)

- All platform-specific code (WASAPI, SMTC, later Core Audio taps and AppleScript) lives in a `platform` module behind the two traits. No Windows-only crates in the core.
- CI builds on a macOS runner from early on so the core keeps compiling against Metal before the macOS audio backend exists.
- The feature-frame protocol and all WGSL are platform-independent by construction.

## Alternatives considered

- **WebGPU in Tauri 2 / Electron as the product.** Fastest shader iteration, same WGSL. Rejected as the production shell because it gives up frame pacing, HDR, multi-monitor control, and WKWebView WebGPU is macOS-version dependent. Its iteration advantage is captured instead by the optional browser sketchbook fed by the Rust engine.
- **Bevy.** Same wgpu underneath; ECS and render-graph churn add cost without benefit for a shader-first visualizer.
- **librespot as the primary audio path.** Sample-accurate and metadata-rich, but ToS-grey, no 2026 release, broken by Spotify in Aug 2026.
- **cpal for capture.** Does not document Windows loopback as of 2026-10-16; `wasapi` crate has a working per-process example.
- **TouchDesigner / Notch / Synesthesia.** Patches, not products; binary project files; licensing.

## Consequences

- Solo-maintainable single-language production codebase; the cost is writing our own frame graph, line renderer and UI plumbing, offset by hot reload and egui.
- Two hosts (Rust, optional TypeScript sketchbook) consume one feature contract; the contract must be versioned and tested.
- Windows-first is baked into the ingest layer; macOS requires a Core Audio tap backend (macOS 14.2+) and a different metadata source, slotting in behind existing traits.
- Visual sync quality depends on the calibration step, since loopback cannot know output latency.
