# Phase 7 — Controls: information architecture for the panel

> [!NOTE]
> A working log, kept as history: statuses and plans below are as they were written. What is open now is in [`ROADMAP.md`](ROADMAP.md); what the project is, in the [README](../README.md).

Status: **built 2026-10-17 (same day), all four steps, local only.** Greg chose tabs, modifiers opening in place, and the rename to Picture. What shipped differs from the plan in small ways, noted at the end.

## Why

Greg, after the Sigil shapes pass: "the interface definitely needs work when it comes to adjusting the dials. It's just starting to collide and mash together and hard to see what set of controls you're working on from what modifier or etc."

He is right, and the cause is structural, not cosmetic. The Layers section was designed when a layer was a visual plus an opacity. Since then a layer has grown a band, a tint mode, a palette with a stop editor, an ordered chain of up to six modifiers each with four to eight controls, and visuals with up to 22 controls (the Sigil). All of it renders as **one scrolling column, 320 px wide**, in this order:

```
Layers header + hint
layer rows (switch · name · arrows), opacity slider under each on layer
─────
LAYER 1
Visual      [scope] [ribbons]
            [starfield] [kaleido] [sigil]        ← 5 pills wrap to 2 lines
Reacts to   [full mix] [sub]
            [bass] [low mid] [mid]
            [presence] [air]                     ← 3 lines
Tint        [scene] [album art]
            [own colour]                         ← 2 lines
PALETTE     [none] [phosphor] [techno]
            [neon] [rave] [80s pop] …            ← 4 lines, then strip, sliders, stop editor
MODIFIERS   rows… add row… selected modifier's controls (no header saying which)
─────
SIGIL       22 controls
SCENE TINT  (a scene property, living in the layer section)
```

Counted today: 74 controls in `panel.rs`, 6 to 22 per visual, 4 to 8 per modifier. Four pill groups alone cost eleven rows before the first slider. There is no persistent answer to "which layer, which modifier, which visual am I on": the only cue is a small-caps label that scrolls away.

## Principles

1. **One object in focus.** The panel is an *inspector*: it shows the controls of exactly one thing (a layer's visual, a layer's colour, one modifier, the scene's picture) under a header that never scrolls away and says what that thing is.
2. **Stack first, details second.** The composition (which layers are on, in what order, how loud) is always visible at the top of Layers in a compact form; details live below under tabs.
3. **Progressive disclosure.** Controls come in named groups that collapse; a group remembers whether it is open. Defaults are quiet: a control at its default is dimmed, so a glance shows what has been touched.
4. **Pick controls by cardinality.** Two or three options → a segmented control on one line. Four or more → a dropdown. Pills that wrap are gone.
5. **Stable positions.** A control never moves because another control changed; dependent controls (photos folder, custom palette stops) appear *below* their parent in the same place every time.
6. **Colour as a map.** Each layer slot has a fixed hue (1 green, 2 blue, 3 amber) used on its row, its bar pip and the inspector header stripe. You know which layer you are in without reading.
7. **Keyboard parity stays.** 1-3 pick the layer, Space / A / L / H / N / Tab as today; add Esc to step back out of a modifier to its layer.

## The structure

Rail sections stay: Scenes · Layers · Picture (was Post) · Sync · Display · Meters · About.

### Layers

```
┌ Layers ─────────────────────────────── Hide UI (H) ┐
│ STACK                                               │
│ ▌● 1 sigil        bass   ▮▮▮▮▮▮▮▮▮▯ 1.00  ▲▼       │  ← row: hue bar, switch, name, band chip,
│ ▌  fold · lumi                                       │     inline opacity, arrows; chips = chain
│ ▌● 2 ribbons             ▮▮▮▮▮▯▯▯▯▯ 0.50  ▲▼       │
│ ▌○ 3 scope                                  ▲▼       │
├─────────────────────────────────────────────────────┤
│ ▌ LAYER 1 · SIGIL                        [ Visual | Colour | Effects ]   ← sticky header
│                                                     │
│  (Visual tab)                                       │
│  Visual        [ Sigil                          ▾ ] │  ← dropdown
│  Reacts to     [ bass  60–250 Hz                ▾ ] │  ← dropdown with presets + custom
│  ▾ Look                                              │  ← collapsible groups
│     Size ───────────────○──── 0.80                   │
│     Spread ─────────○──────── 1.00                   │
│     …                                                │
│  ▸ Elements (8)                                      │  ← collapsed, count of controls
│  ▸ Structure (3)                                     │
│  ▸ Motion (3)                                        │
│  Reset sigil                                         │
└─────────────────────────────────────────────────────┘
```

**Colour tab:** Tint `[scene | album art | own]` segmented, colour picker when own; Palette `[ neon ▾ ]` dropdown with the strip preview beside it, Amount / Cycle / Reacts / Wrap; `▸ Custom stops` collapsed unless the preset is custom.

**Effects tab:** the chain as rows (switch · name · ▲▼ · ×), one `[ + add modifier ▾ ]` dropdown. Selecting a row pushes a **second-level header**:

```
│ ▌ LAYER 1 · SIGIL › LUMINESCENCE              ‹ back │
│     Afterglow ─────○──── …                           │
│     …                                                │
```

The layer stripe keeps its hue so the modifier is visibly *inside* layer 1. Esc or ‹ back returns to the Effects tab. The stack rows' chain chips jump straight to a modifier.

### Picture (was Post)

Exposure, auto exposure, bloom, grain **and the scene tint** (tint picker, presets, tint from album art). Tint is a scene-wide property and belongs with the other scene-wide picture settings, not under a layer. Header reads "Picture · saved with the scene".

### Scenes

Unchanged in content: director, built-in list, yours, save, folder. Two small changes: the list becomes a single-column list with the current scene marked, and "unsaved scene" on the bar gets a Save affordance in the Scenes header.

### Controls

- **Slider row** stays (label left, value right, thin track). Adds: dim at default, double-click resets to default, shift-drag for fine steps, and the value becomes editable on click.
- **Group header**: small caps + chevron + control count when collapsed; click toggles; open state saved per (section, group) in settings (`ui_open: BTreeMap<String,bool>`), not per layer.
- **Dropdown**: egui `ComboBox` restyled to the theme (hairline, accent on hover, mono value).
- **Segmented**: three-cell pill row with a sliding accent cell.
- **Panel width** 320 → 352 px so a slider keeps a usable track beside a 5-char value. 1280 px windows keep 928 px of picture; the portrait monitor is unaffected.

## Build order

1. **Inspector skeleton** (biggest win, mostly mechanical): sticky header with layer hue, the three tabs, dropdowns for Visual / Reacts to / Palette, segmented Tint. Modifier controls get the second-level header and back. Scene tint moves to Picture. *Gate:* every control reachable today is reachable, screenshots of each tab at 1280x800.
2. **Groups + quiet defaults**: collapsible groups with saved state; dim-at-default and double-click reset on slider rows; visuals' `ui()` emit groups through a small `Groups` helper so all five visuals read alike. *Gate:* Sigil's 22 controls fit in one screen with Look open.
3. **Stack rows**: inline opacity, band chip, chain chips, hue bar; bar pips take the hue. *Gate:* drag, arrows and chips hand-tested.
4. **Polish**: panel width, editable values, shift-drag, Esc-back, Scenes list tidy.

Each step is one commit and one `AUDIOVIZ_PANEL=` screenshot set. Nothing in settings is renamed; `editing` and the modifier selection stay as they are, plus the open-state map.

## Open questions for Greg

- Tabs (Visual · Colour · Effects) or one page with collapsible groups for all three? Tabs are proposed because Effects alone can be a page.
- Should a modifier open *in place* (pushing the layer page) as proposed, or in a side panel to the left of the inspector? In place keeps one column and works at 1280 px; a second column needs ~700 px of UI.
- Picture for the renamed Post section, or keep Post?

## What was built (2026-10-17)

- `widgets.rs`: `slider_def` (default-aware slider row: dimmed label at the default, double-click on the slider resets, the value is an egui `DragValue` in mono so it drags for fine steps and takes typed numbers), `segmented`, `dropdown` (restyled `ComboBox` with a fixed label column), `group` (collapsible, open state in a process-wide map seeded from `Settings.ui_open` and stored back every frame), `chip`, `inspector_header` (hue bar + breadcrumb + action). `slider_row` still exists for controls without a meaningful default (band cuts, audio delay, segment counts).
- `theme.rs`: `LAYER_HUES` (green, blue, amber) and `layer_hue(i)`; `PANEL_W` 320 → 352.
- `settings.rs`: `Section::Picture` (serde alias "Post" so saved files still load), `LayerTab`, `Settings.layer_tab`, `Settings.ui_open`.
- `panel.rs`: Layers is now **stack + inspector**. The stack rows carry the hue bar (also the drag handle), switch, name, band chip, an inline 64 px opacity slider in the hue, order buttons, and the modifier chain as chips (click one to open that modifier). The inspector header is a breadcrumb (`LAYER 1 · SIGIL`, or `LAYER 1 · SIGIL › KALEIDOSCOPE ‹ back`) that never scrolls; under it the tab bar and a scroll area for the body. Visual tab: Visual and Reacts-to dropdowns (custom range exposes the cut sliders), then the visual's grouped controls. Colour tab: tint as a segmented control, palette as a dropdown with the strip, Amount / Cycle / Reacts, wrap, custom stops in a group with a "Start from" dropdown. Effects tab: the chain rows with `›` to open, arrows, remove, and an Add dropdown that opens the new modifier at once. A modifier page replaces the tabs in place, with On, its controls, Remove and Back; Esc also goes back (`app.rs`, before the panel closes). Picture = post stack with defaults plus the scene tint. Scenes list rows are full width with a marker dot.
- Visuals: every `ui()` uses `slider_def` with its params' defaults and `group`s (scope: Beam / Trace; ribbons: Field / Look; black hole: Lens and disk / Sky; sigil: Look / Elements / Structure / Motion; kaleidoscope stays flat with a segmented picture source). Modifier controls use defaults too; their pill rows are segmented controls.
- `chrome.rs`: bar pips take the layer hue.
- Dev: `AUDIOVIZ_MOD=n` opens modifier n's page for screenshots; `AUDIOVIZ_PANEL=Picture`.

Differences from the plan: no shift-drag (the editable DragValue covers fine steps); the Scenes header has no Save affordance (the save block is one scroll away and the bar already says "unsaved scene"); the focused stack row is highlighted with its hue at low alpha rather than only the bar.

## Second pass: the rows themselves (2026-10-17)

Greg: "most important are controls and tuners and sliders." Looking at the panel at 1:1, a slider row cost two lines (label + value, then the slider), so seven controls filled a screen; the rail was invisible on the panel colour, the handle was a plain 17 px disc, and nothing marked the default. Now (`widgets::slider_def`):

- **One line per control**: label column 108 px (truncated with a tooltip when long), the slider, then the editable value in a 48 px mono column. Rows are 31 px, so a group of eight controls fits in a quarter of the panel. The panel is 380 px wide to keep a 120 px track.
- **Rail** drawn in a visible grey with the travelled part in the accent; handle 13 px.
- **Default tick** on the rail at the default value (log ranges mapped properly), label dimmed while the value sits there, double-click on the slider returns to it.
- **Hover**: the row gets a faint raised background (one frame behind) and the label brightens, so the control under the pointer is unambiguous. Toggle rows got the same hover.
- Dropdown rows share the label column width.

## Third pass: missing options (2026-10-17)

Greg: "not all the sliders are there for certain things." An audit of every params struct against the panel found no field without a control, but three real gaps and one false impression:

- **Groups were collapsed by default** (Elements, Structure, Motion, Sky), so a visual looked like it had six controls. Every group now opens by default; the remembered state only closes what you close.
- **Per-modifier Mix** (wet/dry) did not exist. Every modifier now has `mix` (default 1): the director blends the pass's output back toward its input with the existing mix shader, using a third ping texture so the input is never the texture being written. Costs one fullscreen pass per modifier only when Mix is below 100%. Old settings files load with 1.
- **Reset on a modifier page** (keeps the On state) next to Remove and Back; `Modifier::reset`.
- **Solo** on each stack row (the `S` chip): the director draws only that layer while tuning it. Never saved (`Settings.solo` is skipped), a warning line under the stack clears it.
- Stack rows show the opacity value in mono beside the slider. Segmented controls on modifier pages carry a label (Mirrors, Direction, Recursion, Map, Shape). Integer sliders (Segments, Step on beats, Depth) have defaults and ticks too.
