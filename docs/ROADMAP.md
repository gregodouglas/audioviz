# Roadmap and open issues

What is still open, in rough order, with the reasoning. The history of what was built, and when, is in the phase logs (`docs/phase6-ui.md` onward) and `git log`. Items with a GitHub issue say so; mirroring lags, and this file wins.

Rewritten 2026-10-23 at 0.4.76. Done items were dropped from this file; the phase logs keep them.

## State

- **Released:** v0.1.0 (2026-10-17) is the only published release. Everything since then is code on `main`, with installers built locally when Greg wants one. A tagged 0.4.x release with notes waits for the pace to settle (Greg's call).
- **On `main`:** 30 visuals, all loaded from folders (none compiled in); 43 factory scenes; 17 effects; two directors; the audio engine (ADR-0006); the lab (ADR-0003); motion per ADR-0007.

## Hand tests owed

Mouse paths never reach egui from posted messages, so these need a person:

- Panel: drag a stack row by its hue bar, the effect chips, inline opacity, dropdowns, typing into a value, double-click reset, Esc out of an effect page.
- Scenes section: Duplicate, Rename, Delete and Restore from the row menu.
- Your own visuals: New visual, Open visuals folder, Restore factory visuals, a folder dropped in by hand (steps in `docs/phase6-ui.md`).
- Blend modes: set a layer to *over* and move it between slots 1 and 2.
- Casting (experimental): Display › Cast to a TV against the Roku, with Spotify playing on the PC.
- Window: the portrait-monitor fullscreen cut-off (not reproduced; needs a screenshot), tray menu, strip controls against Spotify, the installer wizard, Win+arrow and Snap Layouts on the portrait monitor.
- Audio capture: pause Spotify (`made up` should grow a second a second in the debug line), switch the default output on endpoint loopback, unplug the output.

## The audio engine after its first pass (ADR-0006)

Steps 0 to 5 each have a first version (0.4.30 to 0.4.52; log in `docs/phase8-audio.md`).

1. **Greg's eye on the new signals** with real music. Visuals already use bars, builds, drops and sections. Open questions: whether *In balance* should be the default for Wheel, Waves, Pulse and Eclipse, and whether the Kaleidoscope's section tumble works (it is unseen, because the lab has no album art). Factory scenes that link sliders to the new signals are still to come. Per ADR-0004: entrain, do not trigger, and check with the lab's `--run`.
2. **Tuning on real music.** In three minutes from Greg's Spotify (`tmp/live-3min.wav`, track unknown) the tempo wanders from 131 to 142 BPM: find the track and fix it.
   - Record more clips (EDM, rock/pop) and run the three measuring examples on each. S in the director view saves a session, which lets Greg catch a moment where the engine and the music disagree.
   - A four-bar section is predicted as eight when nothing earlier in the track was four bars. A fix needs real music, to weigh it against eight-bar sections predicted as four.
3. **Open items of the steps:**
   - Sections: changes of chord alone are missed, and the labels are approximate.
   - Links: choices, toggles and integer sliders cannot be linked.
   - The director view costs about 2 ms of CPU a frame while open, most of it tessellating its curves.
4. **Later, its own ADR:**
   - Song mapping from files: a WAV or a whole song analysed ahead, scenes timed to it, rendered videos.
   - Greg's studio timeline (`docs/model.md`).

## Visuals

- **Bug eyed, Real** (0.4.72-0.4.76): depth of field in Macro, depth in the skin's colour, lower lashes that read as stitches, reflections at the edge of the cornea's dome. Macro costs about 4.8 ms at 2560x1440.
- **Creatures:** a "ballet" director that lines strokes up on phrases, comb-jelly tentacle branches (tentilla), then other creatures on the creature library (eels, a fish shoal, birds).
- **Candidates:** Tunnel of spectrum rings (partly covered by Audio tunnel), Strings (chroma chords as string art), Interference (sinusoid line fields in moiré). The queued ports and their sources are in `docs/INSPIRATION.md`.
- **Effects:**
  - New: glitch (block displacement and channel tears on onsets), posterize / hue cycle, CRT, echo (a delayed copy under the picture).
  - Halftone still lacks a dither.
- **The format:** a depth test, and pictures of a visual's own. A pipeline cache across runs would cut start-up builds (wgpu's cache is Vulkan-only, and FXC re-runs every launch).

## Shared space

- **Build the variants apart.** A visual with `"space"` compiles its triangles twice, stacked and shared, in one job. The Jellyfish therefore builds in 10.0 s instead of 5.6, and a shared scene shows nothing until both are done. Build them as two jobs, the variant in use first, and prewarm the other.
- **Effects in a shared space** (Greg asked to revisit; ADR-0002 bypasses a layer's chain in a space). The likely answer is a *Space effects* chain: one 2D chain over the finished space, reusing `render_layer`'s chain code.

## Parked: the lion's mane (Greg, 2026-10-19)

ADR-0005 steps 1 to 3 are done. Step 4 (bell, mass) has had three passes and step 5 (the mane) its first part (0.4.24 to 0.4.28). For when it comes back:

- **The mass** (`lion/plume.wgsl`): brighter cream highlights (it reads orange-tan beside photo 42), and a denser veil of hair just under the bell.
- **The hair** (`lion/hair.wgsl`): ribbons through each lock that cover in the cover pass; then drop the stopgap dimming by `behind` in `lion/hair.wgsl` and `lion/mane.wgsl`. The curls kink a little at 16 links a hair.
- **Step 6:**
  - Motion: slow weak pulses, arms that bunch on the squeeze, hair that stretches and lags.
  - Palettes reaching the hair, lumps and core via `tone()`.
  - The variant's own controls.
  - Giants' cost: 5.56 ms against a budget of about 5.
- Reference: the photos in `tmp/ref/lions-mane/` (REFERENCE.md); tools in `tmp/tools/`.

## Measured default audio delay (needs Greg)

On the reference rig with wired output: press **C**, tap along for ten beats and read the offset. If it is clearly non-zero, it becomes `audio_delay_ms`'s shipped default, with a note in the Sync hint that it was measured on one rig.

## Casting (experimental)

- Current: Windows Miracast via the cast panel, which detects the new display and goes fullscreen there in one click. Roku takes no Google Cast.
- Only if mirroring disappoints: a native Roku channel fed by an encoded live stream (Media Foundation H.264 → HLS, BrightScript player). That is weeks of work, with seconds of lag to compensate. Not planned.

## Shipping (deferred by Greg)

- **Code signing:** Azure Artifact Signing is wired into `release.yml` behind `CODE_SIGNING`. It needs the account, a certificate profile and five secrets. Releases ship unsigned until then.
- **winget** submission after a signed public release. Until then, winget appears nowhere user-facing.
- **Bespoke installer** (issue #3): deferred, and Inno stays. A dark Inno skin on the app's theme tokens would take a day. A self-made installer would take a week and fare worse with SmartScreen while unsigned.
- **Settings file:** every schema change so far migrates (`config::migrate`, `Layer::normalize`, serde defaults and aliases). Keep it that way. A dev run must never overwrite `%APPDATA%\audioviz\settings.json` (`config::save` refuses under `AUDIOVIZ_DEFAULTS`).

## Later: macOS

- Core Audio process tap (macOS 14.2+) behind `AudioSource`; MediaRemote / Now Playing behind `MetadataSource`.
- `.app` bundle, notarization, Homebrew cask. The target machine is a MacBook Pro M4 Max.
- `platform/other.rs` already stubs the Windows-only entry points (open URL, launch Spotify, battery, update check, cast), so the macOS build compiles. CI builds it with null audio and metadata backends.
