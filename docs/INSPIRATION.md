# Inspiration and credits

audioviz borrows ideas, never code, unless a licence says otherwise and the entry below says what was taken. Every visual or modifier that follows someone else's work names them here and in its own doc comment. Keep this file current when a port lands.

## Sources

- **Uberviz** by Felix Turner (Airtight Interactive), https://uberviz.io (Lantern, Pareidolia, BBNG, Word Problems, Splice, Loop, Nero). **No published licence**; the only GitHub copy is a third-party mirror that grants nothing. Treated as inspiration only: nothing copied, every shader and the visual logic written fresh from a description of the effect. Credit line: *Inspired by Uberviz by Felix Turner / Airtight Interactive, https://uberviz.io. No code used.* Two of its passes (the dot-screen halftone and colorify) are stock three.js examples by alteredq under the MIT licence; ours are written fresh regardless.
- **audible-visuals** ("Visualizer Experiments") by Sonia Boller, https://github.com/soniaboller/audible-visuals, **Apache License 2.0**. Ideas may be ported; if code is ever ported, keep the licence text and a NOTICE stating the changes. Credit line: *After audible-visuals by Sonia Boller, https://github.com/soniaboller/audible-visuals, Apache License 2.0.*

## What is in audioviz

| In audioviz | After | Author | Licence | What was taken |
|---|---|---|---|---|
| Halftone modifier | the halftone pass in Uberviz (alteredq's dot screen) | Felix Turner; alteredq | none / MIT | the idea: a rotated dot screen that drifts between the picture's colours and one ink. Written fresh: luma-weighted, sized marks, three mark shapes, palette ink. |
| Crystal visual | the Crystal visualizer in Uberviz (Lantern) | Felix Turner | none | the idea: translucent additive tetrahedra tumbling as a cluster, loudness-scaled, one shard re-posed per beat, a flung-apart mode. Written fresh: palette facets, glowing edges, eased poses, bar-phase scatter. |
| Scanlines modifier | the "Lines" pass in Uberviz | Felix Turner | none | the idea: a sine stripe over the frame. Written fresh: pitch, angle, cut and glow, crawling with the music. |
| Wheel visual | the Wheel in Uberviz (Lantern) | Felix Turner | none | the idea: a disc of spectrum wedges, brightness by level squared, hue round the wheel, beat jumps and a walking band mapping. Written fresh: rays past the rim, soft edges, centre hole, palette span. |
| Waves visual | the stacked-waveform landscape made famous by the *Unknown Pleasures* cover (Peter Saville, after a pulsar plot by Harold Craft) | | | the look only |

## Queued ports (from the 2026-10-18 research), with what each takes

- **Loop** (Uberviz): sixty concentric waveform rings, each lagging the previous frame's, lightness by level.
- **Eclipse** (Uberviz Lantern): sixteen glow dots on a ring reading the spectrum, the disc masked black.
- **Ripples** (Uberviz BBNG / Lantern): a noise-displaced plane printed with contour lines or dots, depth-darkened.
- **Spiral family, Sphere (urchin), Waveform stack** (audible-visuals): parametric spirals with a slow parameter drift displaced by the waveform; radial spikes banded by magnitude.
- **Beat roll** (Uberviz autoMode): per beat, half the time nothing, otherwise exactly one of wobble, strobe, colorify, halftone or audio-fade; the colour/monochrome "animation" Greg noticed.
