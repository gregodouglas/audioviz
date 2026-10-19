# Visuals: the registry and how to add one

The reference. For a walk-through (a first visual, three worked examples, a performance budget, debugging and sharing), read [`docs/building-visuals.md`](building-visuals.md).

A **visual** is the generator inside a layer (see `docs/model.md`). Everything else a layer has belongs to the engine and applies to any visual without the visual knowing: the band it reacts to, its tint and palette, blend and opacity, the effect chain and its director, presets, scene files, levelling, and the Performance timings.

## The registry (`crates/app/src/visuals.rs`)

Every visual has a `VisualDef`:

| Field | What it is |
|---|---|
| `id` | Stable id: the layer's `"visual"`, the key of its parameter block, the presets folder name. Never changes once shipped. |
| `name`, `short`, `summary` | Visual list name, short name (bar, stack, breadcrumb), one-line tooltip. |
| `source` | Where it comes from: `Source::Loaded`, its manifest and code (below). |
| `band_waveform` | The visual draws the waveform itself, so a layer band filters the waveform (only the scope). |

`visuals::all()` lists them in Visual-list order; `get`, `name`, `short`, `summary` and `make` look one up, and `VisualDef::params` gives a visual's full parameters as JSON from whatever a layer stores (defaults filled in). A layer whose visual is not installed keeps its id and settings and draws the scope until the visual comes back.

**Every visual is loaded from a folder** (since the Sigil's port, 2026-10-19). The compiled-in path (`scenes/`, `Source::Native`, typed parameter structs) is gone; `Source` keeps its shape, so another kind of visual would be a second variant. Adding a shipped visual is a folder in `assets/visuals/`, a factory scene in `assets/scenes/` and, when it follows someone's work, a credit in `docs/INSPIRATION.md`.

## Parameters

A layer stores each visual's parameters as a JSON block under the visual's id (`Layer.params`, flattened, so a file reads `"visual": "wheel", "wheel": { ... }`). The manifest says what each value is; the panel is built from it and writes the block back. A block is kept when the layer switches to another visual, so switching back loses nothing; scene files keep only the active visual's block.

`FrameCtx` gives the engine everything a visual may read each frame: the band-limited audio features and waveform, time and size, the layer's slewed tint, its baked palette with the shift and amount, the album art and photos folder. `shader_visual.rs` turns it into the API below.

## Visuals loaded from a folder

A visual can also be two files in a folder, no Rust and no rebuild:

```
%APPDATA%\audioviz\visuals\<folder>\visual.json
%APPDATA%\audioviz\visuals\<folder>\visual.wgsl
```

The app reads the folder at start and lists each visual with the shipped ones, yours marked "Yours"; a folder added while it runs is picked up within a few seconds (`visuals::refresh`, while the Visual tab is open), and the Visual tab's **Your own visuals** group makes one from a starter (`assets/visual-template/`), opens the folder, and restores the factory ones. It is an ordinary layer from then on: band, tint, palette, blend, effects and their director, presets, scenes and the Performance timings all apply. Save either file while it runs and it reloads; a shader that does not compile shows its error (with the line in `visual.wgsl`) in the Visual tab while the last good version keeps drawing. Shaders run inside wgpu's validation and can only draw (no files, no network), so a folder is safe to share. Factory ones ship from `assets/visuals/` and are written into the folder on first run, like presets and scenes; a copy you never edited is brought up to date when a newer app ships a changed version (the app recognises it by a fingerprint list of every version that ever shipped, `assets/factory-history.txt`; rewrite it with `cargo test -p audioviz rewrite_factory_history -- --ignored` after changing a factory file, and a test fails until you do), while an edited one is left alone; a dev run reads the repo's folder (debug builds, with live reload) or the embedded copies, and never writes. **Eclipse** is one (`assets/visuals/eclipse/`): it replaced the compiled-in Eclipse under the same id and parameter names, so existing layers, scenes and presets carry over.

### visual.json

```json
{
  "id": "eclipse",
  "name": "Eclipse",
  "short": "Eclipse",
  "summary": "One line for tooltips.",
  "credit": "Who the idea or code comes from, and its licence.",
  "hint": "The panel's closing explanation.",
  "feedback": false,
  "params": [
    { "id": "count", "label": "Dots", "type": "integer", "group": "Ring", "min": 4, "max": 64, "default": 16 },
    { "id": "radius", "label": "Radius", "group": "Ring", "min": 0.1, "max": 1.2, "default": 0.5, "help": "Tooltip." },
    { "id": "mirror", "label": "Mirror the spectrum", "type": "toggle", "default": true },
    { "id": "print", "label": "Print", "type": "choice", "options": ["Contours", "Dots", "Grid"], "default": "contours" },
    { "id": "gain", "label": "Gain", "min": 0.1, "max": 4, "default": 1, "log": true, "unit": "x" }
  ]
}
```

- `id`: lowercase letters, digits, `-` or `_`; what files point at, so it never changes. An id already taken (a built-in) is skipped.
- Up to 24 `params`. A param id that WGSL reserves (Loop's `smooth`) is `P.<id>_` in the shader; files keep the id. `type` is `slider` (the default), `integer`, `toggle`, `choice` (three options or fewer show as a segmented control, more as a dropdown) or `button` (not saved; the shader sees how many times it was pressed since the visual was picked, so a change is an event: the Sigil's New arrangement). An integer may have `"zero": "By track"`: 0 is then a setting of its own shown by that name, and values between 0 and `min` snap to `min` (the Sigil's seed overrides). `"decimals": n` fixes how many decimals a slider's value shows. Param ids are lowercase identifiers, not WGSL words. `group` gathers controls under a heading, in order of first use. Values are saved in the layer as `"<id>": { "radius": 0.5, ... }`.
- `feedback: true` keeps last frame's picture for `prev(uv)`.
- `uses: [...]` puts shared libraries (`crates/app/shaders/library/`) between the API and the code. Libraries are opt-in so their names never collide with a visual that does not ask for them; an unknown name is refused, and a library brings the ones it needs. There are three:
  - `"creatures"`, things that move in 3D: a camera into a tank (`drifting_camera`, `to_camera`, `camera_dir`, `project`, `clip_pos`, `camera_position`, `facing`, `depth_fade`; set `look` to aim it somewhere other than the middle), a water `current`, a swim `stroke` and a springier `elastic_stroke`, `chain_link` for tentacles, hair and arms (no momentum), `turn_toward`, `perpendicular`, `keep_apart`, `creature_hash`, `wander_point`.
  - `"luminous"`, how translucent, light-giving things look (everything is emission): `glass` (light by how much jelly the eye looks through), `fresnel` (edge glow), `key_light`, `glint` and `sheen` (the light above: shape, the wet highlight, the glow of thin skin seen toward it), `water` (dimmer and bluer with distance), `halo`, `spectrum` and `thin_film` (iridescence), `thin_line` (anti-aliased fine lines), `photophores`, and procedural textures: `fbm`, `cells` (Voronoi) and `keep_detail` (fade texture finer than a pixel).
  - `"jelly"` (brings both): bells that swim. A `Bell` describes a profile (its turn at rest, how a stroke curls it, the lagging margin's flare, the jelly's thickness, lobes, notches, a groove); `bell_node` integrates it into a profile node per prepare item each frame, `bell_point` builds the two skins from the nodes, `bell_tangent`, `bell_thickness`, `bell_lobes`, `bell_drive`.
- `parts: ["species.wgsl", ...]` (up to 16): more code files in the visual's folder, read after `visual.wgsl` in order (one module: anything in one file can call anything in another). An error names the file and its line; saving any of them reloads the visual. The Jellyfish is in seven files.
- `antialias: true` (with `triangles`) draws the picture and the triangles 4x multisampled in one pass and resolves it into the output: smooth silhouettes on meshes for about the cost of the resolve (the Jellyfish's bells cost less than they did without it, since the composite pass went). Not with `trail` yet.
- `levelling: "bright"`: the layer's levelling measures only the bright part of the picture (blocks over 1/32 of the brightest), so faint fill over the frame (plankton, glow in the water) does not count as lit and lift the bright things until they bleach. The default, `"lit"`, counts every lit block.
- `trail: true` gives `shade` a second picture that persists and is not drawn: `write_trail(c)` stores this pixel's trail, `trail(uv)` reads last frame's (black where nothing was written). Add it to what `shade` returns. The Sigil keeps its afterimages there (`max(trail(uv) * keep, fresh)`), so rays and the pen linger while the glyph itself stays sharp, which feedback (the whole picture) cannot do. Needs `fn shade`.
- `pictures: { "from": "<choice id>", "dwell": "<number id>" }` turns on the picture feed: the album art, or (when the choice is on its second option and the photos folder is set) a shuffled slideshow of the photos folder, a new photo every `dwell` seconds. `picture(uv)` is the current picture and `picture_prev(uv)` the one before (0..1 across, y down, mirrored outside, linear colour); `V.picture_fade` (eased 0..1 over 1.5 s), `V.picture_aspect`, `V.picture_prev_aspect` and `V.picture_count` go with them. The panel shows the photos folder field when the choice picks photos.
- `triangles: N` (up to 1048576) draws N triangles from `fn vertex(i: u32) -> MeshVertex` (corner i, three per triangle, in order): `pos` in centred units with a perspective `w` (`(x, y, 0, 1)` when flat), `colour` premultiplied with coverage in `a` (0 adds light, 1 covers), four free `data` values. An optional `fn paint(f: MeshFragment) -> vec4<f32>` colours each pixel from the interpolated `colour`, `data` and its `pixel` position (derivatives such as `fwidth` work there); without it the interpolated colour is drawn. Order per frame: `shade` (when present), the triangles, the line; the first clears. Required with a `vertex` function and refused without one.
- `line_style: "beam"` draws the line as a phosphor beam (the scope's): `width` is the Gaussian sigma in pixels and each segment's brightness falls with its length, so a lingering beam is bright and a racing one dim. The default is `"line"`.
- `band_waveform: true`: when the layer reacts to a band, the waveform itself is band-filtered (the scope), not only the levels and spectrum.
- A choice may list `"values"`, one per option: what it saves, when that differs from the option's name (the scope's "Key and speed" saves `"key"`).
- `line_count: "<param id>"` draws only as many of the line's points as that number control says (Spiral's Points); `point(i)` is called for those only.
- A visual with a line may leave out `fn shade`: the line is then the whole picture and the full-screen pass is skipped (Harmonograph 0.19 → 0.05 ms).
- `line: N` (2 to 65536) draws a line through the N points that `fn point(i: u32) -> LinePoint` gives (`pos` centred, `width` in pixels with 0 breaking the line, `colour`), added over `shade`'s picture. `LINE_POINTS` is N. Required with a `point` function and refused without one.
- `items: N` (up to 262144) runs `fn prepare(i: u32) -> vec4<f32>` from `visual.wgsl` once a frame for each `i < N`, before `shade`; `shade` reads the results with `item(i)`. Inside `prepare`, `item(i)` is last frame's value, so items can keep state. `ITEMS` is N in the shader. Required with a `prepare` function and refused without one.

### visual.wgsl

One function, called for every pixel:

```wgsl
fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();               // the manifest's params by name: P.radius, P.mirror (0 or 1), P.print (option index)
    let p = centred(uv);            // short side's half-extent 1, y up
    let ring = abs(length(p) - P.radius) - 2.0 * pixel();
    return colour(V.flow * 0.1) * smoothstep(pixel(), 0.0, ring) * (0.4 + band_at(0.1));
}
```

The colour is linear HDR light (values above 1 are fine; levelling, effects and the Picture do the rest; NaN pixels are dropped). Available, from `shaders/visual_api.wgsl`:

| | |
|---|---|
| `V.time`, `V.dt`, `V.resolution` | seconds, frame time, pixels |
| `V.flow` | a music clock: 0.5 to ~1.7 s per second with the energy; use it for motion |
| `V.energy`, `V.bass`, `V.mid`, `V.treble` | levels against their own averages (~1 typical) |
| `V.beat`, `V.beat_phase`, `V.beat_count`, `V.bpm` | beat pulse 0..1, phase through the beat, count |
| `V.onset`, `V.onsets` | onset pulses (all; low, mid, high, all) |
| `V.hits`, `V.strengths` | 1 on the frame an onset is detected, and the onset strength now (low, mid, high, all) |
| `V.confidence`, `V.silent` | how sure the beat clock is (0..1); 1 when nothing is playing |
| `V.swell` | bass above its average plus low onsets, fast attack, slow release |
| `V.key`, `V.quiet` | dominant pitch class (C = 0), 1 in a quiet passage |
| `band(i)`, `band_raw(i)`, `band_at(t)` | 32 bands low to high, smoothed or raw; `band_at` interpolates t 0..1 |
| `chroma(i)`, `chroma_slow(i)` | 12 pitch classes, C first; the slow profile (the chord over seconds) |
| `wave(t)`, `wave_mono(t)` | the last 1024 samples (t 0 oldest .. 1 newest), auto-gained |
| `MeshVertex`, `MeshFragment` | a triangle corner from `vertex(i)`, and what `paint` gets per pixel |
| `V.track` | 0..1, a hash of the playing track: a seed that changes with the track |
| `V.instance` | 0..1, random per layer, so two layers of one visual (Deep field's two Ribbons) do not move alike |
| `sample(i)`, `V.wave_new`, `V.auto_gain`, `V.sample_rate` | sample i of the last 4096, newest first; how many arrived since last frame (a trace draws exactly those); the gain already applied; samples per second |
| `colour(t)` | the layer's palette at t (moving with its cycle) over its tint |
| `prev(uv)` | last frame (with `feedback`) |
| `trail(uv)`, `write_trail(c)` | last frame's trail and this frame's (with `trail`) |
| `LinePoint`, `LINE_POINTS` | a point of the line (`pos`, `width` px, `colour`) and how many there are |
| `art(uv)`, `V.has_art` | the playing track's album art (512 x 512, linear colour, y down; black and 0 without art) |
| `item(i)`, `ITEMS` | the prepare step's item `i` (this frame's in `shade`, last frame's in `prepare`), and how many |
| `centred(uv)`, `uncentred(p)`, `pixel()`, `TAU` | coordinates (short side's half-extent 1, y up) and back to uv, one pixel in them |
| `rotate(p, a)`, `noise(p)` | turn a point by `a` radians; smooth value noise 0..1 |
| `hash21`, `hash22`, `ign` | hashes and interleaved gradient noise from the shared prelude |

A loaded visual sees only the shared `params.wgsl` prelude and this API, not the built-in visuals' own helpers, so their names never collide with yours. `cargo test -p audioviz factory_visuals_compile` compiles every factory visual the way the engine does.

Factory examples, each a worked example in the guide: **Polygon** (a still shape), **Pulse** (bands, beats, onsets, the music clock), **Comet** (feedback trails), **Eclipse** (a prepare step), **Harmonograph** (a line, with state in a prepare step). **Jellyfish** (`assets/visuals/jellyfish/`, 2026-10-19) is the first creature visual and the creature libraries' first user, in seven files (`visual.wgsl` the overview and item map, then `species`, `swim`, `body`, `mesh`, `looks`, `tank`): one species per layer (moon jelly, sea nettle, crystal jelly, atolla, comb jelly, lion's mane) in a 3D tank as wide and tall as the picture, seen slightly from below by a slowly drifting camera, or in a close-up that follows one jelly. Each jelly is five prepare items (position and stroke time, velocity, axis and spin, size and hue, its light: glow, flash, wave phase, hue drift) and its bell 25 profile nodes (`bell_node`, from the species' `Bell`); up to 160 strands of 16 links per jelly (tentacles or hair, rooted on the moving margin, one item per link, `chain_link`) are the line; four oral arms of 16 links are V-folded sheets; each bell is two skins of 24 x 64 quads, antialiased, painted by one shared look (glass, edge glow, the light above and its glint, the muscle lighting as it squeezes, water) plus the species' own organs and light; its glow in the water is a sprite, and marine snow in four layers (parallax and depth of field from the camera) is lit by it. Every light answers its own part of the sound, smoothed and bounded, scaled by Music: Natural, Reactive or Performance; without music they still swim and glow on their own. 0.5 to 0.7 ms for four of a species at 1600x1000, about 1.5 for six moon jellies at 2560x1440 with snow; a close-up (one bell filling the screen) about 2.4. **Sigil** (`assets/visuals/sigil/`) is the twelfth port and the last: with it no visual is compiled in. Its CPU state is 81 prepare items: the rebuild (lit, going out, the pending seed, last track, the New arrangement count), the slews and turns, six grouped bands, the arrangement (13 items, built on a rebuild from a seed out of `V.track`), this frame's trace points, 24 ray slots and 16 drips of two items. Every item works out the frame's events (rebuild, ray strike and which slots it takes, drip spawn) from last frame's items the same way; item 7 also says which rays and drips are live, as bits, so `shade` visits only those. Afterimages are the new `trail`; the format also gained `button`, `zero` and `decimals` controls, 24 params, and `V.hits`, `V.strengths`, `V.confidence`, `V.silent`, `chroma_slow()`. The arrangement comes from a different random sequence, so a track's glyph differs from the compiled-in one's; for the A/B the old build was patched to the same sequence and drew the same glyph. 2.7 ms at 2560x1440 against 2.8 (after replacing a runtime vector index in the ring loop, which had cost 0.8 ms). Building it takes about 1.4 s on D3D (FXC, then the driver); a first version took 2.8 s, from a struct of arrays copied about and the ray picking inlined twice (see the guide's budget). **Kaleidoscope** (`assets/visuals/kaleido/`) is the eleventh, on the new picture feed: each picture's window (angle, drift phase, turn direction) is a prepare item, the outgoing one handed over when `V.picture_count` changes; 0.11 ms compiled in, 0.17 loaded. **Flow-field ribbons** (`assets/visuals/ribbons/`) is the tenth: up to 65536 particles, two prepare items each (position and last position; age, life, seed, pitch class), advected through the curl-noise field and the twelve chroma vortices in the prepare step, each drawn as a quad round its last step (131072 triangles, Gaussian stroke in `paint`), trails by feedback; 0.17 ms at 12000 particles and about 0.41 at 65536, as before. **Crystal** (`assets/visuals/crystal/`) is the ninth: the CPU's shard poses (quaternions, scales, opacity, colour, ease), tumble and eased group tilt are 84 prepare items, a beat re-poses the shard a hash of the beat count picks (so every item agrees); `vertex` builds each shard's corners with quaternion maths, sorts solid shards far to near, and draws 64 faces then 96 edge quads (256 triangles), glass with alpha 0, solid with coverage; 0.07 ms both ways. **Waves** (`assets/visuals/waves/`) is the eighth: up to 160 rows of 256 points in 10402 prepare items (Loop's clock and packing; the smoothing holds the end columns, so it runs pass by pass in a shrinking window), drawn as 163200 triangles, each row a black fill (alpha 1) then its line quads (premultiplied coverage), far rows first; 0.20 ms both ways. **Ripples** (`assets/visuals/ripples/`) is the seventh and the first in triangles: a 160 x 160 grid (51200 triangles) whose corners `vertex` places by the old projection, far row first so near hills cover far ones, printed per pixel in `paint` (contours, dots or grid by `fwidth`); flow, swell and four beat rings (seeded by `V.track`) are prepare items. 0.26 ms both ways. **Black hole** (`assets/visuals/blackhole/`, id `starfield`) is the sixth: the 380-line ray march carried over almost unchanged (generated from the old `starfield.wgsl`: its uniform became a private struct `P` that `shade` fills per pixel from `params()`, `V` and one prepare item holding the slewed bass, slewed energy, disk phase and camera azimuth; its `band()` became `hole_band()`), 4.0 ms compiled in and 4.15 loaded. **Loop** (`assets/visuals/loop/`) is the fifth: up to 120 waveform rings of 256 points kept in 7802 prepare items (a ring clock every item works out the same way, four columns per item, one level per ring from last frame's values), the repeated smoothing passes folded into one precomputed kernel per pass count, and every ring drawn as one line broken between rings (0.09 ms compiled in, 0.13 loaded). **Phosphor scope** (`assets/visuals/scope/`) is the fourth: feedback for the phosphor, a beam-style line through exactly the samples that arrived this frame (from last frame's final one, so the trace is continuous), Frequency colouring from zero crossings in the shader (0.11 ms either mode; the compiled-in one did the crossings on the CPU). Shipped visuals whose folder is missing or broken load from the copies inside the app, so the scope, every missing visual's stand-in, is always there. **Spiral** (`assets/visuals/spiral/`) is the third port, a line visual with its drift, turn and beat kick in one prepare item (0.26 ms compiled in, 0.20 loaded). **Spectrum wheel** (`assets/visuals/wheel/`) is the second port of a compiled-in visual: its CPU state (turn, beat jumps, the walking band offset, smoothed wedge levels) lives in a prepare step, item 0 for the wheel and one item per wedge holding its finished colour.

How a visual is built: on a worker thread (`shader_visual::Shared`), so the render thread never waits on FXC or the driver. A layer whose pipelines have not arrived reports `Scene::ready() == false`; the director then holds the slot dark, keeps the visual it replaced drawing in its outgoing seat (its fade paused), and remembers a scene switch's fade timing for when the new one is ready. Builds are kept by a hash of the composed source and flags, so a visual is compiled once a run and every layer showing it shares the pipelines (the bind group layouts are shared for that), and restoring an edited file reuses the earlier build. At startup `prewarm` queues every visual the scenes use and then the rest of the list, behind anything a layer is waiting for (a waiting layer's job jumps the queue). All 18 factory visuals build in about 6 s in the background; through four scene switches during and after it, the longest frame was 12 ms (2560x1440, release), where building on the render thread had stalled up to 2.7 s.

How the prepare step runs: a compute pass (`cs_main`, generated after the visual's code, 64 per workgroup) writes one of two item buffers while reading the other through `item()`; the fragment pass then reads the one just written. Eclipse moved its dot levels, positions and colours there: 1.1 → 0.69 ms at 16 dots and 4.5 → 2.7 at 64 (2560x1440, release), under the old compiled-in Eclipse's 0.8 and 3.5.

The album art is resampled on the CPU to 512 x 512 once per track (`image`'s triangle filter) and written into an sRGB texture each loaded visual owns; `has_art` took the uniform's old pad slot, so the layout did not move.

How the line runs: a compute pass (`cs_line`, after `cs_main`) writes each `point(i)` as two vec4 into a storage buffer; the engine's own `visual_lines.wgsl` draws instance i as the quad from point i to i + 1 (soft edge, round caps at half strength so joins do not double, colour and width blended along it), additively into the visual's output after the `shade` pass. Factory example: **Harmonograph** (6000 points, 0.19 ms).

What a loaded visual cannot do yet: a depth test, pictures of its own (only the album art and the photos folder), state that survives a restart. Each is a possible extension of the format; the guide's "Current limits" has the list.
