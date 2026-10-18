# Roadmap and open issues

Working backlog beyond the phase briefs. Phase briefs (`docs/phase*.md`) carry the design and the progress log; this file is the list of what is still open, in rough order, with the reasoning. Items with a GitHub issue say so; mirroring lags, the file wins.

Rewritten 2026-10-18 after the Sigil, panel, colour, casting and blend-mode work. The 0.2.0 code was **pushed to `main` on 2026-10-18** (39 commits, no tag, no release; see item 4).

## Now: finish 0.2.0

Pushed as code; the installer Greg tests is his local `dist/audioviz-0.2.0-setup.exe` build.

1. **Hand tests** that scripts cannot do (mouse paths never reach egui from posted messages):
   - Panel: drag a stack row by its hue bar, the modifier chips, inline opacity, dropdowns, typing into a value, double-click reset, Esc out of a modifier page, the 108 px label column (does anything truncate that matters?).
   - Blend modes: set a layer to *over*, move it between slots 1 and 2 with the arrows.
   - Neon look on a scene you know well; it changes how every bright pixel renders.
   - Casting (experimental): Display › Cast to a TV against the Roku, with Spotify playing on the PC.
   - Still owed from earlier: the portrait-monitor fullscreen cut-off (not reproduced here; needs a screenshot), tray menu, strip controls against Spotify, the installer wizard interactively, Win+arrow and Snap Layouts on the portrait monitor.
2. **Look feedback** on the Sigil (drips, centre, colours) and the colour plumbing (ribbons colour spread, luminescence in the picture's colours).
3. **README screenshots for 0.2.0**: `docs/img/scene-*.png` are still 0.1.0's; add the sigil and the new panel.
4. **Push, no release** (Greg, 2026-10-18): 0.2.0 goes out as pushed code only, no tag, no installer, no release notes yet; while the work is moving fast nobody rebuilds binaries per change. Greg runs `target
eleaseudioviz.exe` or builds the installer himself when he wants one. A tagged release with notes comes when the pace settles.

## Next: visuals and sound

- **Director that fades layers** rather than dissolving whole scenes (from the composition-model round). Needs per-layer target opacities and a slew in the director; the UI already has the stack.
- **Sigil second pass** after Greg's look: drip tuning, the centre, and whatever the hand test turns up. Candidates kept ready in the code: cardinal triangles per level, more polygram types, a music-written trace (last struck pitch classes instead of a seeded path).
- **More colour plumbing**: edge modifier in palette colours; a per-layer "reacts to" band for modifiers; a blend-mode per modifier is *not* planned (Mix covers it).
- **Scope**: beam colour by instantaneous frequency (a cheap zero-crossing estimate on the CPU) as an option beside key + speed.
- **Microphone source** ("listen to the room") for setups where the music does not play on the PC. Not needed for Greg's room (Sonos on the TV's HDMI); worth doing before anyone else uses casting.
- **Performance pass on the release build**: all numbers so far are debug builds at 1440x2560; measure release, and the cost of three layers each with six modifiers, before 0.2.0 ships.

## Casting (experimental)

- Current: Windows Miracast via the cast panel, auto-detect the new display, one click to fullscreen there. Roku takes no Google Cast.
- Only if mirroring disappoints: a native Roku channel fed by an encoded live stream (Media Foundation H.264 → HLS, BrightScript player). Weeks of work and seconds of lag to compensate; not planned.

## Shipping

- **Bespoke installer** (issue #3): the app's own design instead of Inno, when there is time. Inno stays for 0.2.0.
- **Code signing**: Azure Artifact Signing, wired in `release.yml` behind `CODE_SIGNING`; needs the account, a certificate profile and the five secrets. Ship unsigned until then (decided Phase 5).
- **winget** submission after a signed public release; no winget mention anywhere user-facing until it is real.
- **Measured default audio delay**: Greg's tap calibration on the reference rig becomes the shipped default if clearly non-zero.
- **Settings file**: every schema change so far migrates (`config::migrate`, `Layer::normalize`, serde defaults and aliases). Keep it that way; a dev run must never clobber `%APPDATA%\audioviz\settings.json` (`config::save` refuses under `AUDIOVIZ_DEFAULTS`).

## Later: macOS

- Core Audio process tap (macOS 14.2+) behind `AudioSource`; MediaRemote / Now Playing behind `MetadataSource`.
- `.app` bundle, notarization, Homebrew cask. The MacBook Pro M4 Max is the target machine.
- `platform/other.rs` already stubs the Windows-only entry points (open URL, launch Spotify, battery, update check, cast) so the macOS build compiles.

## Done since v0.1.0 (for orientation)

Phase 6 UI (chrome, panel, strip, overlays, tray, installer wizard), the composition model (visuals → layers → scenes, bands, per-layer tint, levelling, reorder), black hole v2, kaleidoscope visual, the modifier chain (ten kinds, Mix per modifier), the colour engine (OKLab palettes, presets, per-visual plumbing, neon tonemap), the Sigil visual (three passes), the Phase 7 inspector panel (stack, breadcrumb, tabs, one-line slider rows, groups, solo), blend modes, and the experimental cast flow. Details and dates in `docs/phase6-ui.md` and `docs/phase7-controls.md`.
