# Phase 6 — UI refresh ("look exceptional")

Status: **brief, not started** (written 2026-10-17 right after v0.1.0 shipped). Target version 0.2.0. Tracked as issues #1-#5.

## Why

Greg's verdict on the 0.1.0 build: the scenes look considered, the shell around them does not. The stock Windows title bar, the egui debug window, and the 2005-era Inno wizard all read as "developer tool". The product should feel like one object: the window, the panel, the installer and the mark share one design language.

## Design language (fixed before any code)

- **Reference:** VS Code's integrated title bar for the chrome; Linear / Raycast for the panel density; the Swiss-grid title overlay already in the starfield scene for type.
- **Surface:** near-black `#0b0e10` with a 1 px hairline `#1c2226`; no gradients, no drop shadows except the DWM window shadow.
- **Type:** the overlay's grotesk (Segoe UI → Helvetica Neue fallback chain in `overlay.rs`), 12/13/15/22 px scale; numbers in a tabular monospace (Cascadia Mono / SF Mono fallback).
- **Accent:** the phosphor tint from settings (`settings.tint`, default `#66ff8c`) used only for state: active scene pip, focused control, calibration pulse. Everything else is grey.
- **Motion:** 120 ms ease-out for show/hide; nothing bounces.
- **Density:** 28 px chrome bar, 32 px control rows, 8 px grid.

A one-page mockup (`docs/img/ui-mockup.png`, drawn as SVG by `cargo xtask mockup` so it stays in sync with the token values above) is the first deliverable; Greg signs it off before implementation.

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
