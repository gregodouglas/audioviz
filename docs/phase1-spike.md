# Phase 1 spike — loopback → features → phosphor XY scope

> [!NOTE]
> A working log, kept as history: statuses and plans below are as they were written. What is open now is in [`ROADMAP.md`](ROADMAP.md); what the project is, in the [README](../README.md).

Status: **running 2026-10-16.** Per-process loopback attaches to Spotify, the analyzer meters are live, the phosphor scope renders at the monitor's 240 Hz, shader hot reload works. Latency not yet measured (see protocol below).

Backend: DX12 by default on Windows (the HDR path we want later); `WGPU_BACKEND=vulkan` also works. Debug builds open a console window beside the app for logs; release builds don't.

Dev tool: `powershell -ExecutionPolicy Bypass -File scripts/snapshot.ps1` launches the debug build for 8 s and saves `target/snapshot.png` plus the log. It captures via PrintWindow because plain screen grabs of a flip-model swapchain can come back black.

## What it does

- Captures Spotify's audio with Windows per-process WASAPI loopback (falls back to the default output endpoint if Spotify isn't running).
- Runs a first-cut analyzer (2048-point STFT, 32 log bands with per-band auto-gain, MilkDrop-style bass/mid/treb, RMS, centroid, stereo width).
- Draws the raw stereo signal as an XY oscilloscope with phosphor physics: additive beam segments whose brightness is inversely proportional to beam speed, an HDR persistence buffer that decays each frame, and a tonemap that bleaches hot spots toward white.
- egui overlay (Tab) with live sliders for gain, rotation, beam width/intensity, persistence, exposure, tint, and the audio delay used to line visuals up with what you hear.
- WGSL shaders in `crates/app/shaders/` hot-reload on save in debug builds (F5 forces it). A broken shader keeps the last good pipeline and shows the error in the panel.

## Prerequisites (Windows)

- Rust stable (MSVC toolchain). Installed.
- **Visual Studio Build Tools 2022 with "Desktop development with C++"** (gives `link.exe` and the Windows SDK). Without it every Rust build fails at link time with `linking with link.exe failed`. Install:
  ```
  winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --override "--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
  ```
- Note: Git for Windows puts `C:\Program Files\Git\usr\bin` early on PATH, which contains a coreutils `link.exe`. Once the VS tools are installed rustc locates the real linker through the VS registry and ignores PATH, so no config is needed. If a build still picks the wrong `link.exe`, run cargo from PowerShell or cmd rather than Git Bash.

## Run

```
cargo run --release -p audioviz
```

Keys: Tab panel · F11 borderless fullscreen · F5 reload shaders · Esc quit.

## Layout

```
crates/audio      AudioSource trait + WASAPI backend (process loopback, endpoint fallback)
crates/analysis   Analyzer -> FeatureFrame (sample-clocked, frame-rate independent)
crates/app        winit + wgpu app, Scope scene, egui overlay, shader hot reload
  shaders/        decay.wgsl, beam.wgsl, present.wgsl
```

## Latency measurement protocol (do once per output device)

Loopback is captured before the output buffer, so visuals lead the ear. The "audio delay" slider holds samples back to compensate.

**In-app (since Phase 5):** press **C**, tap **Space** on ten or so beats you hear, press **Enter**. The app matches each tap to the nearest onset the analyzer detected (within ±180 ms, after the current delay line) and applies the median residual to the delay. The spread (±) is the median absolute deviation of the taps; under ~15 ms is a good run. Repeat once to confirm it converges. Record the result in the table below.

**By camera (reference method):**

1. Play a track with a sharp, isolated click or kick (a metronome playlist works).
2. Film the screen and the speaker together with a phone in slow-motion (120 or 240 fps).
3. Count frames between the beam spike and the audible click in the recording; convert to ms.
4. Set the delay slider to that value. Wired headphones are typically 10 to 40 ms; Bluetooth 100 to 250 ms.
5. Record the number here:

| Output device | Measured lead (ms) | Date |
|---|---|---|
| | | |

## Exit criteria

- Music visibly drives the screen with Spotify as the isolated source.
- Loopback-to-ear latency measured and recorded above.
- Shader hot reload works.
