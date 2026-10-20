# ADR-0005 — Variants, covering, and the lion's mane

- **Status:** Accepted 2026-10-19. Greg settled the shape ("two variants, keep it general") and answered the questions (at the end). Step 1 (variants) done in 0.4.21, step 2 (the Jellyfish in two variants) in 0.4.22, step 3 (covering by depth) in 0.4.23; the lion's mane of 0.4.20 stands in until step 4.
- **Context:** Greg, on the reworked lion's mane (0.4.19 and the round after it): "this is the one that is the most off from what's real, essentially this jelly is largely NOT going to be see through, just a few places unlike the others. This is going probably need a refactor as it's different enough to warrant different design"; earlier, "the center mass is huge" and "lion's mane should get its own render of that central mass that is bulky, large, and not really see through". On a separate "Lion's mane" visual: "I don't like the idea of having a visual for jellyfish, then a Lions Mane visual". He wants it under Jellyfish, with its own budget and its own components for rendering.
- **The problem:** a visual is one compiled program with one fixed budget (items, triangles, line points) and one set of passes, and a choice param such as Species is only a value its shader branches on. So every species pays the largest one's budget, carries the others' code, and gets the same passes. Every jelly is also built on one model, glass that gives off light. Nothing covers anything, and the tentacles draw last over everything (ADR-0002, "everything they draw is light"). That model is right for the moon jelly and wrong for this animal. The last round faked covering within it: alpha on the bell, the curtains and a closed mass, a far-to-near order per jelly, and dimming for hair beyond the body. It got closer, but hair still draws over the body that should hide it, and every species pays for the lion's triangles.
- **Reads:** ADR-0002 (the shared space; its surface depth was designed in for the first visual that covers), ADR-0003 (the lab), ADR-0004 (living motion), `docs/visuals.md`.

## What the animal looks like (the brief)

From a sourced research brief (2026-10-19; Wikipedia, Walla Walla University's invertebrate guide, MARLIN, VIMS, Aquarium of the Pacific, the Naturalis zooplankton key, SeaLifeBase). Anything marked *inference* was not sourced and is to be checked against reference photos.

- **Bell:** a flat saucer, thick in the middle and thin at the margin, smooth on top; 30 to 60 cm typical, up to 2 m. It flattens on the stroke until its lobes stand out as an eight-pointed star. The margin has **16 lobes in 8 pairs**: deep clefts between the pairs, a shallow notch between the two lobes of a pair, and a rhopalium on a small flap in each. (Ours had 8 lobes of four painted lappets, which no source supports.)
- **Colour and cover:** the pigment is in the tissue. Small animals are tan, yellowish or light orange and darken with size to brick red, rose, violet and purple; pale milky ones also occur. Inside, sixteen separate stomach pouches (8 rhopalar, 8 tentacular) and curved canals. Which parts are see-through is *inference*: the thick middle and the mass solid and dark; the thin margin, the lobe edges and the tentacle tips the few places light shows through.
- **The central mass:** **four oral arms** (thick, much folded and frilled, about as long as the bell is wide, bunched into a blocky mass when contracted, longer when relaxed) joined by a folded membranous curtain about half their length, with **four ribbon-like, very folded gonads** hanging between them: about eight folded masses round the mouth, darker than the bell (reddish-orange, purplish-brown, violet). It fills most of the middle of the underside (*inference*).
- **The mane:** **8 clusters of up to 150 tentacles, about 1,000 in all**, each cluster in 4 rows on a horseshoe-shaped patch of the underside, set in from the margin under the clefts. Thin as hair, yellow, yellow-brown or red, highly extensible, from 1 to 4 bell widths in a typical animal (much longer in big ones). Together they trail as a dense net, a tangled mass rather than separate threads.
- **Motion:** slow, weak pulses (*inference*: a few seconds each); mostly carried by the water. The arms lengthen as the bell relaxes and bunch as it squeezes; the tentacles stretch and lag.
- **Silhouettes** (*inference*): from the side, a flat reddish disc over a dark, heavy, frilled bulge, over a yellow-brown cloud of hair; from below, a star round a dense core with the hair clusters round it; backlit, the rim, lobes and hair glow while the middle stays dark.
- **Checked against 42 photos** (Wikimedia Commons, 2026-10-19; `tmp/ref/lions-mane/`, with `SOURCES.md` and `REFERENCE.md`): (1) the bell is not solid all over: the middle, about two thirds of the radius, is dense and coloured, and the outer third, the lobes, is a broad band of clear, milky, bluish-white jelly you can see through; (2) the middle shows a web of polygon canals, not radial stripes; (3) the mass sits right under the bell, about as wide as it, a solid block of folded curtains, from cream to orange, red or purple; (4) the hair is paler than the bell (cream, white or pinkish), a dense veil near the bell merging into the mass, opening into a long loose net, glowing in dark water; (5) the colours: amber and orange when small, pink to red when large.

## Decisions

### 1. Variants: one visual, a program per variant (general, in the format)

A visual can have **variants**: different builds of itself, picked by one of its choice params. It is still one visual in the picker, the panel, scenes and presets.

- **Manifest:** `"variants": { "by": "<choice param id>", "list": ["<folder>", ...] }` in `visual.json`. Each listed folder holds a `variant.json` and the variant's own `.wgsl` parts.
- **`variant.json`:** `"when"` (the values of the `by` param it serves), `"parts"` (its files, after the visual's own), `"items"`, `"triangles"`, `"line"` (its own budget), `"layers"` (its design layers), `"covers"` (decision 2), and `"params"` (controls only it has: shown in the panel only while it is picked, stored with the layer's other values, and counted with the visual's toward the limit of 32 per program).
- **One program per variant:** the visual's parts plus the variant's parts, compiled and cached on their own as any visual is, with buffers and draw counts sized to that variant's budget. The visual's shared parts say what they need from a variant (its layout constants and entry points). WGSL declarations can come in any order, so the shared files call what the variant defines.
- **Switching:** changing the `by` param to a value another variant serves swaps programs as changing a layer's visual does. The old one keeps drawing until the new one is built, which is cached after the first build on Vulkan. Prewarm builds the variants the scenes use.
- **The lab** picks the variant from `--set <by>=<value>`, shows that variant's design layers, and reloads on a save to any of its files.
- **Reload, factory files, history, `Restore factory visuals`:** variant folders are part of the visual's folder and go wherever it goes.
- A visual without `"variants"` is unchanged.

**The Jellyfish becomes two variants.** The shared files keep the swarm, swimming and steering, the current, the camera and close-up, the tank, plankton and water, and the controls every species has. The two variants are:
- **`glass`:** moon, sea nettle, crystal, atolla, comb. The bells, arms and tentacles as now, with its budget back to what those species need.
- **`lion`:** the lion's mane, decisions 2 and 3.

A species can be split out of `glass` later if it outgrows the model.

### 2. Covering, in the engine (ADR-0002's surface depth, now)

A variant (or a visual) can **cover**: `"covers": true`.
- The shared space gets a depth attachment (Depth32Float, 4x, reversed Z, about 59 MB at 1440p). *As built: always there in a shared space, since every pipeline in the pass must name a depth format; it cost nothing measurable. A stacked layer gets none: stacked, a covering visual draws its solid parts with its light (`cover_pass()` is false).*
- A covering program's triangles draw in **two passes**: first the parts that cover, writing depth, then its light (glow, translucent edges), testing depth without writing it. `covering()` in the API says which pass is running, so `vertex` leaves out the other pass's triangles.
- Every other member's triangles, every line and `shade` test depth and never write it. The body then hides the hair and the other jellies behind it, and whatever is in front still shows.
- The covering pass draws first, so the order within the light pass still does not matter (light adds).
- The glass variant does not cover, and pays nothing.
- The fakes from the last round go: the per-jelly draw order, the alpha on the bell, and the dimming of hair beyond the body.

### 3. The lion's mane, designed again from the brief

- **Material:** `tissue`, in the `luminous` library beside `glass`. It is opaque, pigmented colour, lit from above and round its folds (wrap lighting, the folds' hollows darker), with a little light scattered under the surface where it is thin (the margin, the lobe edges, the frills' rims), which is where the animal shows light through. Its own light is kept to the few places the animal has it.
- **Bell:** a flat, thick saucer; 16 lobes in 8 pairs, modelled (deep clefts, shallow notches), not painted; it flattens toward a star on the stroke. Solid, coloured tissue over the middle two thirds, with a polygon web of canals; fading into the lobes' broad band of clear, milky, bluish glass, which does not cover.
- **The mass:** eight folded bodies round the mouth, four arms and four gonads alternating. Each is a closed column of tissue with big frills (finer ones where the mesh can carry them), and the arms are joined by a curtain over their upper half. About a bell width long, longer as the bell relaxes, bunching as it squeezes; darker than the bell. It covers.
- **The mane:** about 1,000 hairs in 8 horseshoe clusters of 4 rows under the clefts, swaying in locks (as now). As a mass it covers: ribbons through each lock drawn in the covering pass, so hair behind the body, and behind the hair in front, is hidden. The line hairs draw the fine strands on top, in the light pass.
- **Motion:** slower, weaker pulses than the others, mostly drifting; arms and hair stretch and lag (ADR-0004 throughout).
- **Controls of its own** (the variant's params): mass size, mane density and length, how see-through, colour.

### 4. Iterated in the lab

Designed in the lab from reference photos, part by part (the variant's `"layers"`: bell, margin, mass, curtain, mane, hair, glow), and measured with `--groove` and `--run` (ADR-0004) and the app's timing at 1440p.

## Increments

0. **Reference.** A few reference photos (in `tmp/ref/lions-mane/`, out of the repo unless their licence allows), read in the lab sessions; the brief's inferences checked against them.
1. **Variants in the engine and the format.** Parsing, a program per variant, budgets, switching, prewarm, variant params in the panel, the lab, reload, factory history; `docs/visuals.md`.
2. **The Jellyfish split** into shared files, `glass` and `lion` (the lion as in 0.4.20, moved as it is). No visible change. Measured: the glass species cheaper than at 0.4.19 or the same, never dearer.
3. **Covering in the engine.** The depth attachment, `"covers"`, the two passes, `covering()`, every pipeline in a space testing depth. Measured: no change where nothing covers.
4. **The lion's mane: tissue, the bell and the mass**, covering, in the lab against the photos.
5. **The mane:** about 1,000 hairs, the horseshoe clusters, ribbons that cover and lines on top.
6. **Motion, colour, its controls; Giants checked.** Docs and the phase log current at each step.

## Costs (estimates, to be measured)

- Variants: nothing at run time. A first build per variant used (about 3 s on Vulkan, then cached).
- Depth attachment: memory as above. About 0.1 to 0.2 ms for clearing and testing, and only while something covers.
- The lion's mane: a few thousand more triangles and 1,000 hairs of 16 links per animal. The hair is the largest part: up to 4 animals x 16,000 line points. The target is Giants no dearer than at 0.4.19 (about 4.5 ms at 1440p; 0.4.20's stopgap costs about 0.8 ms more).
- The glass species: back to their cost before the last round, or less (the lion's code and budget leave their program).

## Questions, answered (Greg, 2026-10-19)

1. **Reference photos:** "go find some photos for reference". Collected in `tmp/ref/lions-mane/` with a list of their sources and licences.
2. **Colour:** "color seems mostly right [to] what I see in photos", so the natural colours stay as in 0.4.20. Every new part (the tissue, the mass, the mane's ribbons) is wired to the palettes and the Colours modes like the rest (`tone()`, Palette, Palette light), so palettes reach everything.
3. **The round in the working tree:** committed as 0.4.20 ("current lions mane can stay until new refactor"); it is the `lion` variant's starting point in step 2.
