# ADR-0003 — The lab: one object, still, rendered by the engine

- **Status:** Accepted 2026-10-19 (Greg: "something akin to Blender, but for us (AI and me) to quickly iterate over our designs"). Increment 1 done (0.4.6), the window of increment 3 (0.4.7); building the rest in increments.
- **Context:** Designing the jellies in the app is slow and cannot be compared. A change to a visual costs a release build (the visuals are embedded, 35 to 60 s), a D3D12 shader build (the Jellyfish: 10 s on FXC, 2 s on Vulkan), a start and about 20 s for the exposure to settle, and the frame that comes out depends on a random layout, the build time and whatever music is playing, so a before and an after never show the same thing. Greg wants a tool to render **just the object being worked on**, held still if need be, to focus on its shaders and design, with **layers** as the highest return: seeing a design one part at a time and stacked back up, as Blender's render passes do.
- **Reads:** ADR-0002 (the shared space the lab draws in), `docs/visuals.md`.

## Decisions

### 1. The engine, not a second renderer

The look comes from the engine's code: the visual's `paint` and `shade`, the shared space, diffusion, levelling and the Picture. A renderer of its own would drift from the app, and every shader feature would be built twice. So the app crate becomes a library with two programs: `audioviz` (the app, unchanged) and `audioviz-lab`. The lab is a developer tool; the installer ships only `audioviz.exe`.

### 2. One object, still, on a clock of its own

- The lab builds one layer of the visual in a shared space, with the visual's params from the command line (for the Jellyfish: one jelly, no snow, no water).
- Time is the lab's: fixed steps of 1/60 s, silence for the music, `AUDIOVIZ_SEED`'s fixed instance. It runs a few seconds to let the object settle (a jelly's strands hang), then holds time: every shot is the same frame.
- `--at <seconds>` picks when to hold, so a stroke can be shown open, squeezing or flared.
- Vulkan by default (2 s builds against 10), D3D12 with `--dx12` to check that the two agree.

### 3. The camera is the lab's

The lab writes the space's camera after the visual's own camera pass. A shot nudges it: `yaw`, `pitch` (degrees, added to the visual's camera) and `zoom` (a factor on its distance). A visual with `fn camera` keeps aiming it (the Jellyfish's Close-up looks at its jelly); the lab only turns it round that point.

### 4. Layers

Two kinds, shown one at a time or stacked:

- **The engine's passes**: the final picture; the sharp light (the space before diffusion); the glow alone (what diffusion adds).
- **A visual's design layers**: a manifest names them (`"layers": ["Glass", "Edges", "Organs", ...]`, up to 16) and the code asks `layer_on(i)`, which is always true in the app. In the lab, `--layers` shows only those named. The visual decides what a layer is, so it costs a visual nothing until it names some.

### 5. Output

- **Batch** (first): `audioviz-lab <visual> --set id=value ... --shots front,side,below --layers all` renders off-screen and writes each shot as a PNG and a contact sheet of shots by layers into `--out` (default `tmp/lab/`). The same arguments give the same pixels, so two sheets compare honestly. It prints the time the visual took to build and to draw.
- **Interactive** (next): a window on the same core, the camera orbiting with the mouse, the visual's sliders from the panel code, the layers as toggles, reloading the visual's folder on save.

## Increments

1. **The library and the batch lab**: the crate split, the lab's clock, silence, camera nudge and engine passes, PNGs and a contact sheet. Exit: the moon jelly, one, still, from three sides, in under 10 s from the command, identical over two runs.
2. **Design layers**: `"layers"` and `layer_on(i)`; the Jellyfish names its own.
3. **Interactive**: the window, orbit, sliders, layer toggles, reload on save.
4. Later: debug views (light depth, wireframe, a visual's own coordinates), reference images in tests.

### Increment 1, done (0.4.6, 2026-10-19)

- The crate is a library (`src/lib.rs`, every module public) with two programs: `audioviz` (`src/main.rs`, unchanged) and `audioviz-lab` (`src/bin/lab.rs`). `scripts/release.ps1` and the installer pick `audioviz.exe` by name, so the lab is not shipped.
- `AUDIOVIZ_REPO=<checkout>` makes any build read that checkout's visuals (`repo_folder`) and engine shaders (`ShaderSet::load`), watching them; the lab sets it to the checkout it was built from, with `AUDIOVIZ_DEFAULTS` and `AUDIOVIZ_SEED`.
- Engine hooks, inert in the app: `Director::camera_nudge` (a one-thread compute pass, `CameraNudge`, turns the space's camera after the lead's camera pass) and `Director::pass` (`SpacePass`: all, sharp, glow).
- `--scene` takes a factory scene's picture, tint and the visual's layer from the checkout's `assets/scenes/`; `--set` overrides params. The exposure is fixed by default (`--exposure 1`, levelling and auto exposure off): a lone object filling the frame was lifted until it bleached. `--exposure auto` levels it as the app does.
- **Measured:** one moon jelly, three shots by three passes at 1600x1000: 4.1 s from the command with a cold shader cache (built in 1.7 s on Vulkan), under 1 s warm. Two runs wrote byte-identical PNGs.

### The window, done ahead of design layers (0.4.7, 2026-10-19)

Greg, on the first sheets: the exposure, bloom and glow blew everything out, and he asked for a window to turn the object round instead of only PNGs. So:

- **Design defaults:** bloom 0 (`--bloom` to add it), grain 0, a fixed exposure; the glow stays, and the passes show it alone or without it. (His first sheet had been levelled by auto exposure; a lone jelly filling the frame bleached.)
- **Pan:** the camera nudge gained a slide of what it looks at (right, up, in units of the camera's distance before the zoom, so zooming keeps the framing): `--pan x,y`. The Close-up aims below the bell for its tentacles; `--pan 0,0.15` centres a moon jelly.
- **`--window`:** the same core in a winit window on Vulkan. Left-drag orbits, right- or middle-drag pans, the wheel zooms; 1 2 3 pick the pass; Space runs or holds time, `.` steps one frame; R resets; S saves the window's picture as a PNG. It polls the visual's folder and the engine shaders and reloads what was saved. The title shows the pass, the time, the view and the frame's GPU time. Checked with a scripted drag and wheel: yaw 60, pitch 24, zoom 0.81.
- **`scripts/lab-jelly.ps1 -Species <moon|nettle|crystal|atolla|comb|lion>`** opens one jelly of a species in its factory scene's colours (`-Png` for a sheet; further arguments go to the lab).
- Still to come in increment 3: the visual's sliders and the layer toggles in the window.
