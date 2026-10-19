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
- Up to 16 `params`. `type` is `slider` (the default), `integer`, `toggle` or `choice` (three options or fewer show as a segmented control, more as a dropdown). Param ids are lowercase identifiers, not WGSL words. `group` gathers controls under a heading, in order of first use. Values are saved in the layer as `"<id>": { "radius": 0.5, ... }`.
- `feedback: true` keeps last frame's picture for `prev(uv)`.

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
| `colour(t)` | the layer's palette at t (moving with its cycle) over its tint |
| `prev(uv)` | last frame (with `feedback`) |
| `centred(uv)`, `uncentred(p)`, `pixel()`, `TAU` | coordinates (short side's half-extent 1, y up) and back to uv, one pixel in them |
| `rotate(p, a)`, `noise(p)` | turn a point by `a` radians; smooth value noise 0..1 |
| `hash21`, `hash22`, `ign` | hashes and interleaved gradient noise from the shared prelude |

A loaded visual sees only the shared `params.wgsl` prelude and this API, not the built-in visuals' own helpers, so their names never collide with yours. `cargo test -p audioviz factory_visuals_compile` compiles every factory visual the way the engine does.

Factory examples, each a worked example in the guide: **Polygon** (a still shape), **Pulse** (bands, beats, onsets, the music clock), **Comet** (feedback trails).

What a loaded visual cannot do yet: keep CPU-side state (everything is a function of the inputs above and, with feedback, last frame), draw meshes, lines or particles (Loop, Spiral, Ribbons and Crystal stay built in), or read the album art. Each is a possible extension of the format; the most useful first is a per-frame `prepare` step that computes per-item values once (the built-in Eclipse did its dot levels on the CPU, which is why the loaded one costs 1.1 ms against 0.8 at 16 dots and 2560x1440).
