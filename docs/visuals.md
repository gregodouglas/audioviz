# Visuals: the registry and how to add one

The reference. For a walk-through (a first visual, three worked examples, a performance budget, debugging and sharing), read [`docs/building-visuals.md`](building-visuals.md).

A **visual** is the generator inside a layer (see `docs/model.md`). Everything else a layer has belongs to the engine and applies to any visual without the visual knowing: the band it reacts to, its tint and palette, blend and opacity, the effect chain and its director, presets, scene files, levelling, and the Performance timings.

## The registry (`crates/app/src/visuals.rs`)

Every visual has a `VisualDef`:

| Field | What it is |
|---|---|
| `id` | Stable id: the layer's `"visual"`, the key of its parameter block, the presets folder name. Never changes once shipped. |
| `name`, `short`, `summary` | Visual list name, short name (bar, stack, breadcrumb), one-line tooltip. |
| `make` | Builds the `Scene` at a size. |
| `params` | Its full parameters as JSON from whatever a layer stores (defaults filled in); `visuals::typed::<T>` for a parameter struct `T`. |
| `band_waveform` | The visual draws the waveform itself, so a layer band filters the waveform (only the scope). |

`visuals::all()` lists them in Visual-list order; `get`, `name`, `short`, `summary` and `make` look one up. A layer whose visual is not installed keeps its id and settings and draws the scope until the visual comes back.

## Parameters

A layer stores each visual's parameters as a JSON block under the visual's id (`Layer.params`, flattened, so a file reads `"visual": "wheel", "wheel": { ... }` exactly as before the registry). A visual reads its own typed, in place: `let p = layer.get::<WheelParams>(ID);` and writes them back from its panel with `layer.put(ID, &p)`. A block is kept when the layer switches to another visual, so switching back loses nothing; scene files keep only the active visual's block.

## Adding a built-in visual

1. `scenes/<name>.rs`: the parameter struct (`Serialize + Deserialize + Default`, `#[serde(default)]`), the `Scene` (`id()`, `render`, `output`, `resize`, `reload_shaders`, and `ui` for its controls), `pub const ID`, and `pub fn def() -> VisualDef`. Shaders go in `crates/app/shaders/` and in the `shaders.rs` table.
2. One line in `visuals::builtin()`.
3. A factory scene in `assets/scenes/` and, when it follows someone's work, a credit in `docs/INSPIRATION.md`.

`FrameCtx` gives the visual everything it may read each frame: the band-limited audio features and waveform, time and size, the layer's slewed tint, its baked palette with the shift and amount, the album art and photos folder.

## Visuals loaded from a folder

A visual can also be two files in a folder, no Rust and no rebuild:

```
%APPDATA%\audioviz\visuals\<folder>\visual.json
%APPDATA%\audioviz\visuals\<folder>\visual.wgsl
```

The app reads the folder at start and lists each visual with the built-ins, marked "Loaded"; a folder added while it runs is picked up within a few seconds (`visuals::refresh`, while the Visual tab is open), and the Visual tab's **Your own visuals** group makes one from a starter (`assets/visual-template/`), opens the folder, and restores the factory ones. It is an ordinary layer from then on: band, tint, palette, blend, effects and their director, presets, scenes and the Performance timings all apply. Save either file while it runs and it reloads; a shader that does not compile shows its error (with the line in `visual.wgsl`) in the Visual tab while the last good version keeps drawing. Shaders run inside wgpu's validation and can only draw (no files, no network), so a folder is safe to share. Factory ones ship from `assets/visuals/` and are written into the folder on first run, like presets and scenes; a dev run reads the repo's folder (debug builds, with live reload) or the embedded copies, and never writes. **Eclipse** is one (`assets/visuals/eclipse/`): it replaced the compiled-in Eclipse under the same id and parameter names, so existing layers, scenes and presets carry over.

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
- Up to 16 `params`. A param id that WGSL reserves (Loop's `smooth`) is `P.<id>_` in the shader; files keep the id. `type` is `slider` (the default), `integer`, `toggle` or `choice` (three options or fewer show as a segmented control, more as a dropdown). Param ids are lowercase identifiers, not WGSL words. `group` gathers controls under a heading, in order of first use. Values are saved in the layer as `"<id>": { "radius": 0.5, ... }`.
- `feedback: true` keeps last frame's picture for `prev(uv)`.
- `line_style: "beam"` draws the line as a phosphor beam (the scope's): `width` is the Gaussian sigma in pixels and each segment's brightness falls with its length, so a lingering beam is bright and a racing one dim. The default is `"line"`.
- `band_waveform: true`: when the layer reacts to a band, the waveform itself is band-filtered (the scope), not only the levels and spectrum.
- A choice may list `"values"`, one per option: what it saves, when that differs from the option's name (the scope's "Key and speed" saves `"key"`).
- `line_count: "<param id>"` draws only as many of the line's points as that number control says (Spiral's Points); `point(i)` is called for those only.
- A visual with a line may leave out `fn shade`: the line is then the whole picture and the full-screen pass is skipped (Harmonograph 0.19 → 0.05 ms).
- `line: N` (2 to 65536) draws a line through the N points that `fn point(i: u32) -> LinePoint` gives (`pos` centred, `width` in pixels with 0 breaking the line, `colour`), added over `shade`'s picture. `LINE_POINTS` is N. Required with a `point` function and refused without one.
- `items: N` (up to 16384) runs `fn prepare(i: u32) -> vec4<f32>` from `visual.wgsl` once a frame for each `i < N`, before `shade`; `shade` reads the results with `item(i)`. Inside `prepare`, `item(i)` is last frame's value, so items can keep state. `ITEMS` is N in the shader. Required with a `prepare` function and refused without one.

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
| `V.swell` | bass above its average plus low onsets, fast attack, slow release |
| `V.key`, `V.quiet` | dominant pitch class (C = 0), 1 in a quiet passage |
| `band(i)`, `band_raw(i)`, `band_at(t)` | 32 bands low to high, smoothed or raw; `band_at` interpolates t 0..1 |
| `chroma(i)` | 12 pitch classes, C first |
| `wave(t)`, `wave_mono(t)` | the last 1024 samples (t 0 oldest .. 1 newest), auto-gained |
| `sample(i)`, `V.wave_new`, `V.auto_gain`, `V.sample_rate` | sample i of the last 4096, newest first; how many arrived since last frame (a trace draws exactly those); the gain already applied; samples per second |
| `colour(t)` | the layer's palette at t (moving with its cycle) over its tint |
| `prev(uv)` | last frame (with `feedback`) |
| `LinePoint`, `LINE_POINTS` | a point of the line (`pos`, `width` px, `colour`) and how many there are |
| `art(uv)`, `V.has_art` | the playing track's album art (512 x 512, linear colour, y down; black and 0 without art) |
| `item(i)`, `ITEMS` | the prepare step's item `i` (this frame's in `shade`, last frame's in `prepare`), and how many |
| `centred(uv)`, `uncentred(p)`, `pixel()`, `TAU` | coordinates (short side's half-extent 1, y up) and back to uv, one pixel in them |
| `rotate(p, a)`, `noise(p)` | turn a point by `a` radians; smooth value noise 0..1 |
| `hash21`, `hash22`, `ign` | hashes and interleaved gradient noise from the shared prelude |

A loaded visual sees only the shared `params.wgsl` prelude and this API, not the built-in visuals' own helpers, so their names never collide with yours. `cargo test -p audioviz factory_visuals_compile` compiles every factory visual the way the engine does.

Factory examples, each a worked example in the guide: **Polygon** (a still shape), **Pulse** (bands, beats, onsets, the music clock), **Comet** (feedback trails), **Eclipse** (a prepare step), **Harmonograph** (a line, with state in a prepare step). **Loop** (`assets/visuals/loop/`) is the fifth: up to 120 waveform rings of 256 points kept in 7802 prepare items (a ring clock every item works out the same way, four columns per item, one level per ring from last frame's values), the repeated smoothing passes folded into one precomputed kernel per pass count, and every ring drawn as one line broken between rings (0.09 ms compiled in, 0.13 loaded). **Phosphor scope** (`assets/visuals/scope/`) is the fourth: feedback for the phosphor, a beam-style line through exactly the samples that arrived this frame (from last frame's final one, so the trace is continuous), Frequency colouring from zero crossings in the shader (0.11 ms either mode; the compiled-in one did the crossings on the CPU). Shipped visuals whose folder is missing or broken load from the copies inside the app, so the scope, every missing visual's stand-in, is always there. **Spiral** (`assets/visuals/spiral/`) is the third port, a line visual with its drift, turn and beat kick in one prepare item (0.26 ms compiled in, 0.20 loaded). **Spectrum wheel** (`assets/visuals/wheel/`) is the second port of a compiled-in visual: its CPU state (turn, beat jumps, the walking band offset, smoothed wedge levels) lives in a prepare step, item 0 for the wheel and one item per wedge holding its finished colour.

How the prepare step runs: a compute pass (`cs_main`, generated after the visual's code, 64 per workgroup) writes one of two item buffers while reading the other through `item()`; the fragment pass then reads the one just written. Eclipse moved its dot levels, positions and colours there: 1.1 → 0.69 ms at 16 dots and 4.5 → 2.7 at 64 (2560x1440, release), under the old compiled-in Eclipse's 0.8 and 3.5.

The album art is resampled on the CPU to 512 x 512 once per track (`image`'s triangle filter) and written into an sRGB texture each loaded visual owns; `has_art` took the uniform's old pad slot, so the layout did not move.

How the line runs: a compute pass (`cs_line`, after `cs_main`) writes each `point(i)` as two vec4 into a storage buffer; the engine's own `visual_lines.wgsl` draws instance i as the quad from point i to i + 1 (soft edge, round caps at half strength so joins do not double, colour and width blended along it), additively into the visual's output after the `shade` pass. Factory example: **Harmonograph** (6000 points, 0.19 ms).

What a loaded visual cannot do yet: draw meshes or particles (Ribbons and Crystal stay built in), or read pictures other than the album art. Each is a possible extension of the format.
