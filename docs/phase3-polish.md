# Phase 3 — scene 1 polish and ambient behavior

> [!NOTE]
> A working log, kept as history: statuses and plans below are as they were written. What is open now is in [`ROADMAP.md`](ROADMAP.md); what the project is, in the [README](../README.md).

Status: **landed 2026-10-17.** Goal: a build Greg would leave running on a second monitor for an hour.

## Rendering

Pipeline per frame (`crates/app/src/scope.rs`, shaders in `crates/app/shaders/`, prelude `params.wgsl` prepended to all):

1. **decay** — HDR ping-pong, `new = old * persistence^dt` minus a tiny floor so trails reach black.
2. **beam** — instanced additive segments, Gaussian cross-section, energy ∝ 1/length (a lingering beam is bright, a fast one dim). Base energy is tuned so a diffuse stereo cloud sits near middle gray and a mono line clips to white, as on a real tube.
3. **bloom down** — dual-filter (Kawase) 5-tap chain, six half-res levels.
4. **bloom up** — 3×3 tent upsample accumulated additively back to half res; normalized by level count at composite time so the slider is a fraction of the base image.
5. **auto exposure** — a 1×1 ping-pong state texture tracks the mean of the smallest bloom level (16-sample grid) and slews toward a dark-field target over ~2 s, clamped to 0.25..6×. Measured pre-tonemap, so the beat pulse on exposure does not fight it. Toggle in the panel.
6. **present** — tint × intensity, **AgX** tonemap (Blender's minimal fit: log2 encode over -12.5..+4 EV, sigmoid, inverse matrix, 2.2 EOTF to linear), vignette, animated film grain, interleaved-gradient-noise dither (kills banding in the dark gradients), then the sRGB swapchain encodes.

egui draws to a non-sRGB view of the swapchain (`view_formats`), so panel colors are correct and the earlier warning is gone.

## Ambient behavior

- **Frame cap** (default 60, 0 = VSync only): the event loop uses `ControlFlow::WaitUntil`, so the GPU and CPU idle between frames instead of spinning at the monitor's 240 Hz. The phosphor decay and beam energy are time-based, so the picture is the same at any cap.
- **Monitor selection** and **start fullscreen**: Display section lists monitors with size and refresh rate; F11 or the button toggles borderless fullscreen on the chosen one. Changing the monitor while fullscreen moves the window.
- **Settings persistence**: `%APPDATA%\audioviz\settings.json` (via `dirs::config_dir`), written when anything changes (checked once a second) and on exit. Missing fields fall back to defaults, so old files survive new settings.
- **Capture reconnect**: the WASAPI thread checks every 2 s whether the target process died or changed PID (reopens, falling back to the endpoint if it is gone) or appeared while on endpoint fallback (switches to per-process). Failed opens retry every 3 s. Verified by targeting `charmap.exe`: endpoint fallback at start, switched to per-process 4 s after it launched, back to endpoint 2 s after it was killed.

## Defaults (fresh install)

gain 0.85 + auto, rotation 45° (mono = vertical), beam 1.2 px, intensity 1.0, persistence 0.05/s, exposure 1.0 + auto, bloom 0.3, grain 0.15, beat pulse 0.3, fps cap 60, P1 green tint.

## Not done / later

- HDR (scRGB) swapchain output: wgpu 30 exposes it; untested on this driver. Everything is linear HDR internally, so it is a present-stage change.
- Loopback-to-ear latency still unmeasured (phone camera protocol in `docs/phase1-spike.md`).
- Tray icon / minimize-to-tray, remembering window position, multiple windows (one per monitor).
- Scene-level long-term variation (the research's "an hour shouldn't get monotonous") belongs with Phase 4's scene system.
