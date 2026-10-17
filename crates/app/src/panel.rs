//! The settings panel: a right-docked sheet with an icon rail, one section at
//! a time. Sections: Scenes (compositions), Layers (the three slots and the
//! visual being edited), Post, Sync, Display, Meters (developer), About.

use audioviz_analysis::FeatureFrame;
use audioviz_audio::CaptureMode;
use egui::{Align2, Color32, Rect, Sense, pos2, vec2};

use crate::overlays::CalibrationView;
use crate::scene::Scene;
use crate::scene_store::SceneDef;
use crate::settings::{AudioBand, FoldMode, FractalMode, ImageSource, MAX_LAYERS, MAX_MODS, MaskShape, Modifier, Section, Settings, TintMode, VisualKind};
use crate::theme::{self, ACCENT, BAR_H, ERROR, HAIRLINE, PANEL_W, RAIL_W, TEXT, TEXT_2, TEXT_3};
use crate::widgets::{self, ghost_button, hint, section_header, slider_row, small_caps, toggle_row};

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub hz: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate(String),
    Available {
        tag: String,
        url: String,
    },
    Failed(String),
}

pub struct Stats {
    pub fps: f32,
    pub points_per_sec: f32,
    pub capture: CaptureMode,
    pub shader_error: Option<String>,
    pub sample_rate: u32,
    pub monitors: Vec<MonitorInfo>,
    pub fullscreen: bool,
    pub settings_path: String,
    pub fading: bool,
    pub last_cue: &'static str,
    pub now_playing: Option<String>,
    pub calibration: Option<CalibrationView>,
    pub scenes: Vec<SceneDef>,
    pub update: UpdateStatus,
    pub diagnostics: String,
    pub on_battery: Option<bool>,
}

#[derive(Default)]
pub struct Actions {
    pub toggle_fullscreen: bool,
    pub select_layer: Option<usize>,
    /// Drag-reorder: move layer `.0` to position `.1`.
    pub move_layer: Option<(usize, usize)>,
    /// (layer, from, to): a modifier was dragged within its chain.
    pub move_modifier: Option<(usize, usize, usize)>,
    /// (layer, index): a modifier was removed.
    pub remove_modifier: Option<(usize, usize)>,
    pub apply_scene: Option<String>,
    pub save_scene: Option<String>,
    pub delete_scene: Option<String>,
    pub calibrate_toggle: bool,
    pub calibrate_apply: bool,
    pub open_url: Option<String>,
    pub open_scenes_folder: bool,
    pub check_updates: bool,
    pub copy_diagnostics: bool,
    pub launch_spotify: bool,
    pub set_target: Option<String>,
    pub show_shortcuts: bool,
    pub rerun_first_run: bool,
    pub hide_ui: bool,
}

/// Scratch UI state that is not a setting.
#[derive(Default)]
pub struct PanelState {
    pub scene_name: String,
    /// Which modifier of the edited layer shows its controls.
    pub editing_mod: usize,
    pub target_edit: String,
    pub target_editing: bool,
}

pub fn draw(ctx: &egui::Context, settings: &mut Settings, state: &mut PanelState, f: &FeatureFrame, stats: &Stats, layers: &mut [Box<dyn Scene>], t: f32) -> Actions {
    let mut a = Actions::default();
    if t <= 0.001 {
        return a;
    }
    let screen = ctx.content_rect();
    let x = screen.max.x - PANEL_W * t;
    // The panel slides in at full width (clipped at the screen edge) rather
    // than growing: widgets never see a width below what they need.
    let rect = Rect::from_min_max(pos2(x, screen.min.y + BAR_H), pos2(x + PANEL_W, screen.max.y));

    egui::Area::new(egui::Id::unique("panel")).fixed_pos(rect.min).order(egui::Order::Middle).show(ctx, |ui| {
        ui.set_min_size(rect.size());
        ui.set_clip_rect(rect.intersect(screen));
        let p = ui.painter().clone();
        p.rect_filled(rect, 0.0, theme::panel_fill());
        p.vline(rect.min.x + 0.5, rect.y_range(), egui::Stroke::new(1.0, HAIRLINE));
        p.vline(rect.min.x + RAIL_W + 0.5, rect.y_range(), egui::Stroke::new(1.0, HAIRLINE));
        ui.interact(rect, egui::Id::unique("panel-bg"), Sense::click_and_drag());

        // Rail.
        let mut y = rect.min.y + 28.0;
        for s in Section::ALL {
            if s == Section::Meters && !settings.developer {
                continue;
            }
            let r = Rect::from_center_size(pos2(rect.min.x + RAIL_W * 0.5, y), vec2(RAIL_W, 36.0));
            let resp = ui.interact(r, egui::Id::unique(("rail", s.name())), Sense::click()).on_hover_text(s.name());
            let active = settings.section == s;
            let color = if active { ACCENT } else if resp.hovered() { TEXT_2 } else { TEXT_3 };
            if active {
                p.rect_filled(Rect::from_min_size(pos2(rect.min.x, y - 12.0), vec2(2.0, 24.0)), 0.0, ACCENT);
            }
            widgets::rail_icon(&p, r.center(), s, color);
            if resp.clicked() {
                settings.section = s;
            }
            y += 40.0;
        }

        // Content.
        let content = Rect::from_min_max(pos2(rect.min.x + RAIL_W + 20.0, rect.min.y + 20.0), pos2(rect.max.x - 20.0, rect.max.y - 48.0));
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(content).layout(egui::Layout::top_down(egui::Align::Min)));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut cui, |ui| {
            ui.set_width(content.width() - 8.0);
            match settings.section {
                Section::Scenes => scenes_section(ui, settings, state, stats, &mut a),
                Section::Layers => layers_section(ui, settings, state, stats, layers, &mut a),
                Section::Post => post_section(ui, settings),
                Section::Sync => sync_section(ui, settings, state, stats, &mut a),
                Section::Display => display_section(ui, settings, stats, &mut a),
                Section::Meters => meters_section(ui, f, stats),
                Section::About => about_section(ui, stats, &mut a),
            }
        });

        // Footer.
        let fy = rect.max.y - 44.0;
        p.hline((rect.min.x + RAIL_W + 20.0)..=(rect.max.x - 20.0), fy, egui::Stroke::new(1.0, HAIRLINE));
        p.text(pos2(rect.min.x + RAIL_W + 20.0, fy + 14.0), Align2::LEFT_CENTER, "H hide all   N now playing   ? keys", theme::mono(10.5), TEXT_3);
        p.text(pos2(rect.min.x + RAIL_W + 20.0, fy + 30.0), Align2::LEFT_CENTER, "1-3 layer   Space scene", theme::mono(10.5), TEXT_3);
        p.text(pos2(rect.max.x - 20.0, fy + 30.0), Align2::RIGHT_CENTER, crate::VERSION, theme::mono(10.0), TEXT_3);
        let sc = ui.interact(Rect::from_min_size(pos2(rect.min.x + RAIL_W + 20.0, fy + 6.0), vec2(220.0, 16.0)), egui::Id::unique("footer-shortcuts"), Sense::click());
        if sc.clicked() {
            a.show_shortcuts = true;
        }
    });
    a
}

fn scene_button(ui: &mut egui::Ui, label: &str, active: bool, width: f32) -> bool {
    let b = egui::Button::new(egui::RichText::new(label).color(if active { ACCENT } else { TEXT })).fill(if active { theme::with_alpha(ACCENT, 30) } else { Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if active { ACCENT } else { HAIRLINE })).min_size(vec2(width, 26.0));
    ui.add(b).clicked()
}

fn scenes_section(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, stats: &Stats, a: &mut Actions) {
    if section_header(ui, "Scenes", None, Some("Hide UI (H)")) {
        a.hide_ui = true;
    }
    hint(ui, "A scene is a mix of up to three layers plus post and tint. Space cycles; A lets the director pick on track changes and drops.");
    ui.add_space(8.0);
    toggle_row(ui, "Auto director (A)", &mut settings.auto_scenes);
    hint(ui, &format!("Last cue: {}{}", stats.last_cue, if stats.fading { " · dissolving" } else { "" }));
    if let Some(np) = &stats.now_playing {
        hint(ui, np);
    }
    ui.add_space(8.0);
    small_caps(ui, "Built in");
    for def in stats.scenes.iter().filter(|s| s.builtin) {
        let active = settings.scene_name == def.name;
        if scene_button(ui, &def.name, active, 150.0) {
            a.apply_scene = Some(def.name.clone());
        }
    }
    let user: Vec<&SceneDef> = stats.scenes.iter().filter(|s| !s.builtin).collect();
    if !user.is_empty() {
        small_caps(ui, "Yours");
        for def in user {
            let active = settings.scene_name == def.name;
            ui.horizontal(|ui| {
                if scene_button(ui, &def.name, active, 150.0) {
                    a.apply_scene = Some(def.name.clone());
                }
                if ui.add(egui::Button::new(egui::RichText::new("delete").size(11.0).color(TEXT_3)).frame(false)).clicked() {
                    a.delete_scene = Some(def.name.clone());
                }
            });
        }
    }
    if !settings.scene_name.is_empty() && !stats.scenes.iter().any(|s| s.name == settings.scene_name) {
        hint(ui, &format!("'{}' (edited, not saved)", settings.scene_name));
    }
    ui.add_space(10.0);
    small_caps(ui, "Save current as");
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut state.scene_name).desired_width(150.0).hint_text("name"));
        if ui.add_enabled(!state.scene_name.trim().is_empty(), egui::Button::new("Save")).clicked() {
            a.save_scene = Some(state.scene_name.trim().to_string());
        }
    });
    if !settings.scene_name.is_empty() && stats.scenes.iter().any(|s| s.name == settings.scene_name && !s.builtin) {
        let name = settings.scene_name.clone();
        if ghost_button(ui, &format!("Update '{name}'"), false).clicked() {
            a.save_scene = Some(name);
        }
    }
    ui.add_space(6.0);
    if ghost_button(ui, "Open scenes folder", false).clicked() {
        a.open_scenes_folder = true;
    }
    hint(ui, "Each scene is a JSON file. Copy one to share it; drop one in to import.");
}

/// Payload type for dragging a modifier row (distinct from the layer rows).
#[derive(Clone, Copy)]
struct ModDrag(usize);

fn pill(ui: &mut egui::Ui, name: &str, active: bool) -> bool {
    ui.add(egui::Button::new(egui::RichText::new(name).color(if active { ACCENT } else { TEXT })).fill(if active { theme::with_alpha(ACCENT, 30) } else { Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if active { ACCENT } else { HAIRLINE }))).clicked()
}

fn mini_switch(ui: &mut egui::Ui, on: &mut bool) {
    let (sw, resp) = ui.allocate_exact_size(vec2(24.0, 14.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, theme::ANIM_S);
    ui.painter().rect_filled(sw, 7.0, widgets::lerp_color(HAIRLINE, ACCENT, t));
    ui.painter().circle_filled(pos2(sw.min.x + 7.0 + 10.0 * t, sw.center().y), 5.0, widgets::lerp_color(TEXT_2, theme::BG, t));
}

fn drag_handle(ui: &mut egui::Ui, id: egui::Id, payload: ModDrag) {
    ui.dnd_drag_source(id, payload, |ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(14.0, 16.0), Sense::hover());
        let c = if resp.hovered() || resp.dragged() { TEXT } else { TEXT_3 };
        for k in 0..3 {
            let y = r.min.y + 4.0 + 4.0 * k as f32;
            ui.painter().line_segment([pos2(r.min.x + 2.0, y), pos2(r.max.x - 2.0, y)], egui::Stroke::new(1.0, c));
        }
        resp.on_hover_cursor(egui::CursorIcon::Grab);
    });
}

fn modifiers_section(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, editing: usize, a: &mut Actions) {
    let l = settings.layer_mut(editing);
    let n = l.modifiers.len();
    if n == 0 {
        hint(ui, "Modifiers are passes over this layer's picture, run in order: mirrors, echo, polar wrap, ripples, edges, colour fringes, a mask.");
    }
    state.editing_mod = state.editing_mod.min(n.saturating_sub(1));
    let dragging = egui::DragAndDrop::has_payload_of_type::<ModDrag>(ui.ctx());
    let mut remove: Option<usize> = None;
    let mut moved: Option<(usize, usize)> = None;
    for j in 0..n {
        let focused = j == state.editing_mod;
        let row = ui.horizontal(|ui| {
            drag_handle(ui, ui.scope_id().with(("mod drag", editing, j)), ModDrag(j));
            let m = &mut l.modifiers[j];
            mini_switch(ui, m.enabled_mut());
            let label = egui::RichText::new(m.name()).color(if focused { ACCENT } else { TEXT });
            if ui.add(egui::Button::new(label).frame(false)).clicked() {
                state.editing_mod = j;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(egui::RichText::new("×").color(TEXT_3)).frame(false)).on_hover_text("remove").clicked() {
                    remove = Some(j);
                }
            });
        });
        if dragging {
            let rect = row.response.rect;
            let hover = ui.ctx().pointer_hover_pos().is_some_and(|p| rect.contains(p));
            if hover {
                ui.painter().hline(rect.x_range(), rect.min.y - 2.0, egui::Stroke::new(2.0, ACCENT));
                if ui.input(|i| i.pointer.any_released()) {
                    if let Some(from) = egui::DragAndDrop::take_payload::<ModDrag>(ui.ctx()) {
                        if from.0 != j {
                            moved = Some((from.0, j));
                        }
                    }
                }
            }
        }
    }
    if let Some((from, to)) = moved {
        let m = l.modifiers.remove(from);
        l.modifiers.insert(to, m);
        state.editing_mod = to;
        a.move_modifier = Some((editing, from, to));
    }
    if let Some(j) = remove {
        l.modifiers.remove(j);
        a.remove_modifier = Some((editing, j));
    }
    if l.modifiers.len() < MAX_MODS {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("add").color(TEXT_3));
            for (name, make) in Modifier::KINDS {
                if ui.add(egui::Button::new(egui::RichText::new(name).size(11.0)).frame(false)).clicked() {
                    l.modifiers.push(make());
                    state.editing_mod = l.modifiers.len() - 1;
                }
            }
        });
    }
    if let Some(m) = l.modifiers.get_mut(state.editing_mod) {
        ui.add_space(4.0);
        modifier_controls(ui, m);
    }
}

fn modifier_controls(ui: &mut egui::Ui, m: &mut Modifier) {
    match m {
        Modifier::Fold(k) => {
            ui.horizontal(|ui| {
                ui.label("Mirrors");
                for (name, mode) in [("radial", FoldMode::Radial), ("pinwheel", FoldMode::Pinwheel), ("tiled", FoldMode::Tiled)] {
                    if pill(ui, name, k.mode == mode) {
                        k.mode = mode;
                    }
                }
            });
            if k.mode != FoldMode::Tiled {
                let mut seg = k.segments as f32;
                slider_row(ui, "Segments", &mut seg, 1.0..=24.0, |v| format!("{v:.0}"), false);
                k.segments = seg.round() as u32;
                let mut step = k.step_beats as f32;
                slider_row(ui, "Step on beats", &mut step, 0.0..=16.0, |v| if v < 0.5 { "off".into() } else { format!("every {v:.0}") }, false);
                k.step_beats = step.round() as u32;
            }
            slider_row(ui, "Spin", &mut k.speed, -2.0..=2.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Zoom", &mut k.zoom, 0.2..=3.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Apex offset", &mut k.offset, 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Wander", &mut k.wander, 0.0..=4.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Reacts", &mut k.react, 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Spin follows energy and onsets; zoom breathes with the bass. The apex off-centre is what makes real kaleidoscopes rich.");
        }
        Modifier::Feedback(k) => {
            slider_row(ui, "Decay", &mut k.decay, 0.0..=0.985, |v| format!("{v:.3}"), false);
            slider_row(ui, "Zoom", &mut k.zoom, -1.0..=1.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Spin", &mut k.spin, -1.5..=1.5, |v| format!("{v:.2}"), false);
            slider_row(ui, "Reacts", &mut k.react, 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Last frame echoes under this one, zooming and turning. One feedback per layer keeps its own history.");
        }
        Modifier::Polar(k) => {
            ui.horizontal(|ui| {
                if pill(ui, "wrap", !k.unwrap) {
                    k.unwrap = false;
                }
                if pill(ui, "unwrap", k.unwrap) {
                    k.unwrap = true;
                }
            });
            slider_row(ui, "Radius", &mut k.radius, 0.2..=2.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Spin", &mut k.spin, -2.0..=2.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Reacts", &mut k.react, 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Wrap turns left-to-right into around-the-centre: a scope becomes a ring.");
        }
        Modifier::Ripple(k) => {
            slider_row(ui, "Amount", &mut k.amount, 0.0..=0.2, |v| format!("{v:.3}"), false);
            slider_row(ui, "Wavelength", &mut k.wavelength, 0.02..=0.6, |v| format!("{v:.2}"), false);
            slider_row(ui, "Speed", &mut k.speed, 0.1..=3.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Standing wave", &mut k.sine, 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Reacts", &mut k.react, 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Beats launch rings from the centre. Above 1.5 reacts, onsets launch them too.");
        }
        Modifier::Edge(k) => {
            slider_row(ui, "Strength", &mut k.strength, 0.0..=5.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Keep original", &mut k.keep, 0.0..=1.0, |v| format!("{v:.2}"), false);
            toggle_row(ui, "Edges in the layer tint", &mut k.tinted);
            slider_row(ui, "Reacts", &mut k.react, 0.0..=3.0, |v| format!("{v:.2}"), false);
        }
        Modifier::Chroma(k) => {
            slider_row(ui, "Amount", &mut k.amount, 0.0..=0.08, |v| format!("{v:.3}"), false);
            toggle_row(ui, "Radial from the centre", &mut k.radial);
            slider_row(ui, "Reacts", &mut k.react, 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Pulses with treble and onsets.");
        }
        Modifier::Fractal(k) => {
            ui.horizontal(|ui| {
                ui.label("Recursion");
                if pill(ui, "kaleidoscopic IFS", k.mode == FractalMode::Kifs) {
                    k.mode = FractalMode::Kifs;
                }
                if pill(ui, "julia", k.mode == FractalMode::Julia) {
                    k.mode = FractalMode::Julia;
                }
            });
            let mut it = k.iterations as f32;
            slider_row(ui, "Depth", &mut it, 1.0..=8.0, |v| format!("{v:.0}"), false);
            k.iterations = it.round() as u32;
            if k.mode == FractalMode::Kifs {
                slider_row(ui, "Scale per level", &mut k.scale, 1.1..=3.0, |v| format!("{v:.2}"), false);
                slider_row(ui, "Twist", &mut k.angle, -2.0..=2.0, |v| format!("{v:.2}"), false);
                slider_row(ui, "Shift", &mut k.offset, 0.0..=2.0, |v| format!("{v:.2}"), false);
            } else {
                slider_row(ui, "Orbit speed", &mut k.angle, -2.0..=2.0, |v| format!("{v:.2}"), false);
                slider_row(ui, "Orbit size", &mut k.offset, 0.0..=1.0, |v| format!("{v:.2}"), false);
            }
            slider_row(ui, "Zoom", &mut k.zoom, 0.2..=3.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Deeper levels", &mut k.falloff, 0.1..=1.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Reacts", &mut k.react, 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "IFS folds, turns, scales and shifts the picture at every level so it recurs at every size. Julia pulls it into z² + c filaments; the twist and scale breathe with the music.");
        }
        Modifier::Lumi(k) => {
            slider_row(ui, "Fresh glow", &mut k.fresh_gain, 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Fresh release", &mut k.fresh_s, 0.05..=2.0, |v| format!("{v:.2} s"), false);
            slider_row(ui, "Afterglow", &mut k.afterglow_gain, 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Afterglow release", &mut k.afterglow_s, 0.5..=20.0, |v| format!("{v:.1} s"), true);
            slider_row(ui, "Afterglow hue", &mut k.hue_shift, -180.0..=180.0, |v| format!("{v:.0}°"), false);
            slider_row(ui, "UV saturation", &mut k.uv, 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Excite on onsets", &mut k.excite, 0.0..=4.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Sparkle", &mut k.sparkle, 0.0..=3.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Reacts", &mut k.react, 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "A dual phosphor: the picture flashes in its own colour and leaves a long afterglow in a shifted hue; charged areas sparkle on transients like plankton.");
        }
        Modifier::Mask(k) => {
            ui.horizontal(|ui| {
                ui.label("Shape");
                for (name, shape) in [("circle", MaskShape::Circle), ("band", MaskShape::Band), ("hex", MaskShape::Hex)] {
                    if pill(ui, name, k.shape == shape) {
                        k.shape = shape;
                    }
                }
            });
            slider_row(ui, "Size", &mut k.radius, 0.05..=2.0, |v| format!("{v:.2}"), false);
            slider_row(ui, "Softness", &mut k.soft, 0.0..=0.5, |v| format!("{v:.2}"), false);
            toggle_row(ui, "Keep the outside instead", &mut k.invert);
            slider_row(ui, "Reacts", &mut k.react, 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Lets two layers share the frame by place, not only by adding. Size breathes with the bass.");
        }
    }
}

fn layers_section(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, stats: &Stats, layers: &mut [Box<dyn Scene>], a: &mut Actions) {
    if section_header(ui, "Layers", None, Some("Hide UI (H)")) {
        a.hide_ui = true;
    }
    hint(ui, "Every layer that is on is levelled and added to the picture with its opacity. Pick one to edit it below; drag the handle to reorder.");
    ui.add_space(4.0);
    let editing = settings.editing.min(MAX_LAYERS - 1);
    let dragging = egui::DragAndDrop::has_payload_of_type::<usize>(ui.ctx());
    for i in 0..MAX_LAYERS {
        let focused = i == editing;
        let row = ui.horizontal(|ui| {
            // Drag handle: three short lines; the row's index is the payload.
            let drag_id = ui.scope_id().with(("layer drag", i));
            ui.dnd_drag_source(drag_id, i, |ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(14.0, 16.0), Sense::hover());
                let c = if resp.hovered() || resp.dragged() { TEXT } else { TEXT_3 };
                for k in 0..3 {
                    let y = r.min.y + 4.0 + 4.0 * k as f32;
                    ui.painter().line_segment([pos2(r.min.x + 2.0, y), pos2(r.max.x - 2.0, y)], egui::Stroke::new(1.0, c));
                }
                resp.on_hover_cursor(egui::CursorIcon::Grab);
            });
            let l = settings.layer_mut(i);
            let (sw, resp) = ui.allocate_exact_size(vec2(28.0, 16.0), Sense::click());
            if resp.clicked() {
                l.enabled = !l.enabled;
            }
            let t = ui.ctx().animate_bool_with_time(resp.id, l.enabled, theme::ANIM_S);
            ui.painter().rect_filled(sw, 8.0, widgets::lerp_color(HAIRLINE, ACCENT, t));
            ui.painter().circle_filled(pos2(sw.min.x + 8.0 + 12.0 * t, sw.center().y), 6.0, widgets::lerp_color(TEXT_2, theme::BG, t));
            let label = egui::RichText::new(format!("{}  {}", i + 1, l.visual.short())).color(if focused { ACCENT } else { TEXT });
            if ui.add(egui::Button::new(label).frame(false)).clicked() {
                a.select_layer = Some(i);
            }
            if l.band.enabled {
                ui.label(egui::RichText::new(l.band.label()).font(theme::mono(10.5)).color(TEXT_3));
            }
            if focused {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("editing").size(11.0).color(ACCENT));
                });
            }
        });
        // Drop target: the whole row. An accent line marks where the dragged layer lands.
        if dragging {
            let rect = row.response.rect;
            let hover = ui.ctx().pointer_hover_pos().is_some_and(|p| rect.contains(p));
            if hover {
                ui.painter().hline(rect.x_range(), rect.min.y - 2.0, egui::Stroke::new(2.0, ACCENT));
                if ui.input(|i| i.pointer.any_released()) {
                    if let Some(from) = egui::DragAndDrop::take_payload::<usize>(ui.ctx()) {
                        if *from != i {
                            a.move_layer = Some((*from, i));
                        }
                    }
                }
            }
        }
        if settings.layer(i).enabled {
            let l = settings.layer_mut(i);
            slider_row(ui, "    opacity", &mut l.opacity, 0.0..=2.0, |v| format!("{v:.2}"), false);
        }
    }

    ui.add_space(8.0);
    widgets::hairline(ui);
    ui.add_space(8.0);
    small_caps(ui, &format!("Layer {}", editing + 1));
    ui.add_space(4.0);
    // Visual picker.
    ui.horizontal_wrapped(|ui| {
        ui.label("Visual");
        let l = settings.layer_mut(editing);
        for k in VisualKind::ALL {
            let active = l.visual == k;
            if ui.add(egui::Button::new(egui::RichText::new(k.short()).color(if active { ACCENT } else { TEXT })).fill(if active { theme::with_alpha(ACCENT, 30) } else { Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if active { ACCENT } else { HAIRLINE }))).clicked() {
                l.visual = k;
                // The kaleidoscope is its picture plus the fold: add the mirrors with it.
                if k == VisualKind::Kaleido && !l.has_fold() {
                    l.modifiers.insert(0, Modifier::Fold(Default::default()));
                }
            }
        }
    });
    // Audio band.
    ui.add_space(4.0);
    {
        let l = settings.layer_mut(editing);
        ui.horizontal_wrapped(|ui| {
            ui.label("Reacts to");
            let full = !l.band.enabled;
            if ui.add(egui::Button::new(egui::RichText::new("full mix").color(if full { ACCENT } else { TEXT })).fill(if full { theme::with_alpha(ACCENT, 30) } else { Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if full { ACCENT } else { HAIRLINE }))).clicked() {
                l.band.enabled = false;
            }
            for (name, lo, hi) in AudioBand::PRESETS {
                let active = l.band.enabled && (l.band.lo_hz - lo).abs() < 1.0 && (l.band.hi_hz - hi).abs() < 1.0;
                if ui.add(egui::Button::new(egui::RichText::new(name).color(if active { ACCENT } else { TEXT })).fill(if active { theme::with_alpha(ACCENT, 30) } else { Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if active { ACCENT } else { HAIRLINE }))).clicked() {
                    l.band = AudioBand { enabled: true, lo_hz: lo, hi_hz: hi };
                }
            }
        });
        if l.band.enabled {
            slider_row(ui, "Low cut", &mut l.band.lo_hz, 20.0..=16000.0, |v| format!("{v:.0} Hz"), true);
            slider_row(ui, "High cut", &mut l.band.hi_hz, 20.0..=16000.0, |v| format!("{v:.0} Hz"), true);
            if l.band.hi_hz < l.band.lo_hz * 1.2 {
                l.band.hi_hz = l.band.lo_hz * 1.2;
            }
            hint(ui, "The scope draws only this band of the waveform; other visuals take their energy, onsets and spectrum from it.");
        }
    }
    // Tint source.
    ui.add_space(4.0);
    {
        let l = settings.layer_mut(editing);
        ui.horizontal_wrapped(|ui| {
            ui.label("Tint");
            for (name, mode) in [("scene", TintMode::Scene), ("album art", TintMode::Art), ("own colour", TintMode::Custom)] {
                let active = l.tint.mode == mode;
                if ui.add(egui::Button::new(egui::RichText::new(name).color(if active { ACCENT } else { TEXT })).fill(if active { theme::with_alpha(ACCENT, 30) } else { Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if active { ACCENT } else { HAIRLINE }))).clicked() {
                    l.tint.mode = mode;
                }
            }
            if l.tint.mode == TintMode::Custom {
                ui.color_edit_button_rgb(&mut l.tint.color);
                for (name, tint) in [("green", [0.40, 1.0, 0.55]), ("white", [0.85, 0.9, 1.0]), ("amber", [1.0, 0.65, 0.2]), ("ice", [0.55, 0.8, 1.0])] {
                    if ui.add(egui::Button::new(egui::RichText::new(name).size(11.0)).frame(false)).clicked() {
                        l.tint.color = tint;
                    }
                }
            }
        });
    }
    // Modifier chain.
    ui.add_space(8.0);
    small_caps(ui, "Modifiers");
    modifiers_section(ui, settings, state, editing, a);
    ui.add_space(8.0);
    widgets::hairline(ui);
    ui.add_space(8.0);
    let kind = settings.layer(editing).visual;
    small_caps(ui, kind.name());
    ui.add_space(4.0);
    if let Some(scene) = layers.get_mut(editing) {
        if scene.kind() == kind {
            let l = settings.layer_mut(editing);
            scene.ui(ui, l);
        } else {
            hint(ui, "switching visual…");
        }
    }
    if kind == VisualKind::Kaleido && settings.layer(editing).kaleido.source == ImageSource::Photos {
        ui.add_space(4.0);
        ui.label("Photos folder");
        ui.add(egui::TextEdit::singleline(&mut settings.photos_folder).desired_width(f32::INFINITY).hint_text(r"C:\Users\you\Pictures"));
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(egui::RichText::new("Use my Pictures").size(11.0)).frame(false)).clicked() {
                if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
                    settings.photos_folder = std::path::Path::new(&home).join("Pictures").display().to_string();
                }
            }
            let n = std::fs::read_dir(settings.photos_folder.trim()).map(|rd| rd.filter_map(|e| e.ok()).filter(|e| e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png"))).count()).ok();
            let msg = match n {
                Some(0) => "no jpg/png files there".to_string(),
                Some(n) => format!("{n} pictures"),
                None => "folder not found".to_string(),
            };
            ui.label(egui::RichText::new(msg).size(11.0).color(TEXT_3));
        });
        hint(ui, "JPEG and PNG, shown in a shuffled order. The folder is not read until this picture source is picked.");
    }
    ui.add_space(8.0);
    small_caps(ui, "Scene tint");
    ui.horizontal(|ui| {
        ui.color_edit_button_rgb(&mut settings.tint);
        for (name, tint) in [("green", [0.40, 1.0, 0.55]), ("white", [0.85, 0.9, 1.0]), ("amber", [1.0, 0.65, 0.2]), ("ice", [0.55, 0.8, 1.0])] {
            if ui.add(egui::Button::new(egui::RichText::new(name).size(11.0)).frame(false)).clicked() {
                settings.tint = tint;
            }
        }
    });
    toggle_row(ui, "Tint from album art", &mut settings.tint_from_art);
    hint(ui, "Layers follow the scene tint unless they pick the album art or their own colour above.");
    let _ = stats;
}

fn post_section(ui: &mut egui::Ui, settings: &mut Settings) {
    if section_header(ui, "Post", Some("· whole picture"), Some("Reset")) {
        settings.reset_post();
    }
    let p = &mut settings.post;
    slider_row(ui, "Exposure", &mut p.exposure, 0.1..=10.0, |v| format!("{v:.2}"), true);
    toggle_row(ui, "Auto exposure", &mut p.auto_exposure);
    ui.add_space(6.0);
    slider_row(ui, "Bloom", &mut p.bloom, 0.0..=3.0, |v| format!("{v:.2}"), false);
    slider_row(ui, "Grain", &mut p.grain, 0.0..=1.0, |v| format!("{v:.2}"), false);
    hint(ui, "Saved with the scene.");
}

fn sync_section(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, stats: &Stats, a: &mut Actions) {
    section_header(ui, "Sync", None, None);
    slider_row(ui, "Audio delay", &mut settings.audio_delay_ms, 0.0..=300.0, |v| format!("{v:.0} ms"), false);
    hint(ui, "Loopback hears the music before you do. Raise this until the beam lands on the beat, or calibrate by tapping.");
    ui.add_space(6.0);
    match &stats.calibration {
        None => {
            if ghost_button(ui, "Calibrate by tapping (C)", true).clicked() {
                a.calibrate_toggle = true;
            }
        }
        Some(c) => {
            ui.label(egui::RichText::new("Tap Space on each beat you hear.").color(theme::WARN));
            ui.label(egui::RichText::new(crate::overlays::calibration_status(c)).font(theme::mono(12.0)));
            ui.horizontal(|ui| {
                if ui.add_enabled(c.offset_ms.is_some(), egui::Button::new("Apply (Enter)")).clicked() {
                    a.calibrate_apply = true;
                }
                if ghost_button(ui, "Cancel (C)", false).clicked() {
                    a.calibrate_toggle = true;
                }
            });
        }
    }

    ui.add_space(12.0);
    small_caps(ui, "Source");
    let cap = match &stats.capture {
        CaptureMode::Starting => ("starting…".to_string(), TEXT_3),
        CaptureMode::Process { name, pid } => (format!("{name} (pid {pid})"), ACCENT),
        CaptureMode::Endpoint { device } => (format!("{} not running · listening to '{device}'", settings.target_process), TEXT_2),
        CaptureMode::Failed(e) => (format!("no audio: {e}"), ERROR),
    };
    ui.label(egui::RichText::new(cap.0).color(cap.1).font(theme::mono(11.5)));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Listen to");
        if !state.target_editing {
            state.target_edit = settings.target_process.clone();
        }
        let r = ui.add(egui::TextEdit::singleline(&mut state.target_edit).desired_width(120.0).font(theme::mono(12.0)));
        state.target_editing = r.has_focus();
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && state.target_edit.trim() != settings.target_process {
            a.set_target = Some(state.target_edit.trim().to_string());
        }
    });
    ui.horizontal_wrapped(|ui| {
        for app in ["Spotify.exe", "chrome.exe", "msedge.exe", "vlc.exe", "foobar2000.exe"] {
            if ui.add(egui::Button::new(egui::RichText::new(app.trim_end_matches(".exe")).size(11.0)).frame(false)).clicked() {
                a.set_target = Some(app.to_string());
            }
        }
    });
    hint(ui, "Process name of the player. Falls back to the whole system output when it is not running.");
    if matches!(stats.capture, CaptureMode::Endpoint { .. }) && settings.target_process.to_lowercase().contains("spotify") && ghost_button(ui, "Open Spotify", false).clicked() {
        a.launch_spotify = true;
    }
    hint(ui, &format!("{} Hz", stats.sample_rate));
}

/// Small diagram of the monitors; click to select. Returns true on change.
pub fn monitor_diagram(ui: &mut egui::Ui, monitors: &[MonitorInfo], selected: &mut usize) -> bool {
    if monitors.is_empty() {
        hint(ui, "No monitors reported.");
        return false;
    }
    let (minx, miny, maxx, maxy) = monitors.iter().fold((i32::MAX, i32::MAX, i32::MIN, i32::MIN), |acc, m| (acc.0.min(m.x), acc.1.min(m.y), acc.2.max(m.x + m.w as i32), acc.3.max(m.y + m.h as i32)));
    let span = vec2((maxx - minx).max(1) as f32, (maxy - miny).max(1) as f32);
    let avail = vec2(ui.available_width().min(260.0), 110.0);
    let scale = (avail.x / span.x).min(avail.y / span.y);
    let (area, _) = ui.allocate_exact_size(vec2(span.x * scale, span.y * scale), Sense::hover());
    let p = ui.painter();
    let mut changed = false;
    for (i, m) in monitors.iter().enumerate() {
        let r = Rect::from_min_size(pos2(area.min.x + (m.x - minx) as f32 * scale, area.min.y + (m.y - miny) as f32 * scale), vec2(m.w as f32 * scale, m.h as f32 * scale)).shrink(2.0);
        let resp = ui.interact(r, egui::Id::unique(("mon", i)), Sense::click()).on_hover_text(format!("{} · {}x{}{}", m.name, m.w, m.h, m.hz.map(|h| format!(" @ {h} Hz")).unwrap_or_default()));
        let active = i == *selected;
        p.rect_filled(r, 3.0, if active { theme::with_alpha(ACCENT, 40) } else if resp.hovered() { theme::RAISED } else { theme::with_alpha(HAIRLINE, 120) });
        p.rect_stroke(r, 3.0, egui::Stroke::new(1.0, if active { ACCENT } else { TEXT_3 }), egui::StrokeKind::Inside);
        p.text(r.center(), Align2::CENTER_CENTER, format!("{}", i + 1), theme::mono(12.0), if active { ACCENT } else { TEXT_2 });
        if resp.clicked() && !active {
            *selected = i;
            changed = true;
        }
    }
    changed
}

fn display_section(ui: &mut egui::Ui, settings: &mut Settings, stats: &Stats, a: &mut Actions) {
    section_header(ui, "Display", None, None);
    small_caps(ui, "Monitor");
    let mut sel = settings.monitor;
    if monitor_diagram(ui, &stats.monitors, &mut sel) {
        settings.monitor = sel;
    }
    if let Some(m) = stats.monitors.get(settings.monitor) {
        hint(ui, &format!("{}: {} · {}x{}{}", settings.monitor + 1, m.name, m.w, m.h, m.hz.map(|h| format!(" @ {h} Hz")).unwrap_or_default()));
    }
    ui.horizontal(|ui| {
        if ghost_button(ui, if stats.fullscreen { "Exit fullscreen (F11)" } else { "Fullscreen there (F11)" }, false).clicked() {
            a.toggle_fullscreen = true;
        }
    });
    toggle_row(ui, "Start fullscreen on it", &mut settings.start_fullscreen);

    ui.add_space(8.0);
    small_caps(ui, "Motion");
    slider_row(ui, "Frame cap", &mut settings.fps_cap, 0..=240, |v| if v == 0.0 { "vsync".into() } else { format!("{v:.0} fps") }, false);
    toggle_row(ui, "Low power when hidden / on battery", &mut settings.low_power);
    if let Some(true) = stats.on_battery {
        hint(ui, "On battery now.");
    }
    toggle_row(ui, "Reduce flashing", &mut settings.reduce_flashing);
    if settings.reduce_flashing {
        slider_row(ui, "Allowed brightness rise", &mut settings.flash_rise, 0.5..=8.0, |v| format!("{v:.1}× / s"), false);
        hint(ui, "Caps how fast the picture may brighten between frames. For photosensitive viewers; on by default.");
    }

    ui.add_space(8.0);
    small_caps(ui, "Chrome");
    toggle_row(ui, "Auto-hide bar and strip (3 s)", &mut settings.chrome_autohide);
    toggle_row(ui, "Show what's playing (N)", &mut settings.show_now_playing);
    if settings.show_now_playing {
        toggle_row(ui, "Track-change toast", &mut settings.toast);
    }
    ui.add_space(8.0);
    small_caps(ui, "Developer");
    toggle_row(ui, "Show meters", &mut settings.developer);
    if ghost_button(ui, "Run the welcome steps again", false).clicked() {
        a.rerun_first_run = true;
    }
    hint(ui, &format!("settings: {}", stats.settings_path));
}

fn meters_section(ui: &mut egui::Ui, f: &FeatureFrame, stats: &Stats) {
    section_header(ui, "Meters", None, None);
    hint(ui, &format!("{:.0} fps · {:.0}k pts/s · {} Hz · t={:.0}s", stats.fps, stats.points_per_sec / 1000.0, stats.sample_rate, f.time_s));
    ui.horizontal(|ui| {
        light(ui, f.beat_pulse, Color32::from_rgb(255, 220, 90));
        ui.label(egui::RichText::new(format!("{:.1} bpm · beat {}", f.bpm, f.beat_count)).font(theme::mono(12.0)));
    });
    meter(ui, "conf", f.beat_confidence);
    meter(ui, "phase", f.beat_phase);
    ui.horizontal(|ui| {
        ui.label("onsets");
        for (g, name) in ["low", "mid", "high", "all"].iter().enumerate() {
            light(ui, f.onset_pulse[g], Color32::from_rgb(120, 200, 255));
            ui.small(*name);
        }
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new(format!("{:.1} LUFS-M · {:.1} LUFS-S{}{}", f.lufs_m, f.lufs_s, if f.quiet { " · quiet" } else { "" }, if f.silent { " · silent" } else { "" })).font(theme::mono(12.0)));
    meter(ui, "rms", f.rms * 4.0);
    meter(ui, "bass", f.bass * 0.5);
    meter(ui, "mid", f.mid * 0.5);
    meter(ui, "treb", f.treb * 0.5);
    meter(ui, "width", f.stereo_width);
    meter(ui, "trend", f.energy_trend * 0.5 + 0.5);
    ui.label(egui::RichText::new(format!("centroid {:.0} Hz · peak {:.2}", f.centroid_hz, f.peak)).font(theme::mono(12.0)));
    bars(ui, &f.bands, 40.0, Color32::from_rgb(110, 220, 150), None);
    ui.add_space(8.0);
    ui.label(egui::RichText::new(format!("dominant {}", NOTE_NAMES[f.key_class as usize % 12])).font(theme::mono(12.0)));
    bars(ui, &f.chroma, 40.0, Color32::from_rgb(230, 150, 255), Some(&NOTE_NAMES));
}

fn about_section(ui: &mut egui::Ui, stats: &Stats, a: &mut Actions) {
    section_header(ui, "About", None, None);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
        widgets::mark(ui.painter(), r.center(), 15.0, ACCENT, true);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(format!("audioviz {}", crate::VERSION)).font(theme::bold(15.0)));
            ui.label(egui::RichText::new("MIT · © 2026 Gregory Douglas").color(TEXT_2).size(11.5));
        });
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new("No telemetry. No network, except the button below when you press it.").color(TEXT_2).size(12.0));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let busy = stats.update == UpdateStatus::Checking;
        if ui.add_enabled(!busy, egui::Button::new(if busy { "Checking…" } else { "Check for updates" })).clicked() {
            a.check_updates = true;
        }
        match &stats.update {
            UpdateStatus::Idle | UpdateStatus::Checking => {}
            UpdateStatus::UpToDate(tag) => {
                ui.label(egui::RichText::new(format!("up to date ({tag})")).color(TEXT_2).size(12.0));
            }
            UpdateStatus::Available { tag, url } => {
                if ui.add(egui::Button::new(egui::RichText::new(format!("{tag} available · open")).color(ACCENT)).frame(false)).clicked() {
                    a.open_url = Some(url.clone());
                }
            }
            UpdateStatus::Failed(e) => {
                ui.label(egui::RichText::new(e).color(theme::WARN).size(11.5));
            }
        }
    });
    hint(ui, "Fetches the latest release tag from GitHub, once, when pressed.");
    ui.add_space(10.0);
    small_caps(ui, "Links");
    for (label, url) in [("Releases", "https://github.com/gregodouglas/audioviz/releases"), ("Privacy", "https://github.com/gregodouglas/audioviz/blob/main/PRIVACY.md"), ("Report a problem", "https://github.com/gregodouglas/audioviz/issues")] {
        if ui.add(egui::Button::new(egui::RichText::new(label).color(TEXT)).frame(false)).clicked() {
            a.open_url = Some(url.into());
        }
    }
    ui.add_space(10.0);
    small_caps(ui, "Diagnostics");
    ui.label(egui::RichText::new(&stats.diagnostics).font(theme::mono(10.5)).color(TEXT_2));
    if ghost_button(ui, "Copy diagnostics", false).clicked() {
        a.copy_diagnostics = true;
    }
    hint(ui, "Paste this into a bug report.");
    if let Some(err) = &stats.shader_error {
        ui.add_space(8.0);
        ui.colored_label(ERROR, format!("shader error:\n{err}"));
    }
    ui.add_space(10.0);
    small_caps(ui, "Shortcuts");
    for (k, d) in crate::overlays::SHORTCUTS {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(*k).font(theme::mono(11.0)).color(TEXT));
            ui.label(egui::RichText::new(*d).color(TEXT_2).size(11.5));
        });
    }
}

fn meter(ui: &mut egui::Ui, label: &str, v: f32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{label:>5}")).font(theme::mono(11.0)).color(TEXT_2));
        ui.add(egui::ProgressBar::new(v.clamp(0.0, 1.0)).desired_width(180.0).desired_height(6.0));
    });
}

fn light(ui: &mut egui::Ui, v: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    let a = (v.clamp(0.0, 1.0) * 255.0) as u8;
    ui.painter().circle_filled(rect.center(), 6.0, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a.max(25)));
}

fn bars(ui: &mut egui::Ui, values: &[f32], height: f32, color: Color32, labels: Option<&[&str; 12]>) {
    let n = values.len().max(1) as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height + if labels.is_some() { 12.0 } else { 0.0 }), Sense::hover());
    let painter = ui.painter_at(rect);
    let w = rect.width() / n;
    let bottom = rect.top() + height;
    for (i, b) in values.iter().enumerate() {
        let h = b.clamp(0.0, 1.0) * height;
        let x0 = rect.left() + i as f32 * w;
        painter.rect_filled(Rect::from_min_max(pos2(x0 + 1.0, bottom - h), pos2(x0 + w - 1.0, bottom)), 0.0, color);
        if let Some(l) = labels {
            painter.text(pos2(x0 + w * 0.5, bottom + 6.0), Align2::CENTER_CENTER, l[i % 12], egui::FontId::proportional(9.0), TEXT_2);
        }
    }
}
