# Roadmap and open issues

Working backlog beyond the phase briefs. Each item here is mirrored as a GitHub issue; this file holds the reasoning, the issue holds the discussion.

## UI refresh (Greg, 2026-10-17): "look exceptional"

Greg's direction after testing the Phase 5 build: the app and installer work, but they look stock. Target is a product that looks as considered as the scenes do.

1. **Custom window chrome, VS Code style** (`ui-chrome`). Drop the standard Windows title bar. A thin, dark, integrated bar: the mark at left, "audioviz" in the Swiss grotesk, scene pips in the centre, now-playing at right, window controls drawn by us. Hidden in fullscreen; appears on mouse-move near the top edge in windowed mode. Implementation: `winit` `with_decorations(false)` + egui-drawn bar + `window.drag_window()` / `drag_resize_window()` for move and resize; Windows snap layouts need `WM_NCHITTEST` handling (winit exposes `with_titlebar_hidden`-style options only on macOS, so on Windows this is a small `windows` crate hook on the raw window). Keep the DWM rounded corners and shadow.
2. **Settings panel redesign** (`ui-panel`). The egui debug window becomes a docked right-hand sheet (VS Code side-panel feel): sections as a left icon rail (Scene, Post, Sync, Display, Meters), one section visible at a time, monospace numeric readouts, the same phosphor accent as the scenes. Debug-only meters move behind a "developer" toggle. Replace egui's default dark visuals with a custom `egui::Style` (type scale, 4 px radius, hairline separators, no window title bar).
3. **Installer and uninstaller look** (`installer-modern`). The default Inno wizard is 2005. Options, cheapest first: (a) keep Inno, `WizardStyle=modern` with custom `WizardImageFile` / `WizardSmallImageFile` (phosphor mark on black, 164x314 and 55x58), dark `WizardResizable=no`, a single-page flow (`DisableWelcomePage`, `DisableReadyPage`), and a custom finish page; (b) a bespoke installer window in Rust (the app itself with `--install`, copying files, writing the uninstall key and shortcuts) so the installer *is* the product's design; (c) MSIX for a Store-grade experience, which also removes the SmartScreen issue when signed. Recommendation: (a) now, (b) when the chrome work lands, since the two share the egui style.
4. **First-run experience** (`ui-first-run`). Replace the one-line hint with a short in-app sequence: pick monitor, calibrate delay by tapping, choose the starting scene. Dismissable, never shown again, all three steps reachable later from the panel.
5. **Icon and splash** (`brand`). The Lissajous mark is right; refine stroke weights at 16/32 px, add a monochrome variant for the title bar and tray, and render the installer wizard images from the same SVG via `cargo xtask icon`.

## Shipping

- Code signing: Azure Artifact Signing, wired in `release.yml` behind `CODE_SIGNING`. Needs the account, a certificate profile, and the five secrets.
- winget submission after the first public release (`wingetcreate submit` with the rendered manifests attached to the release).
- Measured default audio delay: Greg's tap calibration result on the reference rig goes into `docs/phase1-spike.md` and becomes the shipped default if it is clearly non-zero.

## Phase 6: macOS

- Core Audio process tap (macOS 14.2+) behind `AudioSource`; MediaRemote / Now Playing behind `MetadataSource`.
- `.app` bundle, notarization, Homebrew cask.
