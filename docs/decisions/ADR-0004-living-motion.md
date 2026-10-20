# ADR-0004 — Living motion: the jellies groove, nothing jolts

- **Status:** Accepted 2026-10-19 (Greg: "the goal of the jellies is to groove to the music, so mechanics, lights, colours need to be wired"; "this is living ... we can't do jarring animations, so we need a smoothness to motion"). Done for the Jellyfish in 0.4.14.
- **Context:** The jellies answered the music, but much of it was wired as triggers: a stroke could restart while the bell was still springing back (the shape snapped in one frame), flashes rose in one frame, palette hues stepped on each beat, and a new flash or stroke cut off the light of the last one. Some reacted hardly at all (the comb jelly's light swung no more with music than without). Two geometric frames could also flip in one frame (the jelly's frame round its axis when tipped far over, an arm's or tentacle's side when it swung into line with the bell). The lab could not show any of it: it ran in silence and held one frame.
- **Reads:** ADR-0003 (the lab), `docs/visuals.md`.

## Decisions

### 1. Entrain, do not trigger

Music pulls the jellies' own rhythms into step; it does not start things. A stroke still begins on a beat once the jelly is ready (Reactive, Performance) or on a drop, but it *takes over* from the last: the bell crossfades from the old stroke, still playing out, to the new over a quarter second (`stroke_handover`), and so does everything that reads the stroke (its light on the bell and down the tentacles, the plankton's spark). Its strength swells with the smoothed bass.

### 2. Every level eases

Lights rise in 60 to 120 ms and fall over 0.3 to 0.5 s (`ease_to`). Flashes that are set off whole (the crystal jelly's, the atolla's alarm) bloom in over a fifth of a second or more (`flash_of`) and may only start again once the last has all but faded, so a new one never cuts the old one's light. Colours glide (the palette hue moves faster on a beat instead of stepping).

### 3. One groove clock for everything that waves

What waves (tentacle sway, fringe curl, arm billow and frills, the margin's ripple, the comb jelly's wobble) runs on a clock that quickens with the music's energy, by Music, added up frame by frame (`GROOVE`), so a change of pace never makes a phase jump. The water's current and the camera keep plain time.

### 4. Frames are carried, not rebuilt

A frame worked out afresh each frame from a fixed reference has a direction where it flips. The jelly's frame round its axis is carried as the axis turns (kept as the spin from `perpendicular()`'s reference). A tentacle sways along the direction round the margin, and an arm's opening direction is the sum of two estimates that fail in different places.

### 5. Measured, in the lab

`--groove <bpm>` plays a made-up, repeatable groove (kick, snare, hi-hats, a swell of the mids, a drop every sixteen bars); `--run <s>` keeps time running and measures each frame's change from the last and the frame's light. A jolt is a single frame well above its neighbours (or a one-frame dip in the light); fast motion is a run of large steps. `--palette <name>` picks the layer's palette. The acceptance for a living visual: no single-frame spikes over about 4x the median, no one-frame light dips, in Natural, Reactive and Performance, across a drop, alone and several together; and its light swinging visibly more with music than in silence.

## Consequences

- Any visual of living things follows the same rules; the lab's `--run` is the test.
- Palettes reach everything, and a palette's dark end is lifted (`lifted`) so no jelly vanishes on it.
