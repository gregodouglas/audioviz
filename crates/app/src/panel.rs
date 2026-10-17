//! The settings panel: a right-docked sheet with an icon rail, one section at
//! a time. Replaces the old floating debug window.

use audioviz_analysis::FeatureFrame;
use audioviz_audio::CaptureMode;
use egui::{Align2, Color32, Rect, Sense, pos2, vec2};

use crate::looks::Look;
use crate::overlays::CalibrationView;
use crate::scene::Scene;
use crate::settings::{SCENE_COUNT, Section, Settings};
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
    pub scene_names: Vec<&'static str>,
    pub scene_current: usize,
    pub scene_fading: bool,
    pub last_cue: &'static str,
    pub now_playing: Option<String>,
    pub calibration: Option<CalibrationView>,
    pub looks: Vec<Look>,
    pub update: UpdateStatus,
    pub diagnostics: String,
    pub on_battery: Option<bool>,
}

#[derive(Default)]
pub struct Actions {
    pub toggle_fullscreen: bool,
    pub switch_scene: Option<usize>,
    pub calibrate_toggle: bool,
    pub calibrate_apply: bool,
    pub apply_look: Option<Settings>,
    pub save_look: Option<String>,
    pub delete_look: Option<String>,
    pub open_url: Option<String>,
    pub open_looks_folder: bool,
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
    pub look_name: String,
    pub target_edit: String,
    pub target_editing: bool,
}

pub fn draw(ctx: &egui::Context, settings: &mut Settings, state: &mut PanelState, f: &FeatureFrame, stats: &Stats, scene: &mut dyn Scene, t: f32) -> Actions {
    let mut a = Actions::default();
    if t <= 0.001 {
        return a;
    }
    let screen = ctx.content_rect();
    let x = screen.max.x - PANEL_W * t;
    let rect = Rect::from_min_max(pos2(x, screen.min.y + BAR_H), screen.max);

    egui::Area::new(egui::Id::unique("panel")).fixed_pos(rect.min).order(egui::Order::Middle).show(ctx, |ui| {
        ui.set_min_size(rect.size());
        ui.set_clip_rect(rect);
        let p = ui.painter().clone();
        p.rect_filled(rect, 0.0, theme::panel_fill());
        p.vline(rect.min.x + 0.5, rect.y_range(), egui::Stroke::new(1.0, HAIRLINE));
        p.vline(rect.min.x + RAIL_W + 0.5, rect.y_range(), egui::Stroke::new(1.0, HAIRLINE));
        // Swallow pointer input on the panel background.
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
                Section::Scene => scene_section(ui, settings, stats, scene, &mut a),
                Section::Post => post_section(ui, settings, stats),
                Section::Sync => sync_section(ui, settings, state, stats, &mut a),
                Section::Display => display_section(ui, settings, stats, &mut a),
                Section::Looks => looks_section(ui, settings, state, stats, &mut a),
                Section::Meters => meters_section(ui, f, stats),
                Section::About => about_section(ui, stats, &mut a),
            }
        });

        // Footer.
        let fy = rect.max.y - 44.0;
        p.hline((rect.min.x + RAIL_W + 20.0)..=(rect.max.x - 20.0), fy, egui::Stroke::new(1.0, HAIRLINE));
        p.text(pos2(rect.min.x + RAIL_W + 20.0, fy + 14.0), Align2::LEFT_CENTER, "H hide all   N now playing   ? keys", theme::mono(10.5), TEXT_3);
        p.text(pos2(rect.min.x + RAIL_W + 20.0, fy + 30.0), Align2::LEFT_CENTER, "C calibrate   F11 fullscreen", theme::mono(10.5), TEXT_3);
        p.text(pos2(rect.max.x - 20.0, fy + 30.0), Align2::RIGHT_CENTER, crate::VERSION, theme::mono(10.0), TEXT_3);
        let sc = ui.interact(Rect::from_min_size(pos2(rect.min.x + RAIL_W + 20.0, fy + 22.0), vec2(220.0, 16.0)), egui::Id::unique("footer-shortcuts"), Sense::click());
        if sc.clicked() {
            a.show_shortcuts = true;
        }
    });
    a
}

fn scene_section(ui: &mut egui::Ui, settings: &mut Settings, stats: &Stats, scene: &mut dyn Scene, a: &mut Actions) {
    if section_header(ui, "Scene", None, Some("Hide UI (H)")) {
        a.hide_ui = true;
    }
    toggle_row(ui, "Layers: blend several scenes", &mut settings.layer_mode);
    if settings.layer_mode {
        hint(ui, "Every layer that is on is added to the picture with its opacity. 1-3 picks the layer to edit.");
        ui.add_space(4.0);
        for (i, name) in stats.scene_names.iter().enumerate() {
            let focused = i == stats.scene_current;
            ui.horizontal(|ui| {
                let l = settings.layer_mut(i);
                let (sw, resp) = ui.allocate_exact_size(vec2(28.0, 16.0), Sense::click());
                if resp.clicked() {
                    l.enabled = !l.enabled;
                }
                let t = ui.ctx().animate_bool_with_time(resp.id, l.enabled, theme::ANIM_S);
                ui.painter().rect_filled(sw, 8.0, widgets::lerp_color(HAIRLINE, ACCENT, t));
                ui.painter().circle_filled(pos2(sw.min.x + 8.0 + 12.0 * t, sw.center().y), 6.0, widgets::lerp_color(TEXT_2, theme::BG, t));
                let label = egui::RichText::new(format!("{}  {}", i + 1, name)).color(if focused { ACCENT } else { TEXT });
                if ui.add(egui::Button::new(label).frame(false)).clicked() {
                    a.switch_scene = Some(i);
                }
                if focused {
                    ui.label(egui::RichText::new("editing").size(11.0).color(TEXT_3));
                }
            });
            if settings.layer(i).enabled {
                let l = settings.layer_mut(i);
                slider_row(ui, "    opacity", &mut l.opacity, 0.0..=2.0, |v| format!("{v:.2}"), false);
            }
        }
    } else {
        ui.horizontal_wrapped(|ui| {
            for (i, name) in stats.scene_names.iter().enumerate() {
                let active = i == stats.scene_current;
                let b = egui::Button::new(egui::RichText::new(format!("{}  {}", i + 1, name)).color(if active { ACCENT } else { TEXT })).fill(if active { theme::with_alpha(ACCENT, 30) } else { Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if active { ACCENT } else { HAIRLINE }));
                if ui.add(b).clicked() && !active {
                    a.switch_scene = Some(i);
                }
            }
            if stats.scene_fading {
                ui.label(egui::RichText::new("fading…").color(TEXT_3));
            }
        });
        ui.add_space(4.0);
        toggle_row(ui, "Auto director (A)", &mut settings.auto_scenes);
        hint(ui, &format!("Changes scene on track change, drops, quiet, dwell. Last cue: {}", stats.last_cue));
    }
    if let Some(np) = &stats.now_playing {
        hint(ui, np);
    }
    ui.add_space(8.0);
    widgets::hairline(ui);
    ui.add_space(8.0);
    small_caps(ui, stats.scene_names.get(stats.scene_current).copied().unwrap_or("scene"));
    ui.add_space(4.0);
    scene.ui(ui, settings);

}

fn post_section(ui: &mut egui::Ui, settings: &mut Settings, stats: &Stats) {
    let cur = stats.scene_current;
    let name = stats.scene_names.get(cur).copied().unwrap_or("scene");
    if section_header(ui, "Post", Some(&format!("· {name}")), Some("Reset")) {
        settings.reset_post(cur);
    }
    let p = settings.post_mut(cur);
    slider_row(ui, "Exposure", &mut p.exposure, 0.1..=10.0, |v| format!("{v:.2}"), true);
    toggle_row(ui, "Auto exposure", &mut p.auto_exposure);
    ui.add_space(6.0);
    slider_row(ui, "Bloom", &mut p.bloom, 0.0..=3.0, |v| format!("{v:.2}"), false);
    slider_row(ui, "Grain", &mut p.grain, 0.0..=1.0, |v| format!("{v:.2}"), false);
    hint(ui, "Per scene. Blended across cross-fades.");
    ui.add_space(8.0);
    if ghost_button(ui, "Copy to all scenes", false).clicked() {
        let p = settings.post_for(cur);
        for i in 0..SCENE_COUNT.max(settings.post.len()) {
            *settings.post_mut(i) = p;
        }
    }
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
        toggle_row(ui, "Title overlay in the starfield (T)", &mut settings.type_overlay);
        toggle_row(ui, "Track-change toast", &mut settings.toast);
    }
    toggle_row(ui, "Tint from album art", &mut settings.tint_from_art);
    ui.add_space(8.0);
    small_caps(ui, "Developer");
    toggle_row(ui, "Show meters", &mut settings.developer);
    if ghost_button(ui, "Run the welcome steps again", false).clicked() {
        a.rerun_first_run = true;
    }
    hint(ui, &format!("settings: {}", stats.settings_path));
}

fn looks_section(ui: &mut egui::Ui, settings: &mut Settings, state: &mut PanelState, stats: &Stats, a: &mut Actions) {
    section_header(ui, "Looks", None, None);
    hint(ui, "A look is every visual setting at once. Scene, sliders, post, tint. Not your monitor, delay or window.");
    ui.add_space(8.0);
    for look in &stats.looks {
        let active = settings.look == look.name;
        ui.horizontal(|ui| {
            let b = egui::Button::new(egui::RichText::new(&look.name).color(if active { ACCENT } else { TEXT })).fill(if active { theme::with_alpha(ACCENT, 30) } else { Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if active { ACCENT } else { HAIRLINE })).min_size(vec2(140.0, 26.0));
            if ui.add(b).clicked() {
                a.apply_look = Some(look.settings.clone());
                settings.look = look.name.clone();
            }
            if look.builtin {
                ui.label(egui::RichText::new("built in").size(11.0).color(TEXT_3));
            } else if ui.add(egui::Button::new(egui::RichText::new("delete").size(11.0).color(TEXT_3)).frame(false)).clicked() {
                a.delete_look = Some(look.name.clone());
            }
        });
    }
    if !settings.look.is_empty() && !stats.looks.iter().any(|l| l.name == settings.look) {
        hint(ui, &format!("'{}' (edited)", settings.look));
    }
    ui.add_space(10.0);
    small_caps(ui, "Save current as");
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut state.look_name).desired_width(150.0).hint_text("name"));
        if ui.add_enabled(!state.look_name.trim().is_empty(), egui::Button::new("Save")).clicked() {
            a.save_look = Some(state.look_name.trim().to_string());
        }
    });
    ui.add_space(6.0);
    if ghost_button(ui, "Open looks folder", false).clicked() {
        a.open_looks_folder = true;
    }
    hint(ui, "Each look is a JSON file. Copy one to share it; drop one in to import.");
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
