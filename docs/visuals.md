# Visuals: the registry and how to add one

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

## Next: visuals loaded from a folder

The registry is the base for visuals that are not compiled in: a folder with a `visual.json` (names, credit, parameters with ranges) and a `visual.wgsl` (one shader function with a standard prelude of time, audio, palette, tint and parameters), found at start, listed with the built-ins, its panel built from the parameter list. Being worked on; see the progress log in `docs/phase6-ui.md`.
