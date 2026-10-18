# Roadmap and open issues

Working backlog beyond the phase briefs. Phase briefs (`docs/phase*.md`) carry the design and the progress log; this file is the list of what is still open, in rough order, with the reasoning. Items with a GitHub issue say so; mirroring lags, the file wins.

Rewritten 2026-10-18 after the Sigil, panel, colour, casting and blend-mode work. The 0.2.0 code was **pushed to `main` on 2026-10-18** (39 commits, no tag, no release; see item 4). The evening's five roadmap commits (fading director, colour plumbing, scope frequency colour, microphone source, performance pass) were **pushed the same night** at Greg's word, still code only.

## Now: finish 0.2.0

Pushed as code; the installer Greg tests is his local `dist/audioviz-0.2.0-setup.exe` build.

1. **Hand tests** that scripts cannot do (mouse paths never reach egui from posted messages):
   - Panel: drag a stack row by its hue bar, the modifier chips, inline opacity, dropdowns, typing into a value, double-click reset, Esc out of a modifier page, the 108 px label column (does anything truncate that matters?).
   - Blend modes: set a layer to *over*, move it between slots 1 and 2 with the arrows.
   - Neon look on a scene you know well; it changes how every bright pixel renders.
   - Casting (experimental): Display › Cast to a TV against the Roku, with Spotify playing on the PC.
   - Still owed from earlier: the portrait-monitor fullscreen cut-off (not reproduced here; needs a screenshot), tray menu, strip controls against Spotify, the installer wizard interactively, Win+arrow and Snap Layouts on the portrait monitor.
2. ~~Look feedback on the Sigil~~ Greg, 2026-10-18 evening: "fantastic, it grooves to music, and the colour plumbing now makes this look very close in feel to the original inspiration." No further Sigil pass queued; the kept candidates below stay available.
3. **README screenshots for 0.2.0**: `docs/img/scene-*.png` are still 0.1.0's; add the sigil and the new panel.
4. **Push, no release** (Greg, 2026-10-18): 0.2.0 goes out as pushed code only, no tag, no installer, no release notes yet; while the work is moving fast nobody rebuilds binaries per change. Greg runs `target
eleaseudioviz.exe` or builds the installer himself when he wants one. A tagged release with notes comes when the pace settles.

## Now: more visuals, modifiers and effects (Greg, 2026-10-18 late)

Greg: "I want to focus on adding visuals." Shipping is deferred (below) except the measured default delay. Taste to build against: spacey, lines, post-modern, neon palettes, striking over realistic; every new piece is an ordinary layer (bands, tint, palette, modifiers, blend) and gets a built-in scene. One per commit, a snapshot to Greg before any tuning.

Inspiration Greg handed over (2026-10-18 late), credits kept in `docs/INSPIRATION.md`: **soniaboller/audible-visuals** (three.js; port the visuals worth it, credit the author), and **Uberviz** by Felix Turner (uberviz.io; the Crystal visualizer, the halftone, the colour/monotone drift). Uberviz's licence is unknown, so it is inspiration only unless a licensed repo turns up.

~~Visual presets~~ built 2026-10-18 night (`preset_store.rs`: built-in plus saved per visual; Ribbons › Starfield is Greg's). More built-ins welcome as looks get found.

Visual candidates, in the order to try (reordered 2026-10-18 late on the research; details in `docs/INSPIRATION.md`):

- ~~Waves~~ **built 2026-10-18** (waveform or spectrum rows in perspective, hidden-line fill, palette by age or level; built-in scene). Awaiting Greg's look.
- ~~Crystal~~ **built 2026-10-18** (after Uberviz, written fresh): tumbling glass shards, loudness-scaled, one shard eased to a new pose per beat, scatter over the bar, palette facets and glowing edges; built-in scene. Awaiting Greg's look.
- ~~Loop~~ **built 2026-10-18** (after Uberviz): concentric waveform rings, each lagging the last, brightness by level; factory scene Loop. Awaiting Greg's look.
- **Eclipse** (after Uberviz): a black disc ringed by spectrum glow dots; one fragment pass.
- **Ripples** (after Uberviz): a noise-displaced plane printed with contour lines or dots.
- ~~Wheel~~ **built 2026-10-18** (after Uberviz): spectrum wedges, rays, beat jumps, walking bands; backdrop of the built-in Lantern scene.
- **Spiral** (after audible-visuals, Apache-2.0): parametric spirals, slow parameter drift, waveform displacement, drawn as a polyline.
- **Tunnel**: a wireframe tunnel of rings flown through, each ring's radius the spectrum at the moment it was born; lines only.
- **Strings**: chords of the chroma drawn as taut lines between points on a circle (string art); a chord change re-strings it.
- **Harmonograph**: damped Lissajous of the two strongest pitch classes, drawn as one long line.
- **Interference**: a few sinusoid line fields driven by bands, moiré where they cross.

Modifier candidates: ~~warp~~ (built 2026-10-18), ~~halftone~~ (built 2026-10-18; dither still open), ~~scanlines~~ and ~~mirror~~ (built 2026-10-18), ~~glow~~, ~~vignette~~ and ~~colorify~~ (built 2026-10-18 late night; Lantern's Crystal runs Glow and Vignette pinned, Colorify in the pool), ~~beat roll~~ (built 2026-10-18 as the **effects director**, `docs/model.md`), **glitch** (block displacement and channel tears on onsets), **posterize / hue cycle**, **scanlines / CRT**, **echo** (a delayed copy under the picture, reusing the history pair).

## Done: visuals and sound (the previous "Next" list)

- ~~Director that fades layers~~ **done 2026-10-18**: per-slot eased opacity ramps, a replaced visual fades out under its successor, the auto director staggers the slots bottom first (`docs/phase6-ui.md` log).
- ~~Sigil second pass~~ Not needed after Greg's 2026-10-18 verdict (above). Candidates kept ready in the code if he ever asks: cardinal triangles per level, more polygram types, a music-written trace (last struck pitch classes instead of a seeded path).
- ~~More colour plumbing~~ **done 2026-10-18**: the edge modifier colours by Picture, Tint or Palette (direction-mapped); every modifier has its own "Reacts to" band (default: the layer's). A blend-mode per modifier is *not* planned (Mix covers it).
- ~~Scope beam colour by instantaneous frequency~~ **done 2026-10-18**: "Palette by: Frequency" on the scope, a zero-crossing estimate per point mapped by octave; Key and speed stays the default.
- ~~Microphone source~~ **done 2026-10-18**: Sync › Source › The room (default microphone, WASAPI shared mode). Opens and streams here; untested with music in the room.
- ~~Performance pass on the release build~~ **done 2026-10-18** (table in `docs/phase6-ui.md`): release at 2560x1440 is 160 fps with three plain layers and 63 fps with six modifiers on each; every modifier pass costs 0.1 to 0.5 ms except **fractal at about 2.4 ms** (incoherent deep-level reads). Found and fixed on the way: Feedback plus Luminescence in one chain crashed the app (shared history pair; now one pair per slot, on demand). Monitor 1 runs at 59 Hz, which the startup log now shows.
- ~~Fractal cost~~ **done 2026-10-18**: a per-slot mip pyramid of the fractal's input, read at the lod of each level's magnification. Six fractals per layer 20 → 62 fps; the heavy set 63 → 98 fps.

## Casting (experimental)

- Current: Windows Miracast via the cast panel, auto-detect the new display, one click to fullscreen there. Roku takes no Google Cast.
- Only if mirroring disappoints: a native Roku channel fed by an encoded live stream (Media Foundation H.264 → HLS, BrightScript player). Weeks of work and seconds of lag to compensate; not planned.

## Measured default audio delay (kept, needs Greg)

On the reference rig with wired output: press **C**, tap along for ten beats, read the offset the sheet shows (it is also in the Sync section after applying). Tell the session the number; if it is clearly non-zero it becomes `audio_delay_ms`'s shipped default, with a note in the Sync hint that it was measured on one rig.

## Shipping (deferred by Greg, 2026-10-18 late)

- **Bespoke installer** (issue #3): the app's own design instead of Inno. Deferred; Inno stays. When it comes back: a dark Inno skin on the app's theme tokens is a day, a self-made installer is a week and worse for SmartScreen while unsigned.
- **Code signing**: Azure Artifact Signing, wired in `release.yml` behind `CODE_SIGNING`; needs the account, a certificate profile and the five secrets. Ship unsigned until then (decided Phase 5). Deferred.
- **winget** submission after a signed public release; no winget mention anywhere user-facing until it is real. Deferred with signing.
- **Settings file**: every schema change so far migrates (`config::migrate`, `Layer::normalize`, serde defaults and aliases). Keep it that way; a dev run must never clobber `%APPDATA%\audioviz\settings.json` (`config::save` refuses under `AUDIOVIZ_DEFAULTS`).

## Later: macOS

- Core Audio process tap (macOS 14.2+) behind `AudioSource`; MediaRemote / Now Playing behind `MetadataSource`.
- `.app` bundle, notarization, Homebrew cask. The MacBook Pro M4 Max is the target machine.
- `platform/other.rs` already stubs the Windows-only entry points (open URL, launch Spotify, battery, update check, cast) so the macOS build compiles.

## Done since v0.1.0 (for orientation)

Phase 6 UI (chrome, panel, strip, overlays, tray, installer wizard), the composition model (visuals → layers → scenes, bands, per-layer tint, levelling, reorder), black hole v2, kaleidoscope visual, the modifier chain (ten kinds, Mix per modifier), the colour engine (OKLab palettes, presets, per-visual plumbing, neon tonemap), the Sigil visual (three passes), the Phase 7 inspector panel (stack, breadcrumb, tabs, one-line slider rows, groups, solo), blend modes, and the experimental cast flow. Details and dates in `docs/phase6-ui.md` and `docs/phase7-controls.md`.
