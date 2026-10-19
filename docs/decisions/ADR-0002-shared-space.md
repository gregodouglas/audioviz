# ADR-0002 — A shared space: layers in one scene, with depth, refraction and diffusion

- **Status:** Accepted 2026-10-19 (Greg answered the four questions; see the end). Building in increments.
- **Context:** Greg, 2026-10-19: the basic problem with layers is that they are additive; expand the engine so layers can interact when that is turned on. Ribbons should move through, in and around the jellies' space instead of being a picture in front or behind. Later there will be collidable objects across layers (not now: no physics), but now the basics, so shaders work on the whole scene: layering, and the diffusion and refraction of light from the other things in it. For the jellies he picked **diffusion** (their own light blurred through the body) and **refraction** (what is behind bends through the bell), then **depth** (thickness and depth of field), then shaded tentacle lines. Real jellies are nearly index-matched, so refraction stays subtle. The moon jelly benefits first.
- **Reads:** `docs/model.md` (the words), `docs/visuals.md` (the format), `crates/app/src/director.rs`, `crates/app/src/shader_visual.rs`, `crates/app/shaders/library/`, `assets/visuals/jellyfish/`.

## How a frame works today

For each layer, bottom to top (`Director::render`, `render_layer`):

1. The visual draws into its own HDR texture (`ShaderVisual::render`): prepare (compute), line points (compute), then `shade` and the triangles in one 4x multisampled pass resolved into the output (`antialias`), then the line, added on top without multisampling.
2. The layer's effect chain runs over that texture (2D passes).
3. The layer meter measures it (1/8 downsample, then the lit mean) and sets a levelling gain.
4. `layer_blend.wgsl` scales it by gain and opacity and meets the composite so far by its blend mode (add, over, screen, multiply).

Then the Picture (`post.rs`: bloom, exposure, tonemap) runs once over the composite.

Four things about the jellies decide most of this design:

- **Everything they draw is light.** `paint` returns alpha 0 for every part (bells, arms, glow sprites) and the tentacles are an additive line. Adding light does not depend on order, so in a shared space the jellies need no sorting and no depth buffer to come out right. Depth only matters to them for **focus** (depth of field) and for **refraction** (bend only what is really behind).
- **The camera is per layer, worked out in the shader.** `the_camera()` builds a `Camera` (in the `creatures` library) from `V.time` and the layer's own Camera drift and View; Close-up follows item 3, which the prepare step updates on the GPU. Two jelly layers with the same settings already agree (the drift depends only on time), but nothing makes them, and Close-up layers never do (`V.instance` sets the orbit).
- **The tank is already scene-sized.** `tank()` comes from `V.resolution`, so every layer's tank is the same box.
- **Each antialiased layer owns a 4x RGBA16F target**, 118 MB at 2560x1440. Deep sea's three jelly layers hold 354 MB of multisample memory between them.

## Decisions to make

### 1. Turning it on

**Recommendation: a scene setting, "Shared space", off by default.** It is saved in the scene file as `"space": "shared"`, and a missing field means `"stack"`, so every tuned scene and every user scene draws exactly as it does today. No factory scene changes until Greg picks which ones should switch.

Which layers join: a layer joins the space when its visual declares `"space": true` in `visual.json`. That flag means the visual reads the scene camera and gives its triangles a real perspective `w`; the jellyfish already do. Every other layer stays a flat 2D picture, and **slot order says where it goes:**

- A flat layer **below** the space becomes its **backdrop**: drawn first, at the far plane, so it is bent through the bells and goes out of focus with the water.
- A flat layer **above** the space is composited over it as today (a scope over the jellies, say).

This adds no new control to each layer, and nothing new goes in the panel column beyond one toggle. A depth slider for a backdrop ("how far behind") can come later if a scene needs one.

*Alternatives:* a per-layer "Space: In it / Behind / In front" control. It is more explicit but it is a new control on every layer, and slot order already says the same thing. Or always on, which would change every tuned scene, so no.

### 2. One camera

**Recommendation: the lead layer publishes the camera on the GPU, and every member reads it.**

- `Camera` moves from the `creatures` library into the core API (`visual_api.wgsl`); the helpers that use it stay in `creatures`.
- A visual in the space may define `fn camera() -> Camera`. The engine generates a one-thread compute entry that writes it into a small **scene buffer**, which every member's bind group exposes as `scene_camera()`.
- The **lead** is the topmost member whose visual defines `camera()`. Its prepare step and camera entry run first each frame; the other members read this frame's camera. The lead's own prepare reads last frame's camera, one frame late, which nobody will see.
- In the jellyfish, `the_camera()` becomes "the scene camera when shared, else my own", so the code stays one path. Close-up keeps working, because the lead's prepare step already tracks its hero on the GPU.
- The lead's Camera drift and View drive the scene. The other members show those controls greyed out with "Layer 1 leads the camera".

*Alternatives:*
- **A camera on the CPU**, with the director computing yaw, pitch, distance and look from scene settings. This needs new scene controls, and it duplicates `drifting_camera` in Rust. It also cannot follow a jelly, because only the GPU knows where the hero is.
- **Each visual keeps its own camera** and the cameras agree by convention. This is today's state: it breaks as soon as two layers differ in one setting, and Close-up never agrees.

### 3. Depth across layers

There are two kinds of depth, and they need different machinery:

- **Light depth**: where the light in a pixel comes from. Depth of field and the refraction mask need it. Emissive things such as jellies, tentacles and plankton have it but must not hide what is behind them.
- **Surface depth**: a z-buffer. Only things that **cover** need it (alpha toward 1): later the collidable objects, a solid shard or a rock.

**Recommendation: build light depth now and design surface depth in, but add it with the first visual that covers.**

- **Light depth** goes in the alpha of a second render target, the **light target** (decision 5), which uses an alpha **Min** blend: each pixel keeps the nearest view depth anything drew there. The rgb of that target is the diffused light. Every pipeline in the space writes it:
  - triangles: the generated `fs_mesh` writes the vertex's view depth (`pos.w`, carried through);
  - lines: `LinePoint` gains a `depth`, stored in the points buffer's spare fourth component;
  - `shade`: writes the far plane unless it calls the new `write_depth(z)` (marine snow could, for its nearest layer).
  - A fragment that adds no light writes the far plane, so a black corner of a quad does not pull the focus forward.
- **Surface depth**: the generated `vs_mesh` already gets view depth in `w`, so in a shared space it writes a proper `clip.z` (reversed Z, near 0.05, far 50). It costs nothing until there is a depth attachment. The attachment (Depth32Float, 4x, 59 MB at 1440p) and a manifest `"covers": true` (depth write on that visual's triangles) arrive together with the first covering visual.

*Alternative:* a z-buffer from day one, with every bell writing depth. That is wrong for the jellies: the outer skin would hide the inner skin, and the gonads and canals live on the inner skin. Emissive bodies must test against depth, never write it.

### 4. Order

- **Light is added, so it needs no order.** That covers everything in the space today.
- **Covering, when it comes**, writes surface depth. Premultiplied over (the triangles' blend) then does the right thing either way round: light drawn first behind a cover is darkened by `1 - a`, and light drawn after is rejected by the depth test.
- **The one case left is partial covering across layers** (0 < a < 1 in two layers overlapping). It falls back to layer order, and within one visual to that visual's own order, as Crystal sorts its shards today. This is a documented limit.

| | What it handles | Cost at 1440p | When |
|---|---|---|---|
| Light adds, covering writes depth | everything we have and the solid objects coming | about nothing | now (covering when needed) |
| Weighted blended OIT (McGuire and Bavoil 2013) | partial covering in any order, approximately | two more 4x targets (accum RGBA16F, revealage R8), a resolve, roughly +0.3 to 0.5 ms | only if a visual needs semi-opaque glass across layers |
| Sorted draws across layers | exact partial covering | merging every layer's triangles into one sorted list: a rewrite of how visuals draw | no |
| Depth pre-pass | overdraw of expensive covering shaders | every covering mesh twice | no: our cost is light, not overdraw |

### 5. Scene-wide passes

The space draws into **one** scene target pair: the **colour target** (4x RGBA16F, as today's per-visual one) and the **light target** (4x RGBA16F: rgb additive glow, alpha Min light depth). Each pass is optional and costs nothing when no member uses it.

**Diffusion: a body's own light, blurred through it.**
- `paint` and `shade` call `write_glow(c)`, a private variable that the generated wrapper writes to location 1, following the pattern of `write_trail`.
- After the space is drawn, the light target is resolved, taken down to 1/4 and 1/8, blurred by a separable Gaussian at two sizes (sigma about 0.006 and 0.028 of the short side), and added back. This is the new engine shader `visual_light.wgsl`.
- It differs from the Picture's **bloom**, which stays: bloom is the lens (everything bright, thresholded, scene-wide); diffusion is the medium (only what a visual says glows inside it, at the strength it says).
- The moon jelly writes its gonads, rhopalia and stroke light as glow, so they bloom softly through the mesoglea instead of sitting sharp on the skin.

**Refraction: what is behind bends through the bell.**
- The space draws in **two phases**:
  - **Back**: every member's `shade` and line, plus the backdrop layers.
  - **Front**: every member's triangles.
- Between them the colour target is resolved into the **behind** texture, and the light target's alpha into **behind depth**: one resolve each, about 0.05 to 0.1 ms.
- `paint` can call `refract(uv, offset)`. It returns `behind(uv + offset) - behind(uv)` (zero where behind depth is nearer than this fragment, so a bell never bends what is in front of it), and the bell adds that to its light.
- Because refraction is **added as a difference**, it is order-free like the rest of the light, and it needs no alpha. Only one surface per pixel should apply it (the moon uses its outer skin, facing the eye), or the bend doubles.
- Behind is bound at binding 2, `prev`'s slot, so a visual with `feedback` cannot join the space (refused with a message).
- *Cost of the split:* the jellies' tentacles (a line, back phase) appear bent through the bells in front of them, which is correct. A bell does not see another bell through it bent, only added. With index-matched jellies that is invisible.

**Depth of field, from light depth.**
- A scene pass after diffusion: the circle of confusion from light depth against the focus distance (the scene camera's `dist`, or the hero in Close-up), a gather blur at half resolution, then a mix by the circle of confusion.
- The marine snow keeps its own analytic blur in `shade`, which is cheaper and already right.
- Thickness stays analytic (`glass`, `bell_thickness`). Back-face depth for true thickness would need a second pass over every bell, and the profile already knows how thick the bell is.

### 6. Effects, levelling, opacity, blend

When layers share targets, the per-layer 2D passes have nothing of their own to work on. **The space becomes one entry in the layer stack**, at the slot of its top member:

| | In the space | Flat layers (unchanged) |
|---|---|---|
| **Opacity** | per layer, applied inside the draw (the generated wrappers scale `paint`, `shade`, the line and the glow), so fades and scene switches work as today | as today |
| **Levelling** | one meter for the whole space, measuring the resolved scene picture; bright-part mode if any member asks | per layer |
| **Blend mode** | none: light adds and covering covers; the control is greyed out with a note. The space meets the flat layers below by Add | as today |
| **Effects** | bypassed and kept in the file; the Effects tab says "Effects work on flat layers; this layer is in the shared space" | as today |
| **Tint, palette, band** | per layer, as today (they reach the visual through `FrameCtx`) | as today |

The fixed order inside the space: back phase, resolve behind, front phase, resolve, diffusion, depth of field, meter. Then the space blends into the composite, and the Picture runs over everything.

A **"Space effects" chain** (a 2D chain over the finished space, reusing `render_layer`'s chain code) is cheap to add later if Greg wants effects on a shared scene. It is left out now so that two places for effects do not appear at once.

**Fades and switches:**
- A visual leaving the space fades by opacity inside it.
- A scene switch from shared to stack fades the whole space as one entry, with the group opacity applied when it blends into the composite.
- A switch between two shared scenes keeps one space: the outgoing members stay in it while they fade, and the camera eases to the new lead's camera over the fade.

### 7. 2D visuals and Ribbons

Three levels, cheapest first:

1. **Flat above:** composited as today (decision 1).
2. **Backdrop:** drawn into its own texture as today, then into the back phase as a screen-filling quad at the far plane. It is refracted, out of focus, and seen through the bells. This works for any visual, unchanged.
3. **Native:** the visual moves into world space (`"space": true`). For Ribbons that is a port: particles in the tank's coordinates with 3D curl noise, drawn with the scene camera. Their trails cannot come from `feedback` while the camera moves (last frame's picture no longer lines up), so each particle keeps its last N positions as items and draws them as a strip. That is the step where ribbons move between the jellies.

### 8. The visual format: what is added

All of this is opt-in. A visual that asks for none of it is unaffected:

| | Where | What |
|---|---|---|
| `"space": true` | visual.json | can join a shared space: reads `scene_camera()`, triangles carry perspective `w`; refused with `feedback` |
| `fn camera() -> Camera` | visual.wgsl | optional: what this visual's camera is when it leads |
| `scene_camera()`, `V.shared` | API | the space's camera this frame; 1 inside a shared space |
| `write_glow(c)` | `paint`, `shade` | this pixel's light for diffusion |
| `write_depth(z)` | `shade` | light depth for this pixel (default far) |
| `refract(uv, offset)`, `behind(uv)`, `behind_depth(uv)` | `paint` | what the back phase drew (front phase only) |
| `LinePoint.depth` | `point` | the line's view depth (0: far) |
| `"covers": true` | visual.json | later: its triangles write surface depth |

The generated wrappers grow a shared-space variant: MRT output, clip z, opacity. A visual's build key already hashes the source and flags, so the stacked and shared builds of one visual are two cache entries, both prewarmed for scenes that use them.

## Budget

The reference is a whole scene on one ordinary PC: RTX 2070 Super, 2560x1440 windowed, release build. Measured the scratchpad `perf.sh` way: the median of 8 debug lines, over two runs, since layouts vary per run.

Estimated costs, to be replaced by measurements in increment 0 and after each step:

| | Change |
|---|---|
| Multisample memory, Deep sea (3 jelly layers) | 354 MB → 236 MB (one colour target plus one light target) |
| Per member saved | its own meter and blend, 0.27 to 0.29 ms each (measured, below) |
| Light target (second MRT on every draw) | +0.1 to 0.3 ms per jelly layer (the bells' 0.61 ms pass is fill-bound) |
| Behind resolves | +0.05 to 0.1 ms |
| Diffusion | +0.15 to 0.25 ms (measured: 0.23 for the passes; see increment 2) |
| Depth of field | +0.3 to 0.5 ms |

**Baseline (increment 0, measured 2026-10-19 at d381bb5).** Stacked, 2560x1440 windowed, release, music playing; the median of the last 8 debug lines, two runs each, which agreed within 0.04 ms:

| Scene | GPU frame | Visuals | Meter and blend per layer | Composite, Picture |
|---|---|---|---|---|
| Jellyfish (3 sea nettles with plankton, 7 moons) | 2.94 to 2.96 ms | 0.92 to 1.00, 0.79 to 0.80 | 0.27 to 0.32 | 0.14, 0.41 |
| Deep sea (5 atollas, 6 comb jellies, 4 crystal jellies) | 3.51 to 3.52 ms | 0.99 to 1.03, 0.53 to 0.57, 0.48 | 0.27 to 0.29 | 0.14, 0.41 |
| Giants (2 lion's manes, 5 moons) | 2.81 to 2.85 ms | 1.03 to 1.05, 0.59 to 0.60 | 0.27 to 0.29 | 0.14, 0.41 |

Every run held 59 fps. The worst frame of each second was 17.6 to 22.3 ms, which is a missed vsync now and then, not a stall. Meter and blend cost about three times the first estimate, so a three-layer space saves about 0.55 ms. That pays for diffusion and refraction together.

**Target:** Deep sea and Giants in a shared space, with diffusion and refraction, within 0.6 ms of their stacked cost, and the whole frame under 4 ms. Depth of field on top is under 1 ms. If the light target costs more than estimated, the fallback is RG11B10 for the glow (half the bandwidth) with light depth moved to its own R16Float target.

## Increments

Each increment ends measured, with snapshots and docs current. The jellies come first.

0. **Baseline.** Measure Jellyfish, Deep sea and Giants as they are: frame time and worst frame. Recreate the scratchpad helpers (`run.sh`, `seq.ps1`, `grid.ps1`, `crop.ps1`, `perf.sh`).
1. **The space.** Built from:
   - the scene setting and the file field;
   - the space as one stack entry, with one colour target;
   - `"space": true`, the lead's published camera, and `Camera` in the core API;
   - opacity inside the draw, one meter, effects and blend greyed out with their notes;
   - the shared-space wrapper variants.

   The jellyfish read `scene_camera()`. **Exit:** in a shared space, Deep sea looks the same as stacked, except that its three layers now provably share one camera. Its cost is the same or lower, every stacked scene is byte-for-byte unchanged in behaviour, and the factory tests pass.
2. **Diffusion.** The light target, `write_glow`, light depth from triangles and lines, and `visual_light.wgsl`. The moon jelly's organs and stroke light glow through the body.
3. **Refraction.** The two phases, the behind resolves, `refract` and `behind`, and backdrop layers. The moon's outer skin bends what is behind, subtly; check it against a backdrop of Ribbons or Spiral.
4. **Depth of field.** From light depth, focused by the scene camera.
5. **Shaded tentacle lines.** Lines with depth, lit and shaded, and the line pipeline in the multisampled pass.
6. **Ribbons into the space.** The native port (decision 7).

### Increment 1, done (0.4.1, 2026-10-19)

Built as designed, except where noted:

- `Settings.space` (`Stacked` or `Shared`), saved in the scene file only when shared, so every stacked file is byte-identical. No panel control yet: a scene is made shared in its file.
- `"space": true` and `fn camera` in the format; `Camera`, `in_space()`, `scene_camera()` and `V.space` (in a space, the layer's opacity) in the API; the camera reaches every visual at binding 10 from one buffer, copied from the lead's generated `cs_camera` output each frame.
- One source per visual for both modes: the wrappers scale by `V.space.y`, which is 1 when stacked. A visual with `"space"` builds its shared-space pipelines (additive picture, triangles and line, all 4x) in the same job.
- The director draws the space as one stack entry: the lead's update and camera first, then the others' updates, one multisampled pass for every member bottom to top, resolved, then one meter and one blend.
- **Levelling, as Greg decided: one exposure for the space**, like a camera. Two changes from the first version, both from snapshots of Deep sea:
  - The space's meter counts a block as lit only above **1/8** of the brightest (a layer uses 1/32). At 1/32 every species' faint glow and tentacles counted, the mean fell and the bright jellies bleached. At 1/8 the three species balance much as they did stacked.
  - The space reaches the Picture **weighted by its members** (their levels, summed). Stacked, each layer is levelled to the target and they add, and the Picture's auto exposure is tuned to that sum.
  - What still differs: a layer brighter than the others when levelled alone (Deep sea's atollas) now gets the space's gain, so its faint parts (the plankton) come up. Opacity balances layers against each other under one gain. Greg asked for a per-layer **Brightness** offset as the knob for it (next).
- **Measured** (2560x1440, release, median of 8 debug lines), shared against stacked with the welcome sheet's 0.11 ms taken off the baseline: Deep sea 2.67 against 3.41 ms, Giants 2.50 against 2.72, Jellyfish 2.65 against 2.84.

### Increment 1, finished (0.4.2, 2026-10-19)

- **Panel:** a **Shared space** toggle in the Layers section under the stack. In a space, a layer's Visual tab shows a note in place of Blend, and its Effects tab says its effects are kept but not drawn.
- **Brightness**, per layer, in stops (-3 to +3, ×2 per stop), applied after levelling in both modes: stacked, in the layer's blend; shared, inside its draws. It is saved only when not 0, so files stay as they were. Under one exposure it balances layers against each other: lowering one lets the space's gain rise for the rest, as a camera's auto exposure would.
- **The factory scenes Jellyfish, Deep sea and Giants are shared now.** Each one's plankton went from 0.35 to 0.15, because the layer that carries it is no longer held down by levelling of its own. Jellyfish's moons are at +0.5 stops. Snapshots against stacked: the same balance of species and the same faint plankton.
- **Not done:** greying out the camera controls (Camera drift, View) on members that do not lead. It needs the manifest to say which controls are the camera's; it comes with the next format change.

### Increment 2, diffusion (0.4.5, 2026-10-19)

Built as designed, except where noted:

- **The light target**: a second 4x RGBA16F attachment of the space's pass, resolved after it; rgb is the glow (added), alpha the light depth (Min). Every space pipeline writes location 1 from generated shared-space entries (`fs_space`, `vs_mesh_space` and `fs_mesh_space`; the line's `fs_space` in `visual_lines.wgsl`), so the stacked entries and pipelines are untouched (stacked Jellyfish measured the same, 2.88 to 2.91 ms against 2.94 to 2.95). Triangles carry their view depth (`clip.w`), the line `LinePoint.depth` (the points buffer's fourth component), `shade` `write_depth(z)`; a fragment that lights nothing writes `LIGHT_FAR` (50).
- **Changed: what is written to the light target is masked.** With the depth kept (the alpha's Min blend) the bells' pass cost about 0.3 ms more; with `shade` and the line also writing every pixel of the 4x target, once per member, another 0.3 to 0.5. Nothing reads light depth until refraction and depth of field, so the triangles write the glow only (`LIGHT_GLOW`) and `shade` and the line write nothing there (`LIGHT_NONE`). The outputs stay in the shaders. When refraction lands, light depth comes back in a target of its own (the R16Float fallback). Meanwhile `write_glow` takes effect from `paint` only.
- **`write_glow(c)`** adds to a private variable the generated wrapper writes out, scaled by the layer's opacity; the visual decides how much of its light stays sharp. A visual that calls it anywhere (`ShaderDef::glow`, `Scene::space_glows`) turns diffusion on for its space.
- **The passes**: glow's own prefilter and blur (`glow_blur.wgsl`, threshold 0, so everything passes) take the light target to a quarter and blur it narrow (sigma 0.6% of the short side), then to an eighth and wide (2.8%); `visual_light.wgsl` adds both back into the space's picture, 65/35. 0.23 ms at 2560x1440.
- **Changed: the meter sees the glow sharp.** Its mean over the lit part fell as light spread over more blocks, and the levelling lifted the whole space (the marine snow brightened, the glow turned to fog). The meter's 1/8 box is linear, so the space now takes its picture down before diffusion and adds the resolved glow on top (`meter_down`, `down_add`): it measures the light as drawn, the glow still sharp (about 0.08 ms).
- **The jellies**: `diffuse(c, part, scale)` in `looks.wgsl` moves `part` of a light into the glow in a space (outside one it all stays sharp, so stacked scenes look as before). The stroke's light 0.5 for every species; organs (gonads, stomachs, the lion's heart) 0.5; small bright points (rhopalia, photophores, crystal rim lights, the atolla's dots and pinwheel) 0.6; broad patterns (nettle stripes, lion folds) 0.3; comb rows 0.25. Hair and tentacles are the line, which does not glow yet.
- **Measured** (2560x1440, release, median of 8 debug lines, two runs), shared, against 0.4.4 the same way: Jellyfish 3.21 to 3.24 ms (2.78 to 2.82), Deep sea 3.31 to 3.37 (2.80 to 2.88), Giants 3.33 to 3.40 (2.56 to 2.79); the meter's added glow pass is about 0.08 of that. Stacked Deep sea is about 3.5, so the shared jelly scenes are still cheaper than they were stacked.
- **Not done:** the build. The space's triangles were the stacked pipeline before; now they are a pipeline of their own, a second compile of `paint`, and the Jellyfish takes 10.0 s to build instead of 5.6 (FXC). A shared jelly scene shows nothing until both are built. On the roadmap: build the shared and stacked variants as separate jobs, the one in use first.

Later, when asked for: surface depth and `"covers"`, weighted blended OIT, a Space effects chain, a depth slider for backdrops, collisions.

## Risks

- **Visual identity of tuned scenes.** Stacked must not change at all. Shared-space code paths are separate entries and separate wrapper variants; a test compares the generated stacked source before and after.
- **Levelling a whole space** may sit differently from three levelled layers (a dim species next to a bright one). Per-layer opacity is the knob; watch it in increment 1.
- **MRT bandwidth** on the fill-bound bell pass. Measured in increment 2; fallback above.
- **Refraction double-counting** if two surfaces in a pixel both apply it. The rule is one surface per pixel; the moon applies it on its outer skin only.
- **Build time.** A second variant per visual doubles its builds when a scene is shared. The worker and the cache absorb that; watch the prewarm time (about 6 s today).

## Answers (Greg, 2026-10-19)

1. **"Shared space" is a scene setting**, flat layers placed by slot order (below: backdrop; above: flat on top). Yes.
2. **Effects on shared layers are bypassed for now.** Yes, and *revisit it*: a Space effects chain is on the roadmap ("Revisit: effects in a shared space") so it is not forgotten.
3. **The top member leads the camera.** Yes.
4. **Factory scenes:** as recommended. Jellyfish, Deep sea and Giants switch to a shared space when increment 1 meets its exit (they should look the same, with one camera); the gains (diffusion, refraction) then reach them as each lands. Every other factory scene stays stacked. Each switch is a factory file change, so `rewrite_factory_history` runs with it.
