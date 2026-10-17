# 00 — Research synthesis and proposed direction

> 2026-10-16. Lead-session synthesis of the four briefs in this folder. Read this first; go to the numbered briefs for sources and detail.

## The one-paragraph answer

What it takes to build this is: capture Spotify's audio through the OS (not through Spotify, which gives you neither raw audio nor, since Nov 2024, any analysis API), do your own real-time DSP to turn PCM into musical features, and render in a GPU-native stack you fully control. Every piece is well-trodden individually; nobody has assembled them into a trustworthy, designed, ambient desktop app, which is the gap. The recommended stack is Rust + wgpu, the recommended first scene is a phosphor-physics XY oscilloscope of the actual stereo signal, and the recommended ingest is per-process WASAPI loopback on `Spotify.exe` plus Windows media-session metadata, with the Spotify Connect (librespot) path kept optional.

## What the research established

**Audio (brief 01).** Spotify is a black box by design. The viable, low-risk path is OS loopback: Windows per-process loopback can isolate just Spotify's process tree (so Discord pings don't flash the screen), and macOS 14.2+ has Core Audio process taps that do the same. Track name, album art and play state come free from Windows SMTC with no API key. librespot would give sample-accurate PCM as a Spotify Connect device, but Spotify broke it again in August 2026 and its README calls itself "probably forbidden"; it is an optional personal-use mode at most. Loopback audio is captured *before* you hear it, so visuals naturally lead the ear by 10 to 40 ms wired and far more on Bluetooth. A user-calibrated delay is non-negotiable.

**Rendering (brief 02).** Rust + wgpu + winit (+ egui for a debug overlay) wins for a solo developer: one language, actively maintained (v30), DX12/Vulkan/Metal from one codebase, first-class compute shaders, and full control of the shell (frame caps, borderless multi-monitor, a possible screensaver mode, experimental HDR). The techniques this aesthetic needs are all native to it: compute particle systems, instanced-quad wide lines, HDR ping-pong feedback, physically based bloom, AgX tonemapping, blue-noise dither. Alternatives: WebGPU in Tauri for fastest iteration (loses shell control), Bevy (engine overhead with no payoff). The open-source wgpu visualizer field is tiny (fosfora, vibe, lactea, sonica, all under 150 stars), which confirms nothing exists to build on and nothing exists to compete with.

**Analysis (brief 03).** v1 is classical DSP and is enough to look good: multi-resolution STFT (short for onsets, long for bass and chroma), 24-32 log bands with per-band auto-gain so quiet and loud tracks look alike, asymmetric attack/release envelopes, MilkDrop-style fast and slow band averages as the minimum bar, spectral-flux onsets per band group, a PLL "beat clock" that *predicts* beats rather than reacting to them, chroma for a color wheel, mid/side width for spatial spread, and long-window heuristics for builds and drops. v2 adds a real causal beat tracker (BTrack, real-time PLP, BeatNet+) and small ML (CLAP mood embeddings via ONNX Runtime on the GPU) for slow scene and palette choice. Real-time stem separation is a v3 research item. Design rules that matter: map energy to velocity not position, separate slow from fast parameters, limit flash rate below 3/s, seed long-term variation so an hour doesn't get boring.

**Prior art and aesthetics (brief 04).** Everything on the market is a MilkDrop descendant of murky provenance, a pro VJ tool that needs operating, or wallpaper-store noise. The reference board that fits "spacey, lines, post-modern" runs from Ryoji Ikeda and Inconvergent through Robert Hodgin's Magnetosphere (the actual iTunes visualizer), Tyler Hobbs's flow fields, oscilloscope music, Gaia star maps, Interstellar's lensing, and Swiss typography. Ten concepts were sketched; the top three to prototype are the phosphor XY scope, chroma-steered flow-field ribbons, and a gravitationally lensed star field with a Swiss-grid metadata overlay.

## Proposed direction (pending Greg's decision, see ADR-0001)

**Product shape.** A signed, open-source, no-telemetry Windows app (macOS later) that runs as a borderless ambient window or screensaver, listens to Spotify via loopback, shows a small curated set of *designed scenes* rather than thousands of presets, and treats track metadata and cover art as typographic material. Scenes drift and cross-fade on slow musical cues; a few parameters always map directly to the music so the connection is never lost.

**Architecture.**

```
 Spotify.exe ──► WASAPI process loopback ──► ring buffer (+ calibration delay)
                                                  │
 SMTC (track, art, state) ───────────┐            ▼
                                     │     analysis thread: STFT×3, bands, AGC,
                                     │     envelopes, onsets, beat clock, chroma,
                                     │     mid/side, section heuristics
                                     │            │  (feature frame @ ~100 Hz)
                                     ▼            ▼
                              render thread: wgpu, scenes, HDR ping-pong,
                              compute particles, line renderer, bloom, AgX, dither
                                     │
                              egui overlay (debug, calibration, settings)
```

Clean trait boundaries: `AudioSource` (loopback now, librespot or file later), `MetadataSource` (SMTC now, AppleScript/Web API on Mac), `Scene` (each visual concept), `Features` (the frame handed from analysis to render).

**Phased plan.**

| Phase | Goal | Exit criterion |
|---|---|---|
| 0 Research | This folder | Greg picks direction (ADR-0001) |
| 1 Spike | Rust crate: wasapi process loopback on Spotify → FFT → wgpu window drawing a raw XY scope with phosphor decay | Music visibly drives the screen; loopback-to-ear latency measured |
| 2 Analysis core | Bands, AGC, envelopes, onsets, PLL beat clock, chroma, mid/side; egui debug meters | Feature frame is stable across 20 varied tracks (ambient to metal) |
| 3 Scene 1 polish | Phosphor XY scope with curated gain/rotation, bloom, AgX, dither; frame cap; borderless fullscreen | Greg would leave it running on a second monitor for an hour |
| 4 Scenes 2-3 | Flow-field ribbons; lensed star field + Swiss type overlay from SMTC metadata | Three scenes cross-fading on section cues |
| 5 Ship-ability | Signing, winget, no-telemetry statement, .scr wrapper, settings | Installable by someone who isn't Greg |
| 6 macOS | Core Audio tap + AppleScript metadata; Metal via wgpu | Parity on a Mac |

**Week-1 verification list** (things the briefs flagged as unconfirmed that affect the spike):
- Run the `wasapi-rs` `record_application` example against Spotify; confirm format, latency, and whether system volume affects it.
- Confirm SMTC thumbnail and timeline behaviour with Spotify specifically.
- Measure loopback-to-ear latency with a test tone and a phone camera.
- Confirm wgpu 30 builds and runs on driver 617.x with DX12 backend; check scRGB surface support as a side note.

## Open decisions for Greg

1. **Stack**: Rust + wgpu as proposed, or a WebGPU-in-Tauri prototype first for faster visual iteration (WGSL ports later)?
2. **First scene**: phosphor XY scope as proposed, or lead with flow-field ribbons (more "lines", less "scope")?
3. **Scope of v1**: Windows-only ambient window, or include the .scr screensaver mode from the start?
4. **License and name**: permissive OSS (MIT/Apache-2.0) is assumed given the trust argument; "audioviz" is a working title.
