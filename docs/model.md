# The model: Space, Scene, Layer, Visual, Effect, Picture, and the directors

Written 2026-10-18 with Greg, to settle the words before the directors grew. Every doc and every label in the panel uses these terms. The code keeps a few older type names (`Modifier` for an effect, `auto_scenes` for the scene director's Auto mode); this file is the map between them.

## The hierarchy

1. **Space.** The app and its one picture: the window, the display and sync settings, the scene list, and the **scene director**. Nothing is drawn at this level; it holds the scene that is showing and decides when to change it.
2. **Scene.** A named composition, saved as a file: up to three **layer** slots, the **Picture** settings, and the scene tint. Scenes are what `Space` cycles, what the scene director picks, and what the Scenes section lists. Built-in scenes ship with the app; user scenes are JSON files.
3. **Layer.** One instance of a **visual** with its own reacts-to band, tint mode, palette, blend mode, opacity, an ordered chain of up to eight **effects**, and an **effects director**. Slot 1 is drawn on top. The same visual may be used in more than one layer.
4. **Visual.** The generator inside a layer: Phosphor scope, Flow-field ribbons, Black hole, Kaleidoscope, Sigil, Waves, Crystal, Spectrum wheel. A visual's parameters belong to the layer, so two layers of the same visual differ. A **preset** is a named set of one visual's parameters; applying one touches nothing else on the layer. Presets are files in the audioviz folder, the shipped ones included (written there on first run from copies inside the app), so a shipped preset is edited or replaced like any other; *Restore factory presets* writes the shipped copies back. Scenes work the same way: a saved scene is the whole composition, and the shipped scenes are files beside yours.
5. **Effect.** One pass over the layer's picture, run in chain order, each with its own band, mix (wet/dry) and parameters: Kaleidoscope (fold), Fractal, Luminescence, Feedback, Polar, Ripple, Edge, Chroma split, Mask, Palette (grade), Halftone, Warp, Scanlines, Mirror, Glow, Vignette, Colorify. Effects are engine-level and visual-agnostic: any effect goes on any layer, in any order. In the code an effect is a `Modifier`.
6. **Picture.** The one pass over the composited scene: exposure, auto exposure, bloom, grain, saturation, look (Film or Neon), reduce flashing. Saved with the scene. In the code this is the post stack.

What is *not* a level: the effects are not a second layer of the scene. They live inside a layer, between its visual and the composite. The only scene-wide pass is the Picture.

## The directors

Two directors, one per level that has choices to make, with the same three modes:

- **Manual.** Off. Nothing moves unless you move it.
- **Auto.** The system decides, from the music: beats, bars, onsets, drops, quiet passages, track changes.
- **Cue.** A saved sequence for one song, authored in advance: what an artist would ship with a track. Reserved in the settings today; it arrives with the studio timeline (below).

**Scene director** (Space level, the `A` key, Scenes section). In Auto it changes scene on a track change, a drop, a long quiet passage, or a dwell limit, fading layer by layer. Its state is saved with the settings.

**Effects director** (Layer level, the Effects tab). In Auto it rolls on a cadence of beats: half the rolls change nothing, the others pick which of the layer's **pool** effects are live and fade the rest out over a set time. An effect marked **Always on** is pinned: it stays out of the pool and never fades. Rolls are seeded by the track and the roll count, so a song replays the same way and two layers never roll in lockstep. A scene change rolls every director that is on. Its settings (mode, cadence, live count, sit-out chance, fade) are part of the layer, so they save with the scene.

Why a director lives at the level it does: the scene director chooses *between compositions*; the effects director chooses *within one layer's chain*. The Picture has no director yet; if one comes it would roll look, bloom and saturation the same way.

## Later: the studio timeline

Greg's larger idea, not started: import a song, and on a timeline create the visualization across tracks and transitions, controlling scenes, layers, effects and parameters over the length of the track. The Cue mode of both directors is the playback side of that: a timeline exports a cue list, and a cue list plays back through the directors. Everything above is designed so that a cue only ever sets the same things a person can set by hand.
