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
- ~~Eclipse~~ **built 2026-10-18** (after Uberviz): a black disc ringed by spectrum glow dots with a bass corona; one fragment pass; factory scene Eclipse. Awaiting Greg's look.
- ~~Ripples~~ **built 2026-10-18** (after Uberviz): a flowing noise-displaced plane printed with contours, dots or a grid, rings on beats, depth-darkened; factory scene Ripples. Awaiting Greg's look.
- ~~Wheel~~ **built 2026-10-18** (after Uberviz): spectrum wedges, rays, beat jumps, walking bands; backdrop of the built-in Lantern scene.
- ~~Spiral~~ **built 2026-10-18** (after audible-visuals, Apache-2.0, idea only): one polyline through drift-stepped points, Spiral or Rose, waveform displacement; factory scene Spiral. Awaiting Greg's look.
- **Tunnel**: a wireframe tunnel of rings flown through, each ring's radius the spectrum at the moment it was born; lines only.
- **Strings**: chords of the chroma drawn as taut lines between points on a circle (string art); a chord change re-strings it.
- ~~Harmonograph~~ **built 2026-10-18** as a loaded visual (the proof of the line mode): damped Lissajous at the interval of the two strongest pitch classes, gliding between intervals; factory scene Harmonograph. Awaiting Greg's look.
- **Interference**: a few sinusoid line fields driven by bands, moiré where they cross.

### Scenes section (Greg, 2026-10-18 night)

~~Rework~~ **done 2026-10-18**: Now showing card (changed mark, Save / Save as / Revert), one list with Auto-rotation ticks, the visuals per scene, and a per-row menu (Duplicate, Rename, Delete, Restore). Hand test owed. Considered and not built: thumbnails per scene (Greg chose the list).

### Pluggable visuals (Greg, 2026-10-18 night)

The engine and the visuals are separate: a visual is a registry entry (`docs/visuals.md`), built in or loaded from a folder (`visual.json` + `visual.wgsl`). Eclipse was the first port. In order:

1. ~~**Building visuals guide**~~ **done 2026-10-18**: `docs/building-visuals.md` (layout, manifest, API with units, conventions, three worked examples shipped as factory visuals Polygon, Pulse and Comet, a performance budget, debugging, sharing, limits). The API gained `uncentred`, `rotate` and `noise`, and loaded visuals now see only the API (not the built-ins' helper names); a test compiles every factory visual through naga.
2. ~~**Panel support for authoring**~~ **done 2026-10-18**: Visual tab › Your own visuals (New visual from `assets/visual-template/`, Open visuals folder, Restore factory visuals, the guide), Open its folder per loaded visual, a "Loaded" mark in the Visual list, folders picked up without a restart. Hand tests owed (`docs/phase6-ui.md`).
3. ~~**A per-frame `prepare` step**~~ **done 2026-10-18**: `"items": N` + `fn prepare(i) -> vec4` as a compute pass, read by `shade` via `item(i)`, last frame's values inside `prepare` (per-item state). Eclipse 1.12 → 0.69 ms at 16 dots, 4.5 → 2.7 at 64. ~~The album art as a texture~~ **done 2026-10-18** (`art(uv)`, `V.has_art`). ~~A line drawing mode~~ **done 2026-10-18** (`"line": N` + `fn point(i) -> LinePoint`; Harmonograph is the proof). Follow-up: an unedited factory visual in the user's folder is never upgraded when the shipped copy changes (install writes missing files only), so an older Eclipse keeps its slower code until Restore factory visuals; a stamp of what was installed would allow upgrading files nobody edited.
4. ~~**A second port**~~ **done 2026-10-18**: Spectrum wheel is a loaded visual (`assets/visuals/wheel/`, id `wheel`, the same twelve parameter names); compared side by side and measured (0.18 ms compiled in, 0.16 loaded) before `scenes/wheel.rs`, its two shaders and `WheelParams` were removed. Greg's saved Wheel blocks (settings, Lantern) checked read-only: every key is a manifest parameter.

**Converting every visual (Greg, 2026-10-18 night: "proceed with getting everything converted"; easy ones first, then a format feature per group).** ~~Spiral~~ done (0.26 → 0.20 ms; the format gained `line_count` and line visuals without `shade`). ~~Scope~~ done (0.11 → 0.09 ms, Frequency mode on the GPU; the format gained `sample(i)`, `V.wave_new`, `V.auto_gain`, `V.sample_rate`, `line_style: "beam"`, `band_waveform`, choice `values`, and embedded fallbacks for shipped visuals). ~~Loop~~ done (0.09 → 0.13 ms; items up to 16384; reserved-word param ids allowed as `id_`). ~~Black hole~~ done (4.0 → 4.15 ms; its ray march carried over through a private uniform struct). The four that fitted the format are all loaded. ~~Ripples~~ done with the new triangle mode (0.26 ms both ways; also `V.track`). ~~Waves~~ done (0.20 ms both ways). ~~Crystal~~ done (0.07 ms both ways). ~~Ribbons~~ done (particles as prepare items, up to 262144; `V.instance`; the same cost). ~~Kaleidoscope~~ done (the picture feed: album art or the photos folder, `picture()` / `picture_prev()` with a crossfade). ~~Sigil~~ done 2026-10-19 (the format gained `trail`, `button`, `zero`, `decimals`, 24 params and onset hits/strengths, beat confidence, silence and the slow chroma; 2.7 ms against 2.8). **Every visual is loaded; the compiled-in path is gone.**

~~**Build pipelines off the render thread**~~ **done 2026-10-19**: a worker compiles, the outgoing visual keeps drawing, the new layer fades in when ready; builds are cached by source for the run and every scene's visual is prewarmed at startup. Worst frame through scene switches 12 ms (was up to 2.7 s). Possible later: a cache across runs (wgpu's pipeline cache is Vulkan-only; FXC re-runs every launch, the driver caches its own stage).

**Creatures (Greg, 2026-10-19: procedural animation, "jellyfish like moving"; then "pull out the stops", species and their light with the sound).** ~~Jellyfish, first pass~~ (2D neon swarm). ~~Jellyfish in 3D with six species~~ (one species per layer, Palette or Natural colours, Music: Natural, Reactive, Performance; scenes Jellyfish, Deep sea, Giants) and ~~the creature library~~ (`"uses": ["creatures"]`). Next candidates once Greg has looked: a hero close-up mode (one jelly near the camera, more rings and strands), comb-jelly tentacle branches (tentilla), a "ballet" director that lines strokes up on phrases, then other creatures on the library (eels, a fish shoal, birds).

Next candidates for the format: a depth test, pictures of a visual's own. ~~Upgrading unedited factory files in place~~ done 2026-10-19 (visuals, presets and scenes: a copy matching a version that shipped is updated, an edited one kept; `assets/factory-history.txt`).

Modifier candidates: ~~warp~~ (built 2026-10-18), ~~halftone~~ (built 2026-10-18; dither still open), ~~scanlines~~ and ~~mirror~~ (built 2026-10-18), ~~glow~~, ~~vignette~~ and ~~colorify~~ (built 2026-10-18 late night; Lantern's Crystal runs Glow and Vignette pinned, Colorify in the pool), ~~beat roll~~ (built 2026-10-18 as the **effects director**, `docs/model.md`), **glitch** (block displacement and channel tears on onsets), **posterize / hue cycle**, **scanlines / CRT**, **echo** (a delayed copy under the picture, reusing the history pair).

## Done: visuals and sound (the previous "Next" list)

- ~~Director that fades layers~~ **done 2026-10-18**: per-slot eased opacity ramps, a replaced visual fades out under its successor, the auto director staggers the slots bottom first (`docs/phase6-ui.md` log).
- ~~Sigil second pass~~ Not needed after Greg's 2026-10-18 verdict (above). Candidates kept ready in the code if he ever asks: cardinal triangles per level, more polygram types, a music-written trace (last struck pitch classes instead of a seeded path).
- ~~More colour plumbing~~ **done 2026-10-18**: the edge modifier colours by Picture, Tint or Palette (direction-mapped); every modifier has its own "Reacts to" band (default: the layer's). A blend-mode per modifier is *not* planned (Mix covers it).
- ~~Scope beam colour by instantaneous frequency~~ **done 2026-10-18**: "Palette by: Frequency" on the scope, a zero-crossing estimate per point mapped by octave; Key and speed stays the default.
- ~~Microphone source~~ **done 2026-10-18**: Sync › Source › The room (default microphone, WASAPI shared mode). Opens and streams here; untested with music in the room.
- ~~Performance pass on the release build~~ **done 2026-10-18** (table in `docs/phase6-ui.md`): release at 2560x1440 is 160 fps with three plain layers and 63 fps with six modifiers on each; every modifier pass costs 0.1 to 0.5 ms except **fractal at about 2.4 ms** (incoherent deep-level reads). Found and fixed on the way: Feedback plus Luminescence in one chain crashed the app (shared history pair; now one pair per slot, on demand). Monitor 1 runs at 59 Hz, which the startup log now shows.
- ~~Fractal cost~~ **done 2026-10-18**: a per-slot mip pyramid of the fractal's input, read at the lod of each level's magnification. Six fractals per layer 20 → 62 fps; the heavy set 63 → 98 fps.

## Next: the audio engine after its first pass (ADR-0006, 2026-10-21)

Steps 0 to 5 each have a first version (0.4.30 to 0.4.38; working log `docs/phase8-audio.md`). In order:

1. **Greg's look** at 0.4.38 (installer built): the director view on his own music (D, or Sync > Director view); linking a slider or two; whether scenes feel different now the bands moved (0.4.33: four bass bands instead of eight, more in the mids, quicker); the bar and strip holding still with the view open. Capture hand tests: pause Spotify (`made up` should grow a second a second in the debug line), switch the default output on endpoint loopback, unplug the output.
2. **Visuals use the new signals.** The payoff of the work. Done: the API has a name for every signal and the guide a "Choosing a signal" (0.4.40); the Jellyfish strokes and blooms on real drops, gathers through a build and strokes on the bar in Reactive (0.4.41); Wheel, Waves, Pulse and Eclipse have Spectrum: Each band or In balance, the Wheel a Punch (0.4.42). Then bar, build, drop and section motion in Spiral, Harmonograph, Comet, Kaleidoscope (0.4.44), Ribbons, Ripples, Crystal, Loop (0.4.45) and the Sigil (0.4.46; a glyph per section, 0.4.47), with `section_seed()` and `ease()` in the API. Left: Greg's eye on all of it with real music (In balance may deserve to be the default; the Kaleidoscope's section tumble is unseen, the lab has no album art); factory scenes that link sliders to the new signals. Per ADR-0004: entrain, do not trigger; check with the lab's `--run`.
3. **Tuning on real music.** Three minutes from Greg's Spotify (`tmp/live-3min.wav`, track unknown) had the tempo wander 131 to 142 BPM: find the track and fix it. Record more clips, EDM and rock/pop, and run the three measuring examples on each.
4. **Open items of the steps.** Each signal's delay from sound to screen (1c); saving the history as a session (1b); sections: changes of chords alone are missed, labels are approximate, a four-bar section is predicted as eight; links: choices, toggles and integer sliders cannot be linked (a linked slider shows the live value since 0.4.48; effects' and the Picture's sliders link since 0.4.49); the director view costs ~2 ms CPU a frame while open (from ~4 before 0.4.50; the rest is tessellating its curves).
5. **Later, its own ADR:** song mapping from files (a WAV or a whole song analysed ahead, scenes timed to it, rendering videos), Greg's studio timeline (`docs/model.md`). The live signals and links are what it would drive.

## Casting (experimental)

- Current: Windows Miracast via the cast panel, auto-detect the new display, one click to fullscreen there. Roku takes no Google Cast.
- Only if mirroring disappoints: a native Roku channel fed by an encoded live stream (Media Foundation H.264 → HLS, BrightScript player). Weeks of work and seconds of lag to compensate; not planned.

## Measured default audio delay (kept, needs Greg)

On the reference rig with wired output: press **C**, tap along for ten beats, read the offset the sheet shows (it is also in the Sync section after applying). Tell the session the number; if it is clearly non-zero it becomes `audio_delay_ms`'s shipped default, with a note in the Sync hint that it was measured on one rig.

## Shared space: build the variants apart (2026-10-19)

Since diffusion (0.4.5) a visual with `"space"` compiles its triangles twice, stacked and shared, in one job: the Jellyfish builds in 10.0 s instead of 5.6, and a shared jelly scene shows nothing until both are done. Build them as two jobs, the variant in use first (the visual would need to know its scene's space mode when it asks), and prewarm the other.

## Parked: the lion's mane (Greg, 2026-10-19: "done with this lion's mane for now")

ADR-0005 steps 1 to 3 are done; step 4 (bell, mass) has had three passes and step 5 (the mane) its first part, all in 0.4.24 to 0.4.28. Where it stands and what is left, for when it comes back:
- **The mass** (0.4.28, `lion/plume.wgsl`): 40 real lumps, the right form by Greg's direction (a billowing solid, like a rolling smoke plume or milky fluid). Next: brighter, glowing cream highlights (it reads orange-tan beside photo 42) and a denser veil of hair over it just under the bell.
- **The hair** (0.4.26, `lion/hair.wgsl`): 1024 hairs a jelly following the 160 strands; Greg likes it. Left from step 5: ribbons through each lock that cover in the cover pass; then drop the stopgap dimming by `behind` in `lion/hair.wgsl` and `lion/mane.wgsl`. The curls kink a little at 16 links a hair.
- **Step 6:** motion (slow weak pulses, arms bunch on the squeeze and lengthen as it relaxes, hair stretches and lags), colours through palettes (Palette and Palette light must reach the hair, lumps and core via `tone()`), the variant's own controls (mass size, mane density and length), and Giants' cost: 5.56 ms at 0.4.28 against a budget of about 5.
- Comparison sheets and tools: `tmp/lab/l4/compare_*.png`, `tmp/tools/lion.sh`, `compare.ps1`; the photos in `tmp/ref/lions-mane/` (REFERENCE.md).

## Revisit: effects in a shared space (Greg, 2026-10-19)

ADR-0002 bypasses a layer's effect chain while the layer is in a shared space (the layers share one picture, so a per-layer 2D chain has nothing of its own to work on). Greg agreed for now and asked that it be revisited. The likely answer is a **Space effects** chain: one 2D chain over the finished space, before it meets the flat layers, reusing `render_layer`'s chain code. Decide after the jellies' diffusion and refraction land.

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
