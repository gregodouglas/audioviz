# Phase 6 — UI refresh ("look exceptional")

Status: **built 2026-10-17, local only (not pushed), awaiting Greg's hands-on check.** Target version 0.2.0. Tracked as issues #1-#5.

## Progress log

**2026-10-17, same day as the brief.** Mockups signed off ("100% the right direction"), then built:

- `theme.rs` tokens + egui style; `widgets.rs` shared drawing (mark, slider rows, toggles, rail icons, glyphs).
- `chrome.rs` bar + `hittest.rs` (Windows `WM_NCHITTEST` subclass: caption drag, edge resize, Snap Layouts via `HTMAXBUTTON`, native maximize toggle from the non-client click). Window is `with_decorations(false)` + undecorated shadow + rounded corners. Auto-hide after 3 s; pinned while any sheet or the panel is open and for the first 5 s.
- `panel.rs` docked sheet with rail; sections as in the second-pass spec, including the monitor diagram, reduce flashing, looks, manual update check, copy diagnostics.
- `strip.rs` transport with SMTC control (metadata crate: `MediaCommand`), tint-from-art (metadata crate: `ArtImage::dominant_color`).
- `overlays.rs` toast, empty state, calibration sheet, shortcuts (`?`), first-run sheet (monitor, calibrate, scene).
- `looks.rs` Night / Party / Studio + user JSON files; `L` cycles.
- `tray.rs` (tray-icon crate): show, scene, auto, fullscreen on monitor N, quit; `--tray` starts hidden.
- Post stack: reduce-flashing limiter in `expo.wgsl`/`present.wgsl` (`flags` bit 1, `flash_rise`); on by default.
- `platform/`: open_url, launch_spotify, spotify_installed, on_battery, WinHTTP `latest_release` (manual only), is_newer, diagnostics_header.
- `--set key=value`, `AUDIOVIZ_PANEL=<section>` and `AUDIOVIZ_CHROME=1` dev overrides for screenshots.
- Installer: modern Inno wizard with image strip, checks page, finish-page options (see `installer/`).

Later the same day, from Greg's first hands-on: **H** hides everything (bar button and panel link too), Esc steps outward (sheet → panel → fullscreen → quit), cursor hides in fullscreen; scene switches are snappy (manual fade 0.5 s, retarget during a fade, pips highlight the target at once; the auto director keeps 4 s); **"Show what's playing" (N) is off by default** and gates the title overlay, toast and the strip's track identity; **Inter + JetBrains Mono are bundled** (OFL, `assets/fonts/`) instead of system fonts; scene controls restyled as panel rows. Installer stays Inno for this release (Greg: "that's fine"); the bespoke installer remains issue #3.

Also from Greg: **Layers** (Scene section switch): every enabled scene renders and is added to the composite with its own opacity (0-2), each keeping its own controls; 1-3 picks the layer being edited; dots on the bar pips mark enabled layers. The auto director stays single-scene for now. `AUDIOVIZ_SETTINGS='{"layer_mode":true}'` merges JSON into the settings for screenshots without touching the file.

**The composition model (Greg's idea, same day):** visuals (scope, ribbons, starfield) are renderers; a **layer** is one instance of a visual with its own parameters, opacity and audio band; a **scene** is up to three layers plus post and tint, saved by name. The same visual can be used twice (ribbons over ribbons). "Reacts to" per layer: full mix or a Hz band (the scope band-passes the waveform itself; other visuals get band-restricted energy, onsets and spectrum). Scene changes dissolve a frozen snapshot into the new picture (cheap, chosen over rendering two compositions). Looks are gone; the built-in scenes are Scope, Ribbons, Starfield, Deep field, Night, Party, Studio. Old settings files and old looks migrate (`config::migrate`, tests, `examples/settings_probe.rs`). Still to do from this round: a director that fades layers rather than whole scenes.

**Black hole (2026-10-17, evening).** The lensed starfield became a real black hole, Interstellar-style: `starfield.wgsl` marches rays through the Schwarzschild null-geodesic field (midpoint steps, up to 120), intersecting a thin equatorial disk between the ISCO and 9.5 rs; pixels whose impact parameter clears the disk take the second-order weak-field deflection analytically, and marched rays get the weak-field residual on exit so the two paths meet without a seam. The far side of the disk folds over and under the shadow; Doppler beaming (shift^3) and gravitational redshift make the approaching (left) side brighter and bluer; the disk texture is Keplerian-sheared periodic value noise in (azimuth, log r). Stars are hash-grid points sized in *pixel* space through the inverse Jacobian of the sky lookup (`dpdx`/`dpdy` of the octahedral map), so they stay sub-pixel under magnification; bright ones get four-point diffraction and a halo, and the field fades where a pixel spans more than a cell (photon-ring smear). Audio: rs breathes with the slewed bass, disk brightness and turbulence follow energy / onsets, the beat pulses the innermost orbits, stars sparkle on transients and breathe on a per-star band. With the default tint the disk runs blackbody (red → amber → white); any other tint drives a tint→white ramp. `StarfieldParams` keeps its name (mass, density, twinkle, drift) and gains disk, doppler, tilt; `grid` is gone. Measured 190 fps uncapped at 1440x2560 fullscreen with ribbons and scope layered on top (debug build).

**Layer levelling + reorder (2026-10-17, from Greg's first look at layers).** Greg's ribbons layer (intensity 0.05, 950 particles) vanished under a scope layer whatever the scope's opacity: the composite is additive and auto exposure levels the *sum*, so a layer thousands of times dimmer than its neighbour can never win. Now, with auto exposure on, each layer is levelled before the mix: `layer_down.wgsl` box-downsamples the layer by 8 (16 bilinear taps, full coverage so thin traces count), `layer_meter.wgsl` keeps a 1x1 ping-ponged gain per layer that aims the mean over the *lit* blocks at 0.06 (a frame mean would never reach sparse dots), slewed over 1.5 s and clamped to 0.05..10000; `layer.wgsl` multiplies opacity by that gain (flag bit 0). Opacity is now the mix ratio. Layer rows in the panel have a drag handle; dropping on another row moves the layer (settings, live visual state, band filter and meter travel together; the edited index follows).

Not done / to verify by hand: drag, double-click, Win+arrow, Snap Layouts hover on the portrait monitor; tray menu; strip controls against Spotify; the installer wizard interactively; README screenshots for 0.2.0 (`docs/img/scene-*.png` are still 0.1.0's). The old `ui.rs` is gone; `Settings` moved to `settings.rs`.

## Why

Greg's verdict on the 0.1.0 build: the scenes look considered, the shell around them does not. The stock Windows title bar, the egui debug window, and the 2005-era Inno wizard all read as "developer tool". The product should feel like one object: the window, the panel, the installer and the mark share one design language.

## Design language (fixed before any code)

- **Reference:** VS Code's integrated title bar for the chrome; Linear / Raycast for the panel density; the Swiss-grid title overlay already in the starfield scene for type.
- **Surface:** near-black `#0b0e10` with a 1 px hairline `#1c2226`; no gradients, no drop shadows except the DWM window shadow.
- **Type:** the overlay's grotesk (Segoe UI → Helvetica Neue fallback chain in `overlay.rs`), 12/13/15/22 px scale; numbers in a tabular monospace (Cascadia Mono / SF Mono fallback).
- **Accent:** the phosphor tint from settings (`settings.tint`, default `#66ff8c`) used only for state: active scene pip, focused control, calibration pulse. Everything else is grey.
- **Motion:** 120 ms ease-out for show/hide; nothing bounces.
- **Density:** 28 px chrome bar, 32 px control rows, 8 px grid.

Mockups are drawn as SVG by `cargo xtask mockup` from the token values above and rendered to `docs/img/`; Greg signs them off before implementation. Second pass (2026-10-17, after Greg's "this is 100% the right direction, go further"): seven frames covering the real states, not just the panel.

| Frame | File | What it settles |
|---|---|---|
| Panel open | `ui-mockup.png` | Chrome bar, docked panel, icon rail, Looks (presets) |
| Hover | `ui-hover.png` | Bar + bottom transport strip, status chips, tint-from-art |
| Ambient | `ui-ambient.png` | Nothing but the scene; 4 s track-change toast |
| Empty | `ui-empty.png` | Spotify not running: one sentence, two actions |
| Calibration | `ui-calibrate.png` | Tap ring, per-tap dots, verdict line |
| Install page | `installer-mockup.png` | Checks, upgrade banner, options, one button |
| Finish page | `installer-finish.png` | What happened, launch options, one tip |

## What the second pass adds (the UX, by state)

**Who it is for:** someone with Spotify open and a second monitor, running this for hours and touching it a few times a night. Every addition below is judged by "does it earn its place when the user is not looking at it?"

Ambient (most of the time)
- Nothing on screen but the scene. Chrome, strip and notices are all hover-revealed and fade after 3 s of mouse rest; fullscreen hides them too.
- Track-change toast: art thumb, title, artist, bottom-left, 4 s, in the scenes without a title overlay. Off switch in Display.
- **Reduce flashing** toggle (Display): caps the frame-to-frame luminance delta for photosensitive viewers. This is a safety feature, on by default for the first run on an unknown display, and it ships before anything cosmetic.

Hover (bar + strip)
- Bar: mark · name · scene pips (1 2 3, A auto) · status chips (capture source, sync ms, monitor + fps) · window controls. Chips are read-only glances; clicking one opens the matching panel section.
- Strip: album art, title, artist · album, previous / play-pause / next via the Windows media session (SMTC supports control, not just reading), progress with elapsed / total. Controlling Spotify from the visualizer on the second monitor is the single most useful addition.
- **Tint from album art**: the scene accent follows the dominant art colour (we already receive the art). Toggle on the strip; the fixed phosphor tint remains the default.
- `?` shows every shortcut as an overlay.

Panel
- Docked right, translucent over the scene, icon rail: Scene, Post, Sync, Display, Looks, Meters, About.
- **Looks**: named presets of all settings (Night, Party, Studio ship as examples). Save current, export / import JSON (share a look with someone). Scene-and-look are what a non-tinkerer actually wants to switch.
- Display: monitor picker as a small diagram of the monitors (like Windows display settings), start fullscreen on it, fps cap, low-power mode (30 fps when the window is not visible or on battery), toast on/off, reduce flashing.
- About: version, license, privacy summary, **Copy diagnostics** (GPU, backend, capture mode, fps, settings path) for bug reports, link to the releases page (opened in the browser, the app itself never goes online).
- Meters behind a developer toggle.

Empty state (Spotify not running or nothing playing)
- One sentence, which device is being listened to meanwhile, two actions: Open Spotify, Pick another app (sets the capture target). Replaces the current bottom-left notice for this case.

Calibration
- Full overlay: pulsing tap ring, dots per tap (matched / missed), the numbers, and a verdict ("Looking good" under ±15 ms, "Taps are scattered, try a track with a clear kick" otherwise). Enter applies, C cancels.

Tray
- Start-at-sign-in installs minimised to the tray. Tray menu: show, scene, fullscreen on monitor N, pause capture, quit. For an ambient app the tray is the primary control surface when the window is on the other monitor.

Installer (Inno, step 1)
- Checks before installing: Windows build, DirectX 12 GPU name, Spotify present, current output device. Failing a check explains it on the page instead of failing after install.
- Upgrade banner with "your settings are kept"; location with Change; three options (Start Menu, desktop, start at sign-in minimised to tray); one footer line with size, the unsigned status and where the checksum is.
- Finish page: what was done, Launch, Open Spotify too, Start fullscreen on my second monitor, one tip.
- Uninstaller: same visual, "keep my settings" default, done in one page.

Not in this phase: album-art-driven scenes beyond the tint, multiple windows, macOS.

## Scope, in build order

1. **Chrome** (#1). `with_decorations(false)`; an egui top bar: mark · "audioviz" · scene pips (1 2 3, active in accent) · now-playing (artist — title, truncated) · min / max / close. Drag to move via `window.drag_window()`, edges via `drag_resize_window()`. Snap layouts and Aero Snap need `WM_NCHITTEST`: subclass the HWND with the `windows` crate (`SetWindowSubclass`) and return `HTCAPTION` for the bar, `HTLEFT`… for an 6 px border, `HTCLIENT` elsewhere; keep DWM rounded corners with `DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE)`. Bar hides in fullscreen and in windowed mode after 3 s without mouse movement; reappears on movement near the top edge. macOS: `with_titlebar_transparent` + `with_fullsize_content_view` later (Phase 7).
2. **Panel** (#2). Replace `egui::Window` with a right-docked `SidePanel` (320 px, slides in): icon rail on its left edge (Scene, Post, Sync, Display, Meters, About), one section at a time, section title + reset at the top, sliders with tabular readouts, keyboard hints in the footer. Custom `egui::Style`: no window frames, 4 px radii, hairline separators, accent on focus only. Meters (Beat, Level, Chroma) move behind a "developer" toggle in Display. Panel state (open section, developer) persists in settings.
3. **Installer** (#3), step 1: Inno `WizardStyle=modern`, `WizardSizePercent=120`, custom `WizardImageFile`/`WizardSmallImageFile` rendered by `cargo xtask icon` (mark on `#0b0e10`), `DisableWelcomePage=no` with a single sentence, `DisableReadyPage=yes`, finish page with "Launch" only. Uninstaller gets the same images. Step 2 (bespoke Rust installer or MSIX) stays in `docs/ROADMAP.md` until signing exists.
4. **First run** (#4). Three-step sheet in the new panel style: monitor, tap calibration, starting scene. Skippable; never shown again; each step reachable from the panel.
5. **Brand** (#5). Stroke weights at 16/24/32 px, monochrome variant for the bar, wizard images, all from `assets/icon.svg` via xtask.

Out of scope: new scenes, analysis changes, macOS.

## Risks

- winit 0.30 on Windows with `decorations(false)` loses the DWM shadow and snap unless the hit-test hook is right; budget a day for the `WM_NCHITTEST` subclass and test with Win+arrow, Snap Layouts hover on the max button, and the 4K/240 Hz plus portrait monitor pair.
- egui `SidePanel` steals the full window height; the scene must render underneath it (translucent panel over the composite) or the composite viewport shrinks. Decision: translucent (`#0b0e10` at 92 %), scene unchanged.
- Any change to `Settings` must migrate (see `config::migrate_post` for the pattern); never drop a tuned value.

## Verification

- `scripts/snapshot.ps1` at 1280x800 and 1440x2560 portrait for: bar visible, bar hidden, panel open on each section, first-run sheet, fullscreen.
- Manual: drag by bar, resize from every edge, Win+Left/Right, Snap Layouts, double-click bar to maximize, Esc still quits, Tab still toggles the panel.
- Installer: run the wizard interactively (not silent) and screenshot every page for the issue.

## Exit criteria

- Greg's sign-off on the mockup, then on the running build on the portrait monitor.
- All five issues closed; `v0.2.0` published with new README screenshots.
