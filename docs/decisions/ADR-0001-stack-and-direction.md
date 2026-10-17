# ADR-0001 — Stack, ingest path and first scene

- **Status:** Proposed (2026-10-16). Awaiting Greg's decision.
- **Context:** Phase 0 research (`docs/research/00-synthesis.md`) is complete. The project needs a rendering stack, an audio ingest path and a first scene to start a spike.

## Decision (proposed)

1. **Rendering stack:** Rust + wgpu + winit, egui for a debug/settings overlay. WGSL shaders with file-watch hot reload. Linear HDR internally, AgX tonemap and blue-noise dither at output.
2. **Audio ingest:** Windows per-process WASAPI loopback on `Spotify.exe` via the `wasapi` crate, falling back to endpoint loopback. Track metadata and album art via Windows SMTC. Both behind `AudioSource` / `MetadataSource` traits so librespot (Spotify Connect) and macOS backends can be added later. User-calibrated audio delay from day one.
3. **Analysis:** classical DSP v1 as specified in `docs/research/03-audio-analysis.md` (multi-resolution STFT, log bands, per-band AGC, envelopes, onsets, PLL beat clock, chroma, mid/side). Beat trackers and ML deferred to v2.
4. **First scene:** phosphor-physics XY oscilloscope of the real stereo signal.
5. **Product posture:** open source (MIT or Apache-2.0), no telemetry, signed builds, curated scenes rather than user presets.

## Alternatives considered

- **WebGPU in Tauri 2 / Electron.** Faster shader iteration, same WGSL. Rejected for v1 because it gives up frame pacing, HDR, multi-monitor and screensaver control, and WKWebView WebGPU is macOS-version dependent. Remains a valid prototype path if visual iteration speed turns out to matter more.
- **Bevy.** Same wgpu underneath; ECS and render-graph churn add cost without benefit for a shader-first visualizer.
- **librespot as the primary audio path.** Sample-accurate and metadata-rich, but ToS-grey, no 2026 release, and broken by Spotify in Aug 2026. Optional later.
- **cpal for capture.** Does not document Windows loopback as of 2026-10-16; `wasapi` crate has a working per-process example.
- **TouchDesigner / Notch / Synesthesia.** Patches, not products; binary project files; licensing.

## Consequences

- Solo-maintainable single-language codebase; the cost is writing our own frame graph, line renderer and UI plumbing.
- Windows-first is baked into the ingest layer; macOS requires a Core Audio tap backend (macOS 14.2+) and a different metadata source.
- Visual sync quality depends on the calibration step, since loopback cannot know output latency.
