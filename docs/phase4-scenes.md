# Phase 4 — scene system, scenes 2 and 3, metadata, auto director

Status: **landed 2026-10-17.** Three scenes, cross-fades, Windows now-playing metadata, a Swiss-grid type overlay, and an auto director with musical cues.

## Scene system

- `scene.rs`: `Scene` trait (`render` into the scene's own linear-HDR texture, `output`, `resize`, `reload_shaders`, `ui`) plus shared helpers (texture/bind-group/pipeline builders, additive blend, guarded shader builds).
- `director.rs`: owns the scenes, cross-fades two outputs through `mix.wgsl` (smoothstep, 4 s) into the post stack's input, and runs auto mode.
- `post.rs`: the Phase 3 stack (bloom, auto-exposure, AgX, grain, dither) now consumes the composite. Grain scales with brightness so black stays black; bloom levels attenuate 0.6× per step so glow hugs the lines. These two fixed the background shimmer Greg reported.
- Keys: 1-4 select a scene, Space next, A toggles auto, T toggles the title overlay. The last scene is remembered in settings; `AUDIOVIZ_SCENE=n` overrides at launch (dev).

## Scene 2 — flow-field ribbons (`scenes/ribbons.rs`)

GPU particles (default 12 000 of a 65 536 buffer) advected by a compute shader through a divergence-free curl-noise field plus twelve vortices placed on a circle of fifths, each vortex's strength set by that pitch class's chroma. Chords therefore steer the flow; adjacent vortices spin in opposite directions so they shear. Onsets boost speed, slow bass scales the field. Each particle's step is drawn as an anti-aliased segment into a persistence buffer (shared `decay.wgsl`), colored by pitch-class hue pulled toward the key hue over the tint base, at low saturation. Sliders: particles, speed, field scale, curl, chroma vortices, line width, intensity, persistence, saturation.

## Scene 3 — lensed starfield (`scenes/starfield.rs`)

A fullscreen shader renders a hash-grid star field (three scales, color temperature per star) and a geodesic grid in the *source plane*, seen through a point-mass gravitational lens. The Einstein radius follows the slow bass; low-order multipoles from the spectrum wobble the ring; magnification clamps at 14× so stars stretch into arcs near the ring; the horizon is black; a photon-ring glow breathes with the beat; onsets twinkle. The source plane drifts and rotates slowly so it never freezes.

## Metadata (`crates/metadata`)

`MetadataSource` trait; Windows backend polls the System Media Transport Controls once a second, prefers the session whose app id contains "spotify", reads title/artist/album/playback state/timeline, and fetches + decodes the thumbnail (JPEG/PNG via `image`) only when the track changes. No API keys, no network. Verified live: "Purity Ring — begin again [another eternity]" with art on the first run. macOS backend pending (AppleScript or Web API).

## Type overlay (`overlay.rs`)

Drawn with egui on the background layer so it stays crisp under the bloom and under the debug panel. Loads Segoe UI / Arial (or Helvetica on macOS) from the system as the grotesk. Twelve-column grid with 6% margins: bold title (max 2 lines, 8 columns) bottom-left, artist in caps with album, a hairline rule across the grid whose weight pulses with the beat and fills in the accent color up to the track position, status and time top-left in small caps, album art two columns wide bottom-right with a column rule. Shown on the starfield scene (and during fades into it); toggle with T or the checkbox.

## Auto director

Off by default (checkbox / A). Cues, evaluated every frame: **track change** (metadata id changed, ≥30 s since last switch), **drop** (energy trend was strongly rising, then collapses; ≥90 s), **quiet** (≥6 s of quiet/silence; ≥90 s), **dwell limit** (6 min). Next scene is random, excluding the current and previous. The panel shows the last cue. Not yet exercised over a long listening session.

## Known tuning items

- Ribbon vortex centers show as dark "eyes" because steps longer than 60 px are culled as respawn jumps; arguably a feature.
- Starfield grid is very faint by default; raise "grid lines" to see the bending.
- The overlay's egui text has no letter-spacing control, so the caps line is set plain.
- Scene-specific post settings (e.g. less bloom for the starfield) are not yet separate.
