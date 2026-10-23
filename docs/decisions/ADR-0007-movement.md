# ADR-0007 — Movement: a Motion setting on every layer, nothing jumps

- **Status:** Accepted 2026-10-23 (Greg: "some are the best with these shift and sways and are better suited to color shifts or drawing shifts vs the wobble or sheering shifts that are jarring ... we need work on the movement system in regards to the beat and groove, and not really have our visuals do jarring moves (but some this is the point so its a fine line)"; the Spiral was his case in point). Done in 0.4.66.
- **Context:** ADR-0004 set the rules for the Jellyfish (entrain, every level eases, nothing jolts), and the Jellyfish had its own Music setting (Natural, Reactive, Performance). The other visuals were each wired their own way. An audit of all 29 (2026-10-23) found four ways they jarred:
  1. **The music nudged a value that something multiplies up.** The Spiral's figure is its angle step times each point's index (up to 4096): energy sped up the step's drift and each section glided it by up to 12°, so a change too small to see in the step turned the outer points by degrees, and the figure sheared and churned. The Harmonograph's pitch interval is multiplied along about 250 radians of its curve, so each chord change swept the whole figure.
  2. **Beats stepped motion in one frame.** `kick = 1.0` on the beat, then a decay: the Hallway's and Tunnel's speed jumped 2.5 times, the Spiral's and Loop's waveform push and the Pulse's rings jumped, the Kaleidoscope's spin lurched on onsets and sections, the Wheel snapped to a random angle.
  3. **Raw, jittery signals drove shape.** `V.bass` scaled the Ribbons' whole flow field every frame (every path warped) and `V.onset` lurched its speed; the Kaleidoscope zoomed with raw bass; the Crystal jumped in scale on onsets.
  4. **Speeds followed the raw energy.** About fourteen visuals ran their clocks at `0.5 + 0.5 * V.energy`, and so did `V.flow`: integrated, so nothing jumped, but the motion stuttered.
- **Reads:** ADR-0004, `docs/visuals.md`, `docs/building-visuals.md` ("Reacting to the music").

## Decisions

### 1. Two kinds of reaction

A visual answers the music with **light** (brightness, flashes, colour, palette moves, line width) and with **motion** (sways, kicks, warps, surges of speed, turns, displacement, and the camera). Light always answers. Motion answers as much as the layer says.

### 2. Motion is a layer setting

Every layer has **Motion**: Still, Gentle, Groove or Wild, beside Blend in the Visual tab. The visual's manifest says which suits it (`"motion"` in visual.json, Groove by default; the Spiral and the Harmonograph say Gentle); a layer that does not choose follows that, marked "(suits it)" in the list. Shaders read it as `motion()`: 0, 0.35, 1 or 1.7, and multiply every motion reaction by it. At Still a visual moves at its own pace and only its light answers the music. The Jellyfish's Music setting became it (Still is Natural, Groove Reactive, Wild Performance; Gentle is Reactive at a third), so it is one control everywhere. The lab takes `--motion`.

### 3. Values that cannot jump

The engine works out, per layer, what motion should use, so no visual has to keep its own:

- `kick()`: the beat as a kick that rises over 40 ms (a smoothstep), dies away over about a third of a second, as strong as the beat is sure; `bar_kick()` the same on the bar's first beat; `since_beat()`; `kick_shape(since, decay)` for events a visual keeps itself.
- `pace()`: the music's pace eased over half a second, scaled by Motion (1 at Still). `V.flow` now runs at the eased pace too.
- `bass_soft()` (60 ms up, 300 down) and `onset_soft()` (40 up, 250 down).

`V.beat`, `V.onset`, `V.bass` and `V.energy` stay as they are, for light.

### 4. The rules

1. Nothing moves in one frame: motion rises over 40 ms or more.
2. Raw levels never drive shape; use the eased values above.
3. The music never nudges a value that something else multiplies up (an angle step times an index, a frequency ratio times a long phase). There, it answers with light, or changes at a musical boundary (a bar line) and glides.
4. Every motion reaction is multiplied by `motion()`.
5. A snap that is a visual's character (the Wheel's random turn on a beat, the Crystal's shards re-posing) stays at Groove and Wild and becomes a glide below (Greg, 2026-10-23).

`--run` in the lab is still the test (ADR-0004).

## Consequences

- The Spiral's drift is steady; a beat sends a pulse of light out along its line and each section moves its colours; its waveform push, the beat's part in it and its sway follow Motion.
- The Harmonograph takes a new interval only at a bar line and glides to it more slowly at lower Motion.
- Every factory visual's motion reactions are scaled by `motion()`, and the jumps listed above use the eased values.
- A visual written before this still compiles and keeps its motion at every setting until it calls `motion()`. That is the one thing to add to a visual of your own.
