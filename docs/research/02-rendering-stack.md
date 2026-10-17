# 02 — Rendering and application stack

> Research brief, 2026-10-16. Produced by a Sonnet research subagent; GitHub stats pulled from the GitHub API on 2026-10-16. Items marked **[unverified]** are background knowledge or unconfirmed search results.

## 1. Stack candidates

| Stack | Maturity (2026) | Win + Mac | Shader language | Compute | Shader hot-reload | Solo maintainability |
|---|---|---|---|---|---|---|
| **(a) Rust + wgpu + winit (+ egui)** | wgpu v30.0.1 (Aug 2026), ~18.3k stars, Apache-2.0, pushed daily ([releases](https://github.com/gfx-rs/wgpu/releases)). egui ~31.5k stars. | Yes. DX12/Vulkan on Windows, Metal on macOS, same code can target WebGPU. | WGSL; naga translates GLSL/SPIR-V. | First-class. | Build it yourself: file watch, naga validate, rebuild pipeline. ~50 lines. | **High.** Small API surface, no engine to fight. Cost: own frame graph and UI. |
| (b) Rust + Bevy | 0.18 (Jan 2026), ~49k stars. Breaking releases every few months. | Yes. | WGSL with preprocessor/imports. | Via render graph, lots of boilerplate. | Via asset system [unverified for compute]. | Medium-low. ECS and render-graph churn cost more than they give for a shader-first visualizer. |
| (c) C++ + Vulkan/DX12, or SDL3 GPU | SDL3 GPU is real; SDL_shadercross ~499 stars, pushed 2026-09, possibly still pre-stable. | SDL3 GPU covers Vulkan, D3D12, Metal. Raw Vulkan/DX12 = two backends. | HLSL or SPIR-V cross-compiled. | Yes. | Manual. | Low. More boilerplate and memory-safety risk, no gain for this aesthetic. |
| (d) Godot 4 | Mature. | Yes. | Godot shading language; GLSL for compute. | Through `RenderingDevice`, needs Forward+/Mobile renderer. | Good in editor. | Medium. Compute plumbing awkward; you inherit a whole engine. |
| (e) Unity / Unreal | Mature, heavyweight. | Yes. | HLSL. | Yes. | Yes. | Low fit: large installs, licensing, editor-centric for a shader project. |
| (f) WebGPU in Tauri 2 / Electron | WebGPU in Chrome/Edge 113+, Firefox 141+ (Win), Safari 26 on macOS 26 ([web.dev](https://web.dev/blog/webgpu-supported-major-browsers)). | Tauri uses WebView2 (Win) and WKWebView (Mac); WKWebView WebGPU depends on installed macOS version, with a reported regression on macOS 15 ([WebKit bug](https://bugs.webkit.org/show_bug.cgi?id=299510)). Electron ships Chromium, predictable. | WGSL. | Yes. | Trivial (Vite HMR). | Medium. Fastest iteration, but you lose frame pacing, HDR, borderless/multi-monitor control, screensaver mode. |
| (g) TouchDesigner / Notch / vvvv gamma | Mature commercial. | TD: Win+Mac. Notch Windows-focused [unverified]. vvvv Windows-only [unverified]. | GLSL / HLSL / nodes. | Yes. | Yes. | Rejected: binary project files poor for git and AI-assisted editing, licensing, no shippable standalone app, result is a patch not a product. |
| (h) openFrameworks / Cinder | oF ~10.4k stars, alive. Cinder ~5.5k, slow. | Yes. | GLSL (OpenGL). | OpenGL 4.3+ only, absent on macOS. | Manual. | OpenGL deprecated on macOS. Poor fit for compute-heavy 2026 approach. |

wgpu v30 added a `SurfaceColorSpace` API, exposing HDR output ([docs.rs](https://docs.rs/wgpu/latest/wgpu/enum.SurfaceColorSpace.html)).

## 2. GPU techniques for "spacey lines"

- **Compute particles, curl noise, flow fields.** Storage buffer of particle state, one compute dispatch per frame, draw as points or instanced line segments from the same buffer. A Three.js WebGPU curl-field demo runs ~245k instanced ribbons; millions of points are routine on a 2070-class GPU.
- **Strange attractors (Lorenz, Aizawa, Thomas, Clifford, de Jong).** Integrate N particles per frame in compute (RK4 or Euler), additive-blend into an HDR target, bloom. Each attractor is a small WGSL function, ideal for a solo project.
- **Wide lines without geometry shaders.** WebGPU/wgpu only draw 1-px lines. Standard fix: instanced quads, one instance per segment, vertex shader expands endpoints perpendicular to the segment in screen space using a viewport uniform ([Magnum LineGL](https://doc.magnum.graphics/magnum/classMagnum_1_1Shaders_1_1LineGL.html), [bevy_polyline](https://docs.rs/crate/bevy_polyline)). Round or feathered caps in the fragment shader for a soft-glow look.
- **SDF raymarching, volumetric nebula.** Inigo Quilez articles and Shadertoy. Raymarch at half or quarter resolution and upsample to protect thermals.
- **Feedback / ping-pong buffers.** Two HDR textures swapped each frame with slight zoom, rotation, decay and a flow-field warp. Keep decay under ~0.97 and work in linear HDR to avoid washout. This is MilkDrop's engine; use it as seasoning, not the dish.
- **HDR bloom and tonemapping.** Jimenez SIGGRAPH 2014 (CoD:AW) 13-tap downsample + tent upsample bloom ([iryoku](https://www.iryoku.com/?p=1707)). Tonemapping candidates: ACES, AgX (Troy Sobotka), Khronos PBR Neutral. AgX or a custom curve likely fits a minimal dark aesthetic better than ACES. Verify against primary sources.
- **Temporal accumulation / trails.** Same ping-pong buffer with exponential decay. Cheap, gives most of the "persistence" look.
- **Line integral convolution.** Fragment or compute pass smearing noise along a vector field. Use a low-res field for audio-driven turbulence.
- **Stippling, dithering, CRT grain post.** Blue-noise dither before 8-bit output (also kills banding in dark gradients), plus film grain. Cheap final-stage passes.
- **Oscilloscope XY with phosphor persistence.** Oversample the waveform, add brightness per XY sample into a float buffer, decay each frame ([windytan](https://www.windytan.com/2013/03/rendering-pcm-with-simulated-phosphor.html)). Gaussian beam cross-section in the fragment shader; intensity inversely proportional to beam speed (brighter where the beam lingers). Context: [CDM on oscilloscope music](https://cdm.link/watch-oscilloscope-draw-3d-make-amazing-music-time/).

## 3. Audio to GPU data path

Standard practice [no single source]:
- **Shadertoy convention**: 512x2 texture, row 0 FFT magnitude, row 1 raw waveform. Makes Shadertoy audio shaders near copy-paste.
- **Uniform buffer of band energies**: a few smoothed floats (bass, mid, high, beat phase, RMS, centroid) per frame. Best fit for line/particle parameters.
- **Storage buffer of history**: ring buffer of past frames for trails, spectrograms, waterfalls.
- **Smoothing on CPU**: FFT, windowing, log-band grouping, attack/decay envelopes on CPU; upload already-smoothed values. CPU smoothing is testable. GPU smoothing only when sampling the raw texture per pixel (then a lerp against last frame's texture).

## 4. Desktop shell concerns

- **HDR on Windows with wgpu.** wgpu 30 `SurfaceColorSpace::Auto` picks scRGB for `Rgba16Float` surfaces. Support table shows scRGB on DX12 and Vulkan, but one report says NVIDIA Vulkan on Windows does not advertise `EXTENDED_SRGB_LINEAR` ([blade issue](https://github.com/kvark/blade/issues/158)). Practical: use the DX12 backend on Windows for HDR, treat it as experimental until tested on the 2070 Super / driver 617.x. Design everything in linear HDR internally so output is a final-stage swap.
- **Screensaver (.scr).** A renamed .exe started with `/s` (run), `/p <hwnd>` (preview in parent window), `/c` (config). Rust precedent: [rust-pipes](https://github.com/FaceFTW/rust-pipes). Hard parts: parenting a wgpu surface to a foreign HWND for `/p`, exiting on input. Stub `/p` to a blank window at first. [unverified]
- **Borderless fullscreen, multi-monitor.** winit supports borderless fullscreen per monitor; one window per monitor for different views.
- **VSync, G-Sync, frame pacing.** `PresentMode::Fifo` for VSync. Cap to 30 or 60 fps in ambient mode.
- **Thermals for hours-long use.** Frame cap, reduced-resolution expensive passes (raymarch, bloom), a quality slider. A 2070 Super at full load for hours is loud and wasteful for an ambient piece.
- **macOS Metal via wgpu.** Native mapping. No OpenGL; HDR is EDR, handled differently from scRGB; winit needs the main thread. [HDR specifics unverified]

## 5. Open-source projects to learn from (GitHub API, 2026-10-16)

| Repo | Stars | Last push | License | Notes |
|---|---|---|---|---|
| [kevinraymond/fosfora](https://github.com/kevinraymond/fosfora) | 57 | 2026-10-17 | Apache-2.0 | Audio visualizer and VJ engine with WGSL audio-reactive shaders. Most active and closest to our goals. |
| [TornaxO7/vibe](https://github.com/TornaxO7/vibe) | 110 | 2026-09-07 | AGPL-3.0 | Wayland wallpaper visualizer, WGSL and GLSL via wgpu. Study the plumbing; AGPL, don't copy. |
| [M0Rf30/lactea](https://github.com/M0Rf30/lactea) | 7 | 2026-10-16 | Apache-2.0 | Pure-Rust MilkDrop 2/3 compatible on wgpu. Reference for feedback architecture, though it is what we are not building. |
| [rath/sonica](https://github.com/rath/sonica) | 3 | 2026-09-21 | MIT | Offline FFT + WGSL templates + wgpu to MP4. Useful audio-to-uniform plumbing. |
| [luke1188/iris](https://github.com/luke1188/iris) | 0 | 2026-09-18 | none | Windows realtime visualizer, Rust + wgpu + egui. Read only. |
| [ginger-code/cubensis](https://github.com/ginger-code/cubensis) | 0 | 2022-01 | MIT | Stale. Spectrum/waveform textures, egui, cpal, WASAPI loopback. Reference. |

All small. No large mature wgpu generative visualizer exists to build on, which supports building our own.

## Recommendation

**Rust + wgpu + winit + egui (debug/settings overlay), `wasapi` crate for Windows capture, `realfft` for analysis.**

- One language, one the developer knows, with compile-time checking that helps when AI generates code.
- wgpu is actively maintained (v30, release roughly every two months) and covers DX12/Vulkan/Metal from one codebase.
- Compute + instanced-quad lines + HDR ping-pong is exactly this aesthetic, and wgpu exposes all of it directly.
- WGSL is small enough for AI assistance and matches the shaders in fosfora, vibe, sonica.
- Own the shell: frame cap, borderless per monitor, possible .scr mode, HDR.

**Suggested build order**: audio capture, then a window with a feedback buffer, then the line renderer and a compute particle system, then bloom and tonemapping, then a Windows .scr wrapper, then optional HDR via DX12.

**Alternatives and why they lost**
1. **WebGPU in Tauri 2 / Electron.** Fastest iteration and the same WGSL, but gives up HDR, frame pacing, multi-monitor and screensaver control, and WKWebView WebGPU is version-dependent on macOS. Reasonable for a web-first prototype; WGSL ports straight to wgpu later.
2. **Bevy.** Same wgpu underneath, but ECS and render-graph churn add overhead without helping a shader-first visualizer. Only if this became a 3D scene with entities, which it is not.

Honorable mention: SDL3 GPU in C++ pays memory-safety and build cost with no gain; Godot is a fine prototype tool but not worth the engine around it.

## Facts to verify before committing

- scRGB HDR output on driver 617.x with the DX12 backend; whether Vulkan advertises it.
- wgpu 30 changelog for the exact `SurfaceColorSpace` support matrix.
- Whether the Jimenez CoD:AW slides describe the bloom commonly attributed to them.
- AgX vs ACES vs Khronos PBR Neutral from primary sources.
- `/p` preview-window handling for a wgpu surface in a .scr.
