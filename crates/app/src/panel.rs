//! The settings panel: a right-docked sheet with an icon rail, one section at
//! a time. Sections: Scenes (compositions), Layers (the stack plus an
//! inspector for one layer: Visual / Colour / Effects tabs, a modifier's page
//! opening in place), Picture (post stack and scene tint), Sync, Display,
//! Meters (developer), About.

use audioviz_analysis::FeatureFrame;
use audioviz_audio::CaptureMode;
use egui::{Align2, Color32, Rect, Sense, pos2, vec2};

use crate::overlays::CalibrationView;
use crate::scene::Scene;
use crate::scene_store::SceneDef;
use crate::color;
use crate::settings::{AudioBand, AudioSourceKind, Blend, ChromaSplit, Edge, EdgeColour, Feedback, Fold, FoldMode, Fractal, FractalMode, Grade, ImageSource, LayerTab, Look, Luminescence, MAX_LAYERS, MAX_MODS, Mask, MaskShape, Modifier, PaletteSel, Polar, PostSettings, Ripple, Section, Settings, TintMode, VisualKind};
use crate::theme::{self, ACCENT, BAR_H, ERROR, HAIRLINE, PANEL_W, RAIL_W, TEXT, TEXT_2, TEXT_3, WARN as WARN_C};
use crate::widgets::{self, chip, dropdown, ghost_button, group, hint, inspector_header, section_header, segmented, slider_def, slider_row, small_caps, toggle_row};

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

/// Casting flow, as the panel sees it.
#[derive(Clone, Debug, PartialEq)]
pub enum CastView {
    /// The Windows cast panel was opened; waiting for a new display.
    Waiting { secs: u32 },
    /// A display appeared after casting began: its monitor index.
    Found { monitor: usize },
    /// Opening the cast panel failed.
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
    pub cast: Option<CastView>,
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
    pub set_source: Option<AudioSourceKind>,
    pub show_shortcuts: bool,
    pub rerun_first_run: bool,
    pub hide_ui: bool,
    /// Open the Windows cast panel and watch for a new display.
    pub cast: bool,
    pub cast_cancel: bool,
    /// Go fullscreen on this monitor (the TV that just appeared).
    pub fullscreen_on: Option<usize>,
}

/// Scratch UI state that is not a setting.
#[derive(Default)]
pub struct PanelState {
    pub scene_name: String,
    /// Which modifier of the edited layer shows its controls, and whether
    /// its page is open in place of the layer tabs.
    pub editing_mod: usize,
    pub mod_open: bool,
    pub mod_layer: usize,
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
        widgets::groups_load(&settings.ui_open);
        if settings.section == Section::Layers {
            // The stack and the inspector header stay put; only the tab body scrolls.
            cui.set_width(content.width() - 8.0);
            layers_section(&mut cui, settings, state, stats, layers, &mut a);
        } else {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut cui, |ui| {
                ui.set_width(content.width() - 8.0);
                match settings.section {
                    Section::Scenes => scenes_section(ui, settings, state, stats, &mut a),
                    Section::Layers => {}
                    Section::Picture => picture_section(ui, settings),
                    Section::Sync => sync_section(ui, settings, state, stats, &mut a),
                    Section::Display => display_section(ui, settings, stats, &mut a),
                    Section::Meters => meters_section(ui, f, stats),
                    Section::About => about_section(ui, stats, &mut a),
                }
            });
        }
        widgets::groups_store(&mut settings.ui_open);

        // Footer.
        let fy = rect.max.y - 44.0;
        p.hline((rect.min.x + RAIL_W + 20.0)..=(rect.max.x - 20.0), fy, egui::Stroke::new(1.0, HAIRLINE));
        p.text(pos2(rect.min.x + RAIL_W + 20.0, fy + 14.0), Align2::LEFT_CENTER, "H Hide all   N Now playing   ? Keys", theme::mono(10.5), TEXT_3);
        p.text(pos2(rect.min.x + RAIL_W + 20.0, fy + 30.0), Align2::LEFT_CENTER, "1-3 Layer   Space Scene", theme::mono(10.5), TEXT_3);
        p.text(pos2(rect.max.x - 20.0, fy + 30.0), Align2::RIGHT_CENTER, crate::VERSION, theme::mono(10.0), TEXT_3);
        let sc = ui.interact(Rect::from_min_size(pos2(rect.min.x + RAIL_W + 20.0, fy + 6.0), vec2(220.0, 16.0)), egui::Id::unique("footer-shortcuts"), Sense::click());
        if sc.clicked() {
            a.show_shortcuts = true;
        }
    });
    a
}

/// A scene row: marker dot, name, full width; the current scene is accented.
fn scene_row(ui: &mut egui::Ui, label: &str, active: bool, width: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    let hovered = resp.hovered();
    if active || hovered {
        ui.painter().rect_filled(rect, 4.0, theme::with_alpha(ACCENT, if active { 26 } else { 12 }));
    }
    ui.painter().circle_filled(pos2(rect.min.x + 10.0, rect.center().y), 2.5, if active { ACCENT } else { TEXT_3 });
    ui.painter().text(pos2(rect.min.x + 20.0, rect.center().y), Align2::LEFT_CENTER, label, if active { theme::bold(12.5) } else { theme::sans(12.5) }, if active { ACCENT } else if hovered { TEXT } else { TEXT_2 });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn scenes_section(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, stats: &Stats, a: &mut Actions) {
    if section_header(ui, "Scenes", None, Some("Hide UI (H)")) {
        a.hide_ui = true;
    }
    hint(ui, "A scene is a mix of up to three layers plus the picture settings. Space cycles; A lets the director pick on track changes and drops.");
    ui.add_space(8.0);
    toggle_row(ui, "Auto director (A)", &mut settings.auto_scenes);
    hint(ui, &format!("Last cue: {}{}", stats.last_cue, if stats.fading { " · fading" } else { "" }));
    if let Some(np) = &stats.now_playing {
        hint(ui, np);
    }
    ui.add_space(8.0);
    small_caps(ui, "Built in");
    for def in stats.scenes.iter().filter(|s| s.builtin) {
        let active = settings.scene_name == def.name;
        if scene_row(ui, &def.name, active, ui.available_width()) {
            a.apply_scene = Some(def.name.clone());
        }
    }
    let user: Vec<&SceneDef> = stats.scenes.iter().filter(|s| !s.builtin).collect();
    if !user.is_empty() {
        small_caps(ui, "Yours");
        for def in user {
            let active = settings.scene_name == def.name;
            ui.horizontal(|ui| {
                if scene_row(ui, &def.name, active, ui.available_width() - 56.0) {
                    a.apply_scene = Some(def.name.clone());
                }
                if ui.add(egui::Button::new(egui::RichText::new("Delete").size(11.0).color(TEXT_3)).frame(false)).clicked() {
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
        ui.add(egui::TextEdit::singleline(&mut state.scene_name).desired_width(150.0).hint_text("Name"));
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

fn mini_switch(ui: &mut egui::Ui, on: &mut bool) {
    let (sw, resp) = ui.allocate_exact_size(vec2(24.0, 14.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, theme::ANIM_S);
    ui.painter().rect_filled(sw, 7.0, widgets::lerp_color(HAIRLINE, ACCENT, t));
    ui.painter().circle_filled(pos2(sw.min.x + 7.0 + 10.0 * t, sw.center().y), 5.0, widgets::lerp_color(TEXT_2, theme::BG, t));
}

/// A small triangle button; returns true when clicked. Dimmed when `enabled` is false.
fn arrow_button(ui: &mut egui::Ui, up: bool, enabled: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), if enabled { Sense::click() } else { Sense::hover() });
    let c = if !enabled { theme::with_alpha(TEXT_3, 90) } else if resp.hovered() { TEXT } else { TEXT_3 };
    let cx = r.center().x;
    let (y0, y1) = if up { (r.max.y - 5.0, r.min.y + 5.0) } else { (r.min.y + 5.0, r.max.y - 5.0) };
    ui.painter().add(egui::Shape::convex_polygon(vec![pos2(cx - 4.0, y0), pos2(cx + 4.0, y0), pos2(cx, y1)], c, egui::Stroke::NONE));
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    } else {
        false
    }
}

/// Drop target over a whole item (its row plus anything drawn under it, the
/// full content width): draws the insertion line and returns the dragged
/// payload when the pointer is released over it.
fn drop_zone<T: Clone + Send + Sync + 'static>(ui: &mut egui::Ui, top: f32, bottom: f32) -> Option<T> {
    let x = ui.max_rect().x_range();
    let rect = Rect::from_x_y_ranges(x, top..=bottom);
    let hover = ui.ctx().pointer_hover_pos().is_some_and(|p| rect.contains(p));
    if !hover {
        return None;
    }
    ui.painter().rect_filled(rect, 4.0, theme::with_alpha(ACCENT, 18));
    ui.painter().hline(x, top - 1.0, egui::Stroke::new(2.0, ACCENT));
    if ui.input(|i| i.pointer.any_released()) {
        return egui::DragAndDrop::take_payload::<T>(ui.ctx()).map(|a| (*a).clone());
    }
    None
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

/// Paint a palette as a horizontal strip.
fn palette_strip(ui: &mut egui::Ui, stops: &[color::Stop], cyclic: bool, height: f32) {
    let table = color::bake(stops, cyclic);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let n = table.len() as f32;
    for (i, e) in table.iter().enumerate() {
        let x0 = rect.min.x + rect.width() * i as f32 / n;
        let x1 = rect.min.x + rect.width() * (i as f32 + 1.0) / n;
        let c = Color32::from_rgb((e[0].clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8, (e[1].clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8, (e[2].clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8);
        ui.painter().rect_filled(Rect::from_min_max(pos2(x0, rect.min.y), pos2(x1 + 0.5, rect.max.y)), 0.0, c);
    }
    ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(1.0, HAIRLINE), egui::StrokeKind::Outside);
}

fn palette_section(ui: &mut egui::Ui, settings: &mut Settings, editing: usize) {
    let l = settings.layer_mut(editing);
    let ps = &mut l.palette;
    let mut items: Vec<&str> = vec!["none"];
    items.extend(color::PRESETS.iter().map(|p| p.name));
    items.push("custom");
    let shown: Vec<String> = items.iter().map(|n| widgets::title(n)).collect();
    let shown_ref: Vec<&str> = shown.iter().map(|s| s.as_str()).collect();
    let sel = items.iter().position(|n| *n == ps.name);
    if let Some(i) = dropdown(ui, "Palette", &shown_ref, sel) {
        ps.name = items[i].into();
    }
    if !ps.active() {
        hint(ui, "Visuals colour themselves (tint, pitch hues, blackbody). Pick a palette to colour the ribbons, the black hole's disk and the sigil from it, or add the palette modifier to recolour any layer.");
        return;
    }
    ui.add_space(4.0);
    let stops = ps.stops();
    palette_strip(ui, &stops, ps.cyclic, 14.0);
    ui.add_space(6.0);
    let d = PaletteSel::default();
    slider_def(ui, "Amount", &mut ps.amount, Some(d.amount), 0.0..=1.0, |v| format!("{v:.2}"), false);
    slider_def(ui, "Cycle", &mut ps.cycle, Some(d.cycle), -1.0..=1.0, |v| format!("{v:.2} /s"), false);
    slider_def(ui, "Reacts", &mut ps.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
    toggle_row(ui, "Wrap around (seamless cycling)", &mut ps.cyclic);
    if ps.name == "custom" {
        let n = ps.custom.len();
        group(ui, "palette.stops", "Custom stops", true, n, |ui| {
            let mut remove: Option<usize> = None;
            for (i, st) in ps.custom.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.color_edit_button_rgb(&mut st.rgb);
                    ui.spacing_mut().slider_width = ui.available_width() - 28.0;
                    ui.add(egui::Slider::new(&mut st.t, 0.0..=1.0).show_value(false));
                    if n > 2 && ui.add(egui::Button::new(egui::RichText::new("×").color(TEXT_3)).frame(false)).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                ps.custom.remove(i);
            }
            ui.horizontal_wrapped(|ui| {
                if n < 8 && ui.add(egui::Button::new(egui::RichText::new("Add stop").size(11.0)).frame(false)).clicked() {
                    let t = ps.custom.iter().map(|s| s.t).fold(0.0f32, f32::max);
                    ps.custom.push(color::Stop { t: (t + 0.2).min(1.0), rgb: [1.0, 1.0, 1.0] });
                }
                let names: Vec<String> = color::PRESETS.iter().map(|p| widgets::title(p.name)).collect();
                let names_ref: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                if let Some(i) = dropdown(ui, "Start from", &names_ref, None) {
                    ps.custom = color::PRESETS[i].stops.to_vec();
                }
            });
        });
    }
    hint(ui, "Cycling moves the colours along the palette with the energy and steps on beats. The ribbons spread a chord across it; the black hole's disk runs cool to hot along it; the sigil colours each element by its place.");
}

fn effects_tab(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, editing: usize, a: &mut Actions) {
    let l = settings.layer_mut(editing);
    let n = l.modifiers.len();
    if n == 0 {
        hint(ui, "Modifiers are passes over this layer's picture, run in order: mirrors, fractal recursion, luminescent afterglow, echo, polar wrap, ripples, edges, colour fringes, a mask, a palette recolour.");
        ui.add_space(4.0);
    }
    let dragging = egui::DragAndDrop::has_payload_of_type::<ModDrag>(ui.ctx());
    let mut remove: Option<usize> = None;
    let mut moved: Option<(usize, usize)> = None;
    for j in 0..n {
        let top = ui.cursor().top();
        ui.horizontal(|ui| {
            drag_handle(ui, ui.scope_id().with(("mod drag", editing, j)), ModDrag(j));
            let m = &mut l.modifiers[j];
            mini_switch(ui, m.enabled_mut());
            let label = egui::RichText::new(m.name()).color(if m.enabled() { TEXT } else { TEXT_2 });
            if ui.add(egui::Button::new(label).frame(false)).on_hover_text("Open its controls").clicked() {
                state.editing_mod = j;
                state.mod_open = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(egui::RichText::new("×").color(TEXT_3)).frame(false)).on_hover_text("Remove").clicked() {
                    remove = Some(j);
                }
                if arrow_button(ui, false, j + 1 < n) {
                    moved = Some((j, j + 1));
                }
                if arrow_button(ui, true, j > 0) {
                    moved = Some((j, j - 1));
                }
                if ui.add(egui::Button::new(egui::RichText::new("›").color(TEXT_2)).frame(false)).clicked() {
                    state.editing_mod = j;
                    state.mod_open = true;
                }
            });
        });
        if dragging {
            let bottom = ui.cursor().top();
            if let Some(from) = drop_zone::<ModDrag>(ui, top, bottom) {
                if from.0 != j {
                    moved = Some((from.0, j));
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
        ui.add_space(6.0);
        let names: Vec<&str> = Modifier::KINDS.iter().map(|(n, _)| *n).collect();
        if let Some(i) = dropdown(ui, "Add", &names, None) {
            l.modifiers.push((Modifier::KINDS[i].1)());
            state.editing_mod = l.modifiers.len() - 1;
            state.mod_open = true;
        }
    } else {
        hint(ui, "Six modifiers is the most a layer runs.");
    }
    if n > 0 {
        hint(ui, "Order matters: each pass sees the one above it. Click a name to open its controls; Esc comes back.");
    }
}

/// One modifier's controls, opened in place of the layer tabs.
fn modifier_page(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, editing: usize, a: &mut Actions) {
    let l = settings.layer_mut(editing);
    let j = state.editing_mod;
    let Some(m) = l.modifiers.get_mut(j) else {
        state.mod_open = false;
        return;
    };
    toggle_row(ui, "On", m.enabled_mut());
    slider_def(ui, "Mix", m.mix_mut(), Some(1.0), 0.0..=1.0, |v| format!("{:.0}%", v * 100.0), false);
    modifier_band(ui, m.band_mut());
    ui.add_space(4.0);
    modifier_controls(ui, m);
    ui.add_space(10.0);
    let (mut do_reset, mut do_remove) = (false, false);
    ui.horizontal(|ui| {
        do_reset = ghost_button(ui, "Reset", false).clicked();
        do_remove = ghost_button(ui, "Remove", false).clicked();
        if ghost_button(ui, "‹ Back to effects", false).clicked() {
            state.mod_open = false;
        }
    });
    if do_reset {
        m.reset();
    }
    if do_remove {
        l.modifiers.remove(j);
        a.remove_modifier = Some((editing, j));
        state.mod_open = false;
    }
}

/// Which part of the music a modifier follows: the layer's own band (the
/// default), the full mix, a preset band or a custom range.
fn modifier_band(ui: &mut egui::Ui, band: &mut Option<AudioBand>) {
    let mut items: Vec<&str> = vec!["This layer's band", "Full mix"];
    items.extend(AudioBand::PRESETS.iter().map(|(n, _, _)| *n));
    items.push("Custom range");
    let custom = items.len() - 1;
    let sel = match band {
        None => 0,
        Some(b) if !b.enabled => 1,
        Some(b) => AudioBand::PRESETS.iter().position(|(_, lo, hi)| (b.lo_hz - lo).abs() < 1.0 && (b.hi_hz - hi).abs() < 1.0).map(|p| p + 2).unwrap_or(custom),
    };
    if let Some(i) = dropdown(ui, "Reacts to", &items, Some(sel)) {
        if i == 0 {
            *band = None;
        } else if i == 1 {
            *band = Some(AudioBand { enabled: false, ..band.unwrap_or_default() });
        } else if i == custom {
            *band = Some(AudioBand { enabled: true, ..band.unwrap_or_default() });
        } else {
            let (_, lo, hi) = AudioBand::PRESETS[i - 2];
            *band = Some(AudioBand { enabled: true, lo_hz: lo, hi_hz: hi });
        }
    }
    if let Some(b) = band {
        if b.enabled && sel == custom {
            slider_row(ui, "Low cut", &mut b.lo_hz, 20.0..=16000.0, |v| format!("{v:.0} Hz"), true);
            slider_row(ui, "High cut", &mut b.hi_hz, 20.0..=16000.0, |v| format!("{v:.0} Hz"), true);
            if b.hi_hz < b.lo_hz * 1.2 {
                b.hi_hz = b.lo_hz * 1.2;
            }
        }
    }
}

fn modifier_controls(ui: &mut egui::Ui, m: &mut Modifier) {
    match m {
        Modifier::Fold(k) => {
            let d = Fold::default();
            small_caps(ui, "Mirrors");
            let modes = [FoldMode::Radial, FoldMode::Pinwheel, FoldMode::Tiled];
            if let Some(i) = segmented(ui, &["Radial", "Pinwheel", "Tiled"], modes.iter().position(|m| *m == k.mode).unwrap_or(0)) {
                k.mode = modes[i];
            }
            if k.mode != FoldMode::Tiled {
                slider_def(ui, "Segments", &mut k.segments, Some(d.segments), 1..=24, |v| format!("{v:.0}"), false);
                slider_def(ui, "Step on beats", &mut k.step_beats, Some(d.step_beats), 0..=16, |v| if v < 0.5 { "off".into() } else { format!("every {v:.0}") }, false);
            }
            slider_def(ui, "Spin", &mut k.speed, Some(d.speed), -2.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Zoom", &mut k.zoom, Some(d.zoom), 0.2..=3.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Apex offset", &mut k.offset, Some(d.offset), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Wander", &mut k.wander, Some(d.wander), 0.0..=4.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Reacts", &mut k.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Spin follows energy and onsets; zoom breathes with the bass. The apex off-centre is what makes real kaleidoscopes rich.");
        }
        Modifier::Feedback(k) => {
            let d = Feedback::default();
            slider_def(ui, "Decay", &mut k.decay, Some(d.decay), 0.0..=0.985, |v| format!("{v:.3}"), false);
            slider_def(ui, "Zoom", &mut k.zoom, Some(d.zoom), -1.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Spin", &mut k.spin, Some(d.spin), -1.5..=1.5, |v| format!("{v:.2}"), false);
            slider_def(ui, "Reacts", &mut k.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Last frame echoes under this one, zooming and turning. One feedback per layer keeps its own history.");
        }
        Modifier::Polar(k) => {
            let d = Polar::default();
            small_caps(ui, "Direction");
            if let Some(i) = segmented(ui, &["Wrap", "Unwrap"], k.unwrap as usize) {
                k.unwrap = i == 1;
            }
            slider_def(ui, "Radius", &mut k.radius, Some(d.radius), 0.2..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Spin", &mut k.spin, Some(d.spin), -2.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Reacts", &mut k.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Wrap turns left-to-right into around-the-centre: a scope becomes a ring.");
        }
        Modifier::Ripple(k) => {
            let d = Ripple::default();
            slider_def(ui, "Amount", &mut k.amount, Some(d.amount), 0.0..=0.2, |v| format!("{v:.3}"), false);
            slider_def(ui, "Wavelength", &mut k.wavelength, Some(d.wavelength), 0.02..=0.6, |v| format!("{v:.2}"), false);
            slider_def(ui, "Speed", &mut k.speed, Some(d.speed), 0.1..=3.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Standing wave", &mut k.sine, Some(d.sine), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Reacts", &mut k.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Beats launch rings from the centre. Above 1.5 reacts, onsets launch them too.");
        }
        Modifier::Edge(k) => {
            let d = Edge::default();
            slider_def(ui, "Strength", &mut k.strength, Some(d.strength), 0.0..=5.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Keep original", &mut k.keep, Some(d.keep), 0.0..=1.0, |v| format!("{v:.2}"), false);
            small_caps(ui, "Colour");
            let names: Vec<&str> = EdgeColour::ALL.iter().map(|c| c.name()).collect();
            if let Some(i) = segmented(ui, &names, EdgeColour::ALL.iter().position(|c| *c == k.colour).unwrap_or(0)) {
                k.colour = EdgeColour::ALL[i];
            }
            slider_def(ui, "Reacts", &mut k.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Palette colours each edge by its direction, so a curve runs through the palette and the palette's cycle turns it with the music.");
        }
        Modifier::Chroma(k) => {
            let d = ChromaSplit::default();
            slider_def(ui, "Amount", &mut k.amount, Some(d.amount), 0.0..=0.08, |v| format!("{v:.3}"), false);
            toggle_row(ui, "Radial from the centre", &mut k.radial);
            slider_def(ui, "Reacts", &mut k.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Pulses with treble and onsets.");
        }
        Modifier::Fractal(k) => {
            let d = Fractal::default();
            small_caps(ui, "Recursion");
            if let Some(i) = segmented(ui, &["Kaleidoscopic IFS", "Julia"], (k.mode == FractalMode::Julia) as usize) {
                k.mode = if i == 1 { FractalMode::Julia } else { FractalMode::Kifs };
            }
            slider_def(ui, "Depth", &mut k.iterations, Some(d.iterations), 1..=8, |v| format!("{v:.0}"), false);
            if k.mode == FractalMode::Kifs {
                slider_def(ui, "Scale per level", &mut k.scale, Some(d.scale), 1.1..=3.0, |v| format!("{v:.2}"), false);
                slider_def(ui, "Twist", &mut k.angle, Some(d.angle), -2.0..=2.0, |v| format!("{v:.2}"), false);
                slider_def(ui, "Shift", &mut k.offset, Some(d.offset), 0.0..=2.0, |v| format!("{v:.2}"), false);
            } else {
                slider_def(ui, "Orbit speed", &mut k.angle, Some(d.angle), -2.0..=2.0, |v| format!("{v:.2}"), false);
                slider_def(ui, "Orbit size", &mut k.offset, Some(d.offset), 0.0..=1.0, |v| format!("{v:.2}"), false);
            }
            slider_def(ui, "Zoom", &mut k.zoom, Some(d.zoom), 0.2..=3.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Deeper levels", &mut k.falloff, Some(d.falloff), 0.1..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Reacts", &mut k.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "IFS folds, turns, scales and shifts the picture at every level so it recurs at every size. Julia pulls it into z² + c filaments; the twist and scale breathe with the music.");
        }
        Modifier::Lumi(k) => {
            let d = Luminescence::default();
            slider_def(ui, "Fresh glow", &mut k.fresh_gain, Some(d.fresh_gain), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Fresh release", &mut k.fresh_s, Some(d.fresh_s), 0.05..=2.0, |v| format!("{v:.2} s"), false);
            slider_def(ui, "Afterglow", &mut k.afterglow_gain, Some(d.afterglow_gain), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Afterglow release", &mut k.afterglow_s, Some(d.afterglow_s), 0.5..=20.0, |v| format!("{v:.1} s"), true);
            slider_def(ui, "Afterglow hue", &mut k.hue_shift, Some(d.hue_shift), -180.0..=180.0, |v| format!("{v:.0}°"), false);
            toggle_row(ui, "Glow in the picture's colours", &mut k.own_colour);
            slider_def(ui, "UV saturation", &mut k.uv, Some(d.uv), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Excite on onsets", &mut k.excite, Some(d.excite), 0.0..=4.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Sparkle", &mut k.sparkle, Some(d.sparkle), 0.0..=3.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Reacts", &mut k.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "A dual phosphor: the picture flashes and leaves a long afterglow in a shifted hue; charged areas sparkle on transients like plankton. In the picture's colours a palette carries straight through; off, it glows in the layer tint.");
        }
        Modifier::Grade(k) => {
            let d = Grade::default();
            small_caps(ui, "Map");
            if let Some(i) = segmented(ui, &["By brightness", "By hue"], k.by_hue as usize) {
                k.by_hue = i == 1;
            }
            slider_def(ui, "Amount", &mut k.amount, Some(d.amount), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Reacts", &mut k.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Recolours this layer's picture through its palette (choose one above; none = phosphor). By brightness is a gradient map: dark to the first colours, bright to the last.");
        }
        Modifier::Mask(k) => {
            let d = Mask::default();
            small_caps(ui, "Shape");
            let shapes = [MaskShape::Circle, MaskShape::Band, MaskShape::Hex];
            if let Some(i) = segmented(ui, &["Circle", "Band", "Hex"], shapes.iter().position(|m| *m == k.shape).unwrap_or(0)) {
                k.shape = shapes[i];
            }
            slider_def(ui, "Size", &mut k.radius, Some(d.radius), 0.05..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Softness", &mut k.soft, Some(d.soft), 0.0..=0.5, |v| format!("{v:.2}"), false);
            toggle_row(ui, "Keep the outside instead", &mut k.invert);
            slider_def(ui, "Reacts", &mut k.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Lets two layers share the frame by place, not only by adding. Size breathes with the bass.");
        }
    }
}

fn layers_section(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, stats: &Stats, layers: &mut [Box<dyn Scene>], a: &mut Actions) {
    if section_header(ui, "Layers", None, Some("Hide UI (H)")) {
        a.hide_ui = true;
    }
    hint(ui, "1 is on top. Each layer meets the ones below by its blend (Visual tab): add is order-free, over covers, screen lightens, multiply masks.");
    ui.add_space(4.0);
    let editing = settings.editing.min(MAX_LAYERS - 1);
    if state.mod_layer != editing {
        state.mod_layer = editing;
        state.mod_open = false;
    }
    stack(ui, settings, state, a, editing);
    ui.add_space(8.0);

    let hue = theme::layer_hue(editing);
    let kind = settings.layer(editing).visual;
    let crumb = format!("Layer {} · {}", editing + 1, kind.short());
    let n_mods = settings.layer(editing).modifiers.len();
    state.editing_mod = state.editing_mod.min(n_mods.saturating_sub(1));
    let mod_open = state.mod_open && n_mods > 0;
    if mod_open {
        let name = settings.layer(editing).modifiers[state.editing_mod].name();
        if inspector_header(ui, hue, &[&crumb, name], Some("‹ back")) {
            state.mod_open = false;
        }
    } else {
        inspector_header(ui, hue, &[&crumb], None);
        let sel = LayerTab::ALL.iter().position(|t| *t == settings.layer_tab).unwrap_or(0);
        let tabs: Vec<&str> = LayerTab::ALL.iter().map(|t| t.name()).collect();
        if let Some(i) = segmented(ui, &tabs, sel) {
            settings.layer_tab = LayerTab::ALL[i];
        }
        ui.add_space(4.0);
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width() - 8.0);
        if mod_open {
            modifier_page(ui, settings, state, editing, a);
        } else {
            match settings.layer_tab {
                LayerTab::Visual => visual_tab(ui, settings, layers, editing),
                LayerTab::Colour => colour_tab(ui, settings, editing),
                LayerTab::Effects => effects_tab(ui, settings, state, editing, a),
            }
        }
        ui.add_space(12.0);
    });
    let _ = stats;
}

/// The composition: one row per slot with its hue, switch, name, band, inline
/// opacity and order buttons; the modifier chain as chips under it.
fn stack(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, a: &mut Actions, editing: usize) {
    let dragging = egui::DragAndDrop::has_payload_of_type::<usize>(ui.ctx());
    for i in 0..MAX_LAYERS {
        let focused = i == editing;
        let hue = theme::layer_hue(i);
        let top = ui.cursor().top();
        let has_chain = !settings.layer(i).modifiers.is_empty();
        let item_h = 26.0 + if has_chain { 22.0 } else { 0.0 };
        if focused {
            let r = Rect::from_min_size(pos2(ui.max_rect().min.x - 4.0, top - 2.0), vec2(ui.available_width() + 8.0, item_h + 4.0));
            ui.painter().rect_filled(r, 5.0, theme::with_alpha(hue, 14));
        }
        ui.horizontal(|ui| {
            ui.set_height(26.0);
            let drag_id = ui.scope_id().with(("layer drag", i));
            let enabled = settings.layer(i).enabled;
            ui.dnd_drag_source(drag_id, i, |ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(10.0, 22.0), Sense::hover());
                let c = if enabled || focused { hue } else { theme::with_alpha(hue, 80) };
                ui.painter().rect_filled(Rect::from_min_size(pos2(r.min.x, r.min.y + 2.0), vec2(3.0, 18.0)), 1.5, c);
                resp.on_hover_cursor(egui::CursorIcon::Grab);
            });
            {
                let l = settings.layer_mut(i);
                mini_switch(ui, &mut l.enabled);
                let label = egui::RichText::new(format!("{}  {}", i + 1, l.visual.short())).font(if focused { theme::bold(12.5) } else { theme::sans(12.5) }).color(if focused { TEXT } else { TEXT_2 });
                if ui.add(egui::Button::new(label).frame(false)).clicked() {
                    a.select_layer = Some(i);
                }
                if l.band.enabled {
                    chip(ui, &l.band.label(), TEXT_2, false);
                }
            }
            let solo = settings.solo == Some(i);
            let mut solo_click = false;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if arrow_button(ui, false, i + 1 < MAX_LAYERS) {
                    a.move_layer = Some((i, i + 1));
                }
                if arrow_button(ui, true, i > 0) {
                    a.move_layer = Some((i, i - 1));
                }
                let l = settings.layer_mut(i);
                if l.enabled {
                    let v = ui.visuals_mut();
                    v.widgets.inactive.bg_fill = theme::with_alpha(TEXT_3, 110);
                    v.selection.bg_fill = theme::with_alpha(hue, 150);
                    ui.spacing_mut().interact_size.y = 16.0;
                    ui.label(egui::RichText::new(format!("{:.2}", l.opacity)).font(theme::mono(10.5)).color(TEXT_2));
                    ui.spacing_mut().slider_width = 56.0;
                    ui.add(egui::Slider::new(&mut l.opacity, 0.0..=2.0).show_value(false)).on_hover_text("Opacity: this layer's share of the mix");
                }
                if chip(ui, "S", if solo { WARN_C } else { TEXT_3 }, true).on_hover_text("Solo: show only this layer while tuning").clicked() {
                    solo_click = true;
                }
            });
            if solo_click {
                settings.solo = if solo { None } else { Some(i) };
            }
        });
        if has_chain {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                let l = settings.layer(i);
                for (j, m) in l.modifiers.iter().enumerate() {
                    let c = if m.enabled() { hue } else { TEXT_3 };
                    if chip(ui, m.name(), c, true).on_hover_text("Open its controls").clicked() {
                        a.select_layer = Some(i);
                        state.editing_mod = j;
                        state.mod_open = true;
                        state.mod_layer = i;
                    }
                }
            });
        }
        if dragging {
            let bottom = ui.cursor().top();
            if let Some(from) = drop_zone::<usize>(ui, top, bottom) {
                if from != i {
                    a.move_layer = Some((from, i));
                }
            }
        }
        ui.add_space(2.0);
    }
    if settings.solo.is_some() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Solo on: only that layer is drawn").size(11.0).color(WARN_C));
            if ui.add(egui::Button::new(egui::RichText::new("Clear").size(11.0).color(TEXT_2)).frame(false)).clicked() {
                settings.solo = None;
            }
        });
    }
}

fn visual_tab(ui: &mut egui::Ui, settings: &mut Settings, layers: &mut [Box<dyn Scene>], editing: usize) {
    let l = settings.layer_mut(editing);
    let names: Vec<&str> = VisualKind::ALL.iter().map(|k| k.name()).collect();
    if let Some(i) = dropdown(ui, "Visual", &names, VisualKind::ALL.iter().position(|k| *k == l.visual)) {
        let k = VisualKind::ALL[i];
        l.visual = k;
        // The kaleidoscope is its picture plus the fold: add the mirrors with it.
        if k == VisualKind::Kaleido && !l.has_fold() {
            l.modifiers.insert(0, Modifier::Fold(Default::default()));
        }
    }
    // Audio band: full mix, a preset, or a custom range.
    let mut items: Vec<&str> = vec!["Full mix"];
    items.extend(AudioBand::PRESETS.iter().map(|(n, _, _)| *n));
    items.push("Custom range");
    let custom = items.len() - 1;
    let sel = if !l.band.enabled { 0 } else { AudioBand::PRESETS.iter().position(|(_, lo, hi)| (l.band.lo_hz - lo).abs() < 1.0 && (l.band.hi_hz - hi).abs() < 1.0).map(|p| p + 1).unwrap_or(custom) };
    if let Some(i) = dropdown(ui, "Reacts to", &items, Some(sel)) {
        if i == 0 {
            l.band.enabled = false;
        } else if i == custom {
            l.band.enabled = true;
        } else {
            let (_, lo, hi) = AudioBand::PRESETS[i - 1];
            l.band = AudioBand { enabled: true, lo_hz: lo, hi_hz: hi };
        }
    }
    if l.band.enabled && sel == custom {
        slider_row(ui, "Low cut", &mut l.band.lo_hz, 20.0..=16000.0, |v| format!("{v:.0} Hz"), true);
        slider_row(ui, "High cut", &mut l.band.hi_hz, 20.0..=16000.0, |v| format!("{v:.0} Hz"), true);
        if l.band.hi_hz < l.band.lo_hz * 1.2 {
            l.band.hi_hz = l.band.lo_hz * 1.2;
        }
    }
    if l.band.enabled {
        hint(ui, "The scope draws only this band of the waveform; other visuals take their energy, onsets and spectrum from it.");
    }
    let blends: Vec<&str> = Blend::ALL.iter().map(|b| b.name()).collect();
    if let Some(i) = dropdown(ui, "Blend", &blends, Blend::ALL.iter().position(|b| *b == l.blend)) {
        l.blend = Blend::ALL[i];
    }
    hint(ui, match l.blend {
        Blend::Add => "Add: light sums with the layers below; order does not matter.",
        Blend::Over => "Over: this layer's bright parts cover the layers below it (1 is on top).",
        Blend::Screen => "Screen: lightens the layers below without blowing out.",
        Blend::Multiply => "Multiply: masks the layers below; its dark parts darken them.",
    });
    ui.add_space(8.0);
    let kind = l.visual;
    if let Some(scene) = layers.get_mut(editing) {
        if scene.kind() == kind {
            scene.ui(ui, l);
        } else {
            hint(ui, "Switching visual…");
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
                Some(0) => "No JPG or PNG files there".to_string(),
                Some(n) => format!("{n} pictures"),
                None => "Folder not found".to_string(),
            };
            ui.label(egui::RichText::new(msg).size(11.0).color(TEXT_3));
        });
        hint(ui, "JPEG and PNG, shown in a shuffled order. The folder is not read until this picture source is picked.");
    }
}

fn colour_tab(ui: &mut egui::Ui, settings: &mut Settings, editing: usize) {
    let l = settings.layer_mut(editing);
    small_caps(ui, "Tint");
    let modes = [TintMode::Scene, TintMode::Art, TintMode::Custom];
    if let Some(i) = segmented(ui, &["Scene tint", "Album art", "Own colour"], modes.iter().position(|m| *m == l.tint.mode).unwrap_or(0)) {
        l.tint.mode = modes[i];
    }
    if l.tint.mode == TintMode::Custom {
        ui.horizontal(|ui| {
            ui.color_edit_button_rgb(&mut l.tint.color);
            for (name, tint) in [("Green", [0.40, 1.0, 0.55]), ("White", [0.85, 0.9, 1.0]), ("Amber", [1.0, 0.65, 0.2]), ("Ice", [0.55, 0.8, 1.0]), ("Violet", [0.55, 0.35, 1.0])] {
                if ui.add(egui::Button::new(egui::RichText::new(name).size(11.0)).frame(false)).clicked() {
                    l.tint.color = tint;
                }
            }
        });
    }
    hint(ui, match l.tint.mode {
        TintMode::Scene => "Follows the scene tint (Picture section).",
        TintMode::Art => "Takes the album art's dominant colour; the scene tint when there is no art.",
        TintMode::Custom => "This layer's own base colour.",
    });
    ui.add_space(8.0);
    small_caps(ui, "Palette");
    palette_section(ui, settings, editing);
}

fn picture_section(ui: &mut egui::Ui, settings: &mut Settings) {
    if section_header(ui, "Picture", Some("· saved with the scene"), Some("Reset")) {
        settings.reset_post();
    }
    let d = PostSettings::default();
    let p = &mut settings.post;
    slider_def(ui, "Exposure", &mut p.exposure, Some(d.exposure), 0.1..=10.0, |v| format!("{v:.2}"), true);
    toggle_row(ui, "Auto exposure", &mut p.auto_exposure);
    ui.add_space(6.0);
    slider_def(ui, "Bloom", &mut p.bloom, Some(d.bloom), 0.0..=3.0, |v| format!("{v:.2}"), false);
    slider_def(ui, "Grain", &mut p.grain, Some(d.grain), 0.0..=1.0, |v| format!("{v:.2}"), false);
    ui.add_space(8.0);
    small_caps(ui, "Look");
    if let Some(i) = segmented(ui, &["Film", "Neon"], (p.look == Look::Neon) as usize) {
        p.look = if i == 1 { Look::Neon } else { Look::Film };
    }
    slider_def(ui, "Saturation", &mut p.saturation, Some(d.saturation), 0.0..=2.0, |v| format!("{v:.2}"), false);
    hint(ui, match p.look {
        Look::Film => "Film: bright colours roll off toward white, like a camera.",
        Look::Neon => "Neon: hue and saturation survive brightness; only the hottest cores whiten, like a tube.",
    });
    ui.add_space(8.0);
    small_caps(ui, "Scene tint");
    ui.horizontal(|ui| {
        ui.color_edit_button_rgb(&mut settings.tint);
        for (name, tint) in [("Green", [0.40, 1.0, 0.55]), ("White", [0.85, 0.9, 1.0]), ("Amber", [1.0, 0.65, 0.2]), ("Ice", [0.55, 0.8, 1.0]), ("Violet", [0.55, 0.35, 1.0])] {
            if ui.add(egui::Button::new(egui::RichText::new(name).size(11.0)).frame(false)).clicked() {
                settings.tint = tint;
            }
        }
    });
    toggle_row(ui, "Tint from album art", &mut settings.tint_from_art);
    hint(ui, "The base colour every layer starts from, unless a layer picks the album art or its own colour (Layers › Colour).");
}

fn sync_section(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, stats: &Stats, a: &mut Actions) {
    section_header(ui, "Sync", None, None);
    slider_row(ui, "Audio delay", &mut settings.audio_delay_ms, 0.0..=1000.0, |v| format!("{v:.0} ms"), false);
    hint(ui, "Loopback hears the music before you do. Raise this until the beam lands on the beat, or calibrate by tapping. Bluetooth is 100–250 ms, a cast TV 100–300 ms more.");
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
    let names: Vec<&str> = AudioSourceKind::ALL.iter().map(|s| s.name()).collect();
    if let Some(i) = segmented(ui, &names, AudioSourceKind::ALL.iter().position(|s| *s == settings.source).unwrap_or(0)) {
        a.set_source = Some(AudioSourceKind::ALL[i]);
    }
    let cap = match &stats.capture {
        CaptureMode::Starting => ("Starting…".to_string(), TEXT_3),
        CaptureMode::Process { name, pid } => (format!("{name} (pid {pid})"), ACCENT),
        CaptureMode::Endpoint { device } => (format!("{} not running · listening to '{device}'", settings.target_process), TEXT_2),
        CaptureMode::Microphone { device } => (format!("Listening to the room through '{device}'"), ACCENT),
        CaptureMode::Failed(e) => (format!("no audio: {e}"), ERROR),
    };
    ui.label(egui::RichText::new(cap.0).color(cap.1).font(theme::mono(11.5)));
    ui.add_space(4.0);
    if settings.source == AudioSourceKind::Microphone {
        hint(ui, "For music that plays somewhere else: a TV, a Sonos, a hi-fi. The mic hears it when you do, so the delay above wants 0 unless you calibrate. Bass and beat tracking are only as good as the microphone.");
        hint(ui, &format!("{} Hz", stats.sample_rate));
        return;
    }
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
        for (label, app) in [("Spotify", "Spotify.exe"), ("Chrome", "chrome.exe"), ("Edge", "msedge.exe"), ("VLC", "vlc.exe"), ("foobar2000", "foobar2000.exe")] {
            if ui.add(egui::Button::new(egui::RichText::new(label).size(11.0)).frame(false)).on_hover_text(app).clicked() {
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
    small_caps(ui, "Cast to a TV · experimental");
    match &stats.cast {
        None => {
            if ghost_button(ui, "Cast to a TV…", false).clicked() {
                a.cast = true;
            }
            hint(ui, "Mirrors this PC to a Roku or any Miracast TV through Windows (Win+K). Play Spotify on this PC, not with Spotify Connect: the sound travels with the picture, and a soundbar on the TV gets it too.");
        }
        Some(CastView::Waiting { secs }) => {
            ui.label(egui::RichText::new("Pick the TV in the Windows panel…").color(theme::WARN));
            hint(ui, "If the TV shows the same picture as this screen, press Win+P and choose Extend so it becomes its own display. It will appear in the monitor list above.");
            ui.horizontal(|ui| {
                if ghost_button(ui, "Open the panel again", false).clicked() {
                    a.cast = true;
                }
                if ui.add(egui::Button::new(egui::RichText::new(format!("Stop waiting ({secs} s)")).size(11.0).color(TEXT_2)).frame(false)).clicked() {
                    a.cast_cancel = true;
                }
            });
        }
        Some(CastView::Found { monitor }) => {
            let name = stats.monitors.get(*monitor).map(|m| format!("{} · {}x{}", m.name, m.w, m.h)).unwrap_or_default();
            ui.label(egui::RichText::new(format!("TV connected as display {}", monitor + 1)).color(ACCENT));
            hint(ui, &name);
            ui.horizontal(|ui| {
                if ghost_button(ui, "Fullscreen on the TV", true).clicked() {
                    a.fullscreen_on = Some(*monitor);
                }
                if ui.add(egui::Button::new(egui::RichText::new("Done").size(11.0).color(TEXT_2)).frame(false)).clicked() {
                    a.cast_cancel = true;
                }
            });
            hint(ui, "Wireless displays run 100–300 ms behind. If the beat lands late on the TV, raise Audio delay in Sync.");
        }
        Some(CastView::Failed(e)) => {
            ui.label(egui::RichText::new("Could not open the Windows cast panel").color(ERROR));
            hint(ui, &format!("{e}. Press Win+K instead, then pick the TV."));
            if ghost_button(ui, "Try again", false).clicked() {
                a.cast = true;
            }
        }
    }

    ui.add_space(8.0);
    small_caps(ui, "Motion");
    slider_row(ui, "Frame cap", &mut settings.fps_cap, 0..=240, |v| if v == 0.0 { "VSync".into() } else { format!("{v:.0} fps") }, false);
    toggle_row(ui, "Low power when hidden / on battery", &mut settings.low_power);
    if let Some(true) = stats.on_battery {
        hint(ui, "On battery now.");
    }
    toggle_row(ui, "Reduce flashing", &mut settings.reduce_flashing);
    if settings.reduce_flashing {
        slider_row(ui, "Allowed brightness rise", &mut settings.flash_rise, 0.5..=8.0, |v| format!("{v:.1}×/s"), false);
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
        ui.label(egui::RichText::new(format!("{:.1} BPM · Beat {}", f.bpm, f.beat_count)).font(theme::mono(12.0)));
    });
    meter(ui, "Confidence", f.beat_confidence);
    meter(ui, "Phase", f.beat_phase);
    ui.horizontal(|ui| {
        ui.label("Onsets");
        for (g, name) in ["Low", "Mid", "High", "All"].iter().enumerate() {
            light(ui, f.onset_pulse[g], Color32::from_rgb(120, 200, 255));
            ui.small(*name);
        }
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new(format!("{:.1} LUFS-M · {:.1} LUFS-S{}{}", f.lufs_m, f.lufs_s, if f.quiet { " · Quiet" } else { "" }, if f.silent { " · Silent" } else { "" })).font(theme::mono(12.0)));
    meter(ui, "RMS", f.rms * 4.0);
    meter(ui, "Bass", f.bass * 0.5);
    meter(ui, "Mid", f.mid * 0.5);
    meter(ui, "Treble", f.treb * 0.5);
    meter(ui, "Width", f.stereo_width);
    meter(ui, "Trend", f.energy_trend * 0.5 + 0.5);
    ui.label(egui::RichText::new(format!("Centroid {:.0} Hz · Peak {:.2}", f.centroid_hz, f.peak)).font(theme::mono(12.0)));
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
        ui.label(egui::RichText::new(format!("{label:>10}")).font(theme::mono(11.0)).color(TEXT_2));
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
