use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use audioviz_audio::{CaptureMode, CaptureTarget};
use audioviz_metadata::MetadataHandle;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::chrome::{Chrome, ChromeInput};
use crate::config;
use crate::director::Director;
use crate::engine::Engine;
use crate::gpu::Gpu;
use crate::hittest;
use crate::looks::{self, Look};
use crate::overlay::Overlay;
use crate::overlays::{self, CALIBRATION_MIN_TAPS, CalibrationView, FirstRun};
use crate::panel::{self, MonitorInfo, PanelState, Stats, UpdateStatus};
use crate::platform;
use crate::post::{Post, PostParams};
use crate::scene::FrameCtx;
use crate::scenes::ribbons::Ribbons;
use crate::scenes::scope::{MAX_POINTS, Scope};
use crate::scenes::starfield::Starfield;
use crate::settings::{Section, Settings};
use crate::shaders::ShaderSet;
use crate::strip::Strip;
use crate::theme::{self, BAR_H, PANEL_W, STRIP_H};
use crate::tray::{Tray, TrayAction};

/// Scene index of the starfield (the one with the type overlay).
const STARFIELD_SCENE: usize = 2;
const NOTICE_SECONDS: f32 = 10.0;
const TOAST_SECONDS: f32 = 4.0;
const REPO: &str = "gregodouglas/audioviz";

/// Tap-along audio-delay calibration. The user taps Space on beats they hear;
/// each tap is matched to the nearest onset the analyzer detected (after the
/// current delay line), and the median residual is the correction to apply.
struct Calibration {
    pending: Vec<Instant>,
    /// Matched residuals, ms (positive = tap after onset = visuals lead the ear).
    residuals: Vec<f32>,
    history: Vec<bool>,
    taps: usize,
}

impl Calibration {
    const MATCH_WINDOW_MS: f32 = 180.0;
    const SETTLE_MS: u64 = 250;

    fn new() -> Self {
        Self { pending: Vec::new(), residuals: Vec::new(), history: Vec::new(), taps: 0 }
    }

    fn tap(&mut self) {
        self.taps += 1;
        self.pending.push(Instant::now());
    }

    fn resolve(&mut self, onsets: &[Instant]) {
        let now = Instant::now();
        let settle = Duration::from_millis(Self::SETTLE_MS);
        let mut i = 0;
        while i < self.pending.len() {
            let tap = self.pending[i];
            if now.duration_since(tap) < settle {
                i += 1;
                continue;
            }
            let best = onsets.iter().map(|o| signed_ms(tap, *o)).filter(|d| d.abs() <= Self::MATCH_WINDOW_MS).min_by(|a, b| a.abs().total_cmp(&b.abs()));
            match best {
                Some(d) => {
                    self.residuals.push(d);
                    self.history.push(true);
                }
                None => self.history.push(false),
            }
            self.pending.swap_remove(i);
        }
    }

    fn view(&self) -> CalibrationView {
        let mut sorted = self.residuals.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let median = |v: &[f32]| if v.is_empty() { 0.0 } else { v[v.len() / 2] };
        let offset = median(&sorted);
        let mut dev: Vec<f32> = sorted.iter().map(|r| (r - offset).abs()).collect();
        dev.sort_by(|a, b| a.total_cmp(b));
        CalibrationView {
            taps: self.taps,
            matched: self.residuals.len(),
            history: self.history.clone(),
            offset_ms: (self.residuals.len() >= CALIBRATION_MIN_TAPS).then_some(offset),
            spread_ms: median(&dev),
        }
    }
}

fn signed_ms(a: Instant, b: Instant) -> f32 {
    if a >= b { a.duration_since(b).as_secs_f32() * 1000.0 } else { -(b.duration_since(a).as_secs_f32() * 1000.0) }
}

struct Running {
    gpu: Gpu,
    director: Director,
    post: Post,
    shaders: ShaderSet,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    overlay: Overlay,
    chrome: Chrome,
    strip: Strip,
    panel_state: PanelState,

    engine: Engine,
    metadata: MetadataHandle,
    pending_points: Vec<[f32; 2]>,

    settings: Settings,
    last_saved: Settings,
    last_save_check: Instant,
    sent_delay_ms: f32,
    applied_monitor: usize,
    start: Instant,
    last_frame: Instant,
    fps: f32,
    points_per_sec: f32,
    auto_gain_value: f32,
    post_error: Option<String>,

    calibration: Option<Calibration>,
    calibration_result: Option<(String, Instant)>,
    last_capture: CaptureMode,
    capture_notice: Option<(String, Instant, bool)>,
    first_run: Option<FirstRun>,
    show_shortcuts: bool,
    last_track: Option<u64>,
    toast: Option<(String, String, Instant)>,
    looks: Vec<Look>,
    update: Arc<Mutex<UpdateStatus>>,
    /// Smoothed effective tint (follows the art when enabled).
    tint_current: [f32; 3],
    art_tint: Option<[f32; 3]>,
    occluded: bool,
    battery: Option<bool>,
    last_battery_check: Instant,
    spotify_installed: bool,
    quit: bool,
    tray: Option<Tray>,
    /// "Let the visuals work": every UI element hidden until H / Tab.
    zen: bool,
    cursor_hidden: bool,
}

fn time_floor(t: f32) -> i32 {
    t.floor() as i32
}

/// The taskbar / title-bar icon, embedded from `assets/icon-256.png`.
fn window_icon() -> Option<winit::window::Icon> {
    const PNG: &[u8] = include_bytes!("../../../assets/icon-256.png");
    let img = image::load_from_memory_with_format(PNG, image::ImageFormat::Png).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), w, h).ok()
}

pub struct App {
    state: Option<Running>,
    start_in_tray: bool,
}

impl App {
    pub fn new(start_in_tray: bool) -> Self {
        Self { state: None, start_in_tray }
    }
}

impl Running {
    fn new(window: Arc<Window>, start_in_tray: bool) -> anyhow::Result<Self> {
        let mut settings = config::load();
        // Dev overrides for screenshots: AUDIOVIZ_PANEL=<section> opens the panel
        // on that section and skips the welcome sheet; AUDIOVIZ_CHROME=1 pins the bar.
        if let Ok(sec) = std::env::var("AUDIOVIZ_PANEL") {
            settings.show_ui = true;
            settings.onboarded = true;
            settings.developer = true;
            if let Some(s) = Section::ALL.iter().find(|s| s.name().eq_ignore_ascii_case(&sec)) {
                settings.section = *s;
            }
        }
        // AUDIOVIZ_SETTINGS='{"layer_mode":true}' merges JSON fields (dev, never saved).
        if let Ok(json) = std::env::var("AUDIOVIZ_SETTINGS") {
            if let (Ok(mut base), Ok(serde_json::Value::Object(patch))) = (serde_json::to_value(&settings), serde_json::from_str::<serde_json::Value>(&json)) {
                if let Some(obj) = base.as_object_mut() {
                    for (k, v) in patch {
                        obj.insert(k, v);
                    }
                }
                if let Ok(s) = serde_json::from_value::<Settings>(base) {
                    settings = s;
                }
            }
        }
        if std::env::var_os("AUDIOVIZ_CHROME").is_some() {
            settings.chrome_autohide = false;
            settings.onboarded = true;
            settings.show_now_playing = true;
        }
        let gpu = Gpu::new(window.clone())?;
        let shaders = ShaderSet::load();
        let (w, h) = (gpu.config.width, gpu.config.height);
        let scope = Scope::new(&gpu.device, w, h, &shaders).map_err(|e| anyhow::anyhow!("scope: {e}"))?;
        let ribbons = Ribbons::new(&gpu.device, w, h, &shaders).map_err(|e| anyhow::anyhow!("ribbons: {e}"))?;
        let starfield = Starfield::new(&gpu.device, w, h, &shaders).map_err(|e| anyhow::anyhow!("starfield: {e}"))?;
        let mut director = Director::new(&gpu.device, &shaders, vec![Box::new(scope), Box::new(ribbons), Box::new(starfield)]).map_err(|e| anyhow::anyhow!("director: {e}"))?;
        let start_scene = std::env::var("AUDIOVIZ_SCENE").ok().and_then(|v| v.parse().ok()).unwrap_or(settings.scene);
        director.current = start_scene.min(director.scenes.len() - 1);
        director.auto = settings.auto_scenes;
        let post = Post::new(&gpu.device, gpu.config.format, w, h, &shaders).map_err(|e| anyhow::anyhow!("post: {e}"))?;

        let egui_ctx = egui::Context::default();
        theme::install(&egui_ctx);
        let egui_state = egui_winit::State::new(egui_ctx.clone(), egui::ViewportId::ROOT, &*window, Some(window.scale_factor() as f32), window.theme(), Some(gpu.device.limits().max_texture_dimension_2d as usize));
        let egui_renderer = egui_wgpu::Renderer::new(&gpu.device, gpu.ui_format, egui_wgpu::RendererOptions::default());

        let engine = Engine::start(CaptureTarget::Process(settings.target_process.clone()))?;
        if settings.audio_delay_ms > 0.0 {
            engine.set_delay_ms(settings.audio_delay_ms);
        }
        let app_filter = settings.target_process.trim_end_matches(".exe").to_lowercase();
        let metadata = audioviz_metadata::default_source().start(&app_filter)?;
        hittest::install_on(&window);

        let first_run = (!settings.onboarded).then(FirstRun::default);
        let mut running = Self {
            gpu,
            director,
            post,
            shaders,
            egui_ctx,
            egui_state,
            egui_renderer,
            overlay: Overlay::new(),
            chrome: Chrome::new(),
            strip: Strip::new(),
            panel_state: PanelState::default(),
            engine,
            metadata,
            pending_points: Vec::with_capacity(MAX_POINTS),
            sent_delay_ms: settings.audio_delay_ms,
            applied_monitor: settings.monitor,
            last_saved: settings.clone(),
            last_save_check: Instant::now(),
            tint_current: settings.tint,
            settings,
            start: Instant::now(),
            last_frame: Instant::now(),
            fps: 0.0,
            points_per_sec: 0.0,
            auto_gain_value: 1.0,
            post_error: None,
            calibration: None,
            calibration_result: None,
            last_capture: CaptureMode::Starting,
            capture_notice: None,
            first_run,
            show_shortcuts: false,
            last_track: None,
            toast: None,
            looks: looks::all(),
            update: Arc::new(Mutex::new(UpdateStatus::Idle)),
            art_tint: None,
            occluded: false,
            battery: platform::on_battery(),
            last_battery_check: Instant::now(),
            spotify_installed: platform::spotify_installed(),
            quit: false,
            tray: None,
            zen: false,
            cursor_hidden: false,
        };
        let scene_names: Vec<&str> = running.director.scenes.iter().map(|s| s.name()).collect();
        let monitors = running.monitors();
        match Tray::new(&scene_names, &monitors, running.settings.auto_scenes) {
            Ok(t) => running.tray = Some(t),
            Err(e) => tracing::warn!("tray icon unavailable: {e:#}"),
        }
        if running.settings.start_fullscreen && !start_in_tray {
            running.set_fullscreen(true);
        }
        if start_in_tray {
            running.gpu.window.set_visible(false);
            running.occluded = true;
        }
        Ok(running)
    }

    fn monitors(&self) -> Vec<MonitorInfo> {
        self.gpu
            .window
            .available_monitors()
            .map(|m| {
                let size = m.size();
                let pos = m.position();
                MonitorInfo { name: m.name().unwrap_or_else(|| "display".into()), x: pos.x, y: pos.y, w: size.width, h: size.height, hz: m.refresh_rate_millihertz().map(|r| r / 1000) }
            })
            .collect()
    }

    fn set_fullscreen(&mut self, on: bool) {
        let w = &self.gpu.window;
        if on {
            let monitor = w.available_monitors().nth(self.settings.monitor);
            w.set_fullscreen(Some(Fullscreen::Borderless(monitor)));
            self.applied_monitor = self.settings.monitor;
        } else {
            w.set_fullscreen(None);
        }
    }

    /// F11 / bar button: fullscreen on the monitor the window is on right now.
    fn toggle_fullscreen(&mut self) {
        if self.gpu.window.fullscreen().is_some() {
            self.set_fullscreen(false);
            return;
        }
        let w = &self.gpu.window;
        let current = w.current_monitor();
        if let Some(m) = &current {
            if let Some(i) = w.available_monitors().position(|x| x == *m) {
                self.settings.monitor = i;
                self.applied_monitor = i;
            }
        }
        w.set_fullscreen(Some(Fullscreen::Borderless(current)));
    }

    fn show_window(&mut self) {
        let w = &self.gpu.window;
        w.set_visible(true);
        w.set_minimized(false);
        w.focus_window();
        self.occluded = false;
    }

    fn effective_fps_cap(&self) -> u32 {
        let cap = self.settings.fps_cap;
        if self.settings.low_power && (self.occluded || self.battery == Some(true)) {
            return if cap == 0 { 30 } else { cap.min(30) };
        }
        cap
    }

    fn frame_period(&self) -> Option<Duration> {
        let cap = self.effective_fps_cap();
        (cap > 0).then(|| Duration::from_secs_f64(1.0 / cap as f64))
    }

    fn maybe_save(&mut self, force: bool) {
        if force || self.last_save_check.elapsed() > Duration::from_secs(1) {
            self.last_save_check = Instant::now();
            if self.settings != self.last_saved {
                config::save(&self.settings);
                self.last_saved = self.settings.clone();
            }
        }
    }

    fn reload_shaders(&mut self) {
        self.director.reload_shaders(&self.gpu.device, &self.shaders);
        self.post_error = self.post.reload_shaders(&self.gpu.device, &self.shaders).err();
        if self.director.last_error.is_none() && self.post_error.is_none() {
            tracing::info!("shaders reloaded");
        }
    }

    fn diagnostics(&self) -> String {
        format!(
            "audioviz {}\n{}\nGPU: {}\ncapture: {:?}\n{:.0} fps · {}x{} · monitor {} · cap {}\nsettings: {}",
            crate::VERSION,
            platform::diagnostics_header(),
            self.gpu.adapter_name,
            self.engine.capture_mode(),
            self.fps,
            self.gpu.config.width,
            self.gpu.config.height,
            self.settings.monitor + 1,
            self.effective_fps_cap(),
            config::path().map(|p| p.display().to_string()).unwrap_or_default()
        )
    }

    fn set_target(&mut self, target: String) {
        if target.is_empty() || target == self.settings.target_process {
            return;
        }
        match Engine::start(CaptureTarget::Process(target.clone())) {
            Ok(engine) => {
                engine.set_delay_ms(self.settings.audio_delay_ms);
                self.engine = engine;
                let filter = target.trim_end_matches(".exe").to_lowercase();
                if let Ok(m) = audioviz_metadata::default_source().start(&filter) {
                    self.metadata = m;
                }
                self.settings.target_process = target;
                self.last_capture = CaptureMode::Starting;
            }
            Err(e) => self.capture_notice = Some((format!("Could not switch capture: {e}"), Instant::now(), false)),
        }
    }

    fn apply_look(&mut self, look: &Settings) {
        self.settings.apply_look(look);
        let scene = self.settings.scene;
        self.select_scene(scene);
        self.director.auto = self.settings.auto_scenes;
    }

    fn next_look(&mut self) {
        if self.looks.is_empty() {
            return;
        }
        let i = self.looks.iter().position(|l| l.name == self.settings.look).map(|i| (i + 1) % self.looks.len()).unwrap_or(0);
        let look = self.looks[i].clone();
        self.apply_look(&look.settings);
        self.settings.look = look.name.clone();
        self.calibration_result = Some((format!("Look: {}", look.name), Instant::now()));
    }

    fn check_updates(&mut self) {
        let status = self.update.clone();
        if let Ok(mut s) = status.lock() {
            if *s == UpdateStatus::Checking {
                return;
            }
            *s = UpdateStatus::Checking;
        }
        std::thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                let result = match platform::latest_release(REPO) {
                    Ok(r) => {
                        if platform::is_newer(&r.tag, crate::VERSION) {
                            UpdateStatus::Available { tag: r.tag, url: r.url }
                        } else {
                            UpdateStatus::UpToDate(r.tag)
                        }
                    }
                    Err(e) => UpdateStatus::Failed(format!("{e:#}")),
                };
                if let Ok(mut s) = status.lock() {
                    *s = result;
                }
            })
            .ok();
    }

    fn render(&mut self) {
        if self.shaders.poll() {
            self.reload_shaders();
        }
        if (self.settings.audio_delay_ms - self.sent_delay_ms).abs() > 0.01 {
            self.sent_delay_ms = self.settings.audio_delay_ms;
            self.engine.set_delay_ms(self.sent_delay_ms);
        }
        if self.settings.monitor != self.applied_monitor && self.gpu.window.fullscreen().is_some() {
            self.set_fullscreen(true);
        }
        self.director.auto = self.settings.auto_scenes;
        self.maybe_save(false);
        if self.last_battery_check.elapsed() > Duration::from_secs(30) {
            self.last_battery_check = Instant::now();
            self.battery = platform::on_battery();
        }

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(1e-4, 0.25);
        self.last_frame = now;
        self.fps = self.fps * 0.95 + (1.0 / dt) * 0.05;

        // Pull audio and metadata.
        self.engine.drain_points(&mut self.pending_points);
        if self.pending_points.len() > MAX_POINTS - 1 {
            let drop = self.pending_points.len() - (MAX_POINTS - 1);
            self.pending_points.drain(..drop);
        }
        let features = self.engine.features();
        let now_playing = self.metadata.latest();
        if tracing::enabled!(tracing::Level::DEBUG) && (time_floor(self.start.elapsed().as_secs_f32()) != time_floor(self.start.elapsed().as_secs_f32() - dt)) {
            tracing::debug!("peak {:.3} rms {:.3} auto_gain {:.2} points {} silent {} fps {:.0}", features.peak, features.rms, self.auto_gain_value, self.pending_points.len(), features.silent, self.fps);
        }
        let target = if features.peak > 0.0005 { (0.9 / features.peak).clamp(0.3, 200.0) } else { self.auto_gain_value };
        self.auto_gain_value += (target - self.auto_gain_value) * (1.0 - (-dt / 1.5).exp());
        self.director.consider(&features, now_playing.as_ref().map(|n| n.track_id), dt);

        // Track change: toast and art tint.
        let track_id = now_playing.as_ref().map(|n| n.track_id);
        if track_id != self.last_track {
            if let (Some(_), Some(np)) = (self.last_track, now_playing.as_ref()) {
                if !np.title.is_empty() {
                    self.toast = Some((np.title.clone(), np.artist.clone(), Instant::now()));
                }
            }
            self.last_track = track_id;
            self.art_tint = now_playing.as_ref().and_then(|n| n.art.as_ref()).map(|a| a.dominant_color());
        }
        let tint_target = match (self.settings.tint_from_art, self.art_tint) {
            (true, Some(t)) => t,
            _ => self.settings.tint,
        };
        let k = 1.0 - (-dt / 0.8).exp();
        for i in 0..3 {
            self.tint_current[i] += (tint_target[i] - self.tint_current[i]) * k;
        }

        // Capture fallback / failure notice (failures stay).
        let capture = self.engine.capture_mode();
        if capture != self.last_capture {
            self.capture_notice = match &capture {
                CaptureMode::Failed(e) => Some((format!("No audio: {e}"), Instant::now(), true)),
                _ => None,
            };
            self.last_capture = capture.clone();
        }
        if let Some(c) = &mut self.calibration {
            c.resolve(&self.engine.recent_onsets());
        }
        // Tray menu.
        let tray_actions = self.tray.as_ref().map(|t| t.poll()).unwrap_or_default();
        for a in tray_actions {
            match a {
                TrayAction::Show => self.show_window(),
                TrayAction::Scene(i) => {
                    self.show_window();
                    self.director.switch_to(i);
                }
                TrayAction::ToggleAuto => self.settings.auto_scenes = !self.settings.auto_scenes,
                TrayAction::Fullscreen(i) => {
                    self.show_window();
                    self.settings.monitor = i;
                    self.set_fullscreen(true);
                }
                TrayAction::Quit => self.quit = true,
            }
        }
        if let Some(t) = &self.tray {
            t.set_auto(self.settings.auto_scenes);
        }
        let native = hittest::take_events();
        self.chrome.max_hover = native.max_hover;
        if native.toggle_maximize {
            let w = &self.gpu.window;
            w.set_maximized(!w.is_maximized());
        }

        let frame = match self.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.gpu.reconfigure();
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.gpu.recreate_surface();
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => panic!("surface validation error"),
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });

        // --- scenes → composite → post ---
        let time = self.start.elapsed().as_secs_f32();
        {
            let mut frame_settings = self.settings.clone();
            frame_settings.tint = self.tint_current;
            let ctx = FrameCtx { features: &features, settings: &frame_settings, dt, time, width: self.gpu.config.width, height: self.gpu.config.height, points: &self.pending_points, auto_gain: self.auto_gain_value };
            self.director.render(&self.gpu.device, &self.gpu.queue, &mut encoder, &ctx, self.post.input());
        }
        self.points_per_sec = self.points_per_sec * 0.98 + (self.pending_points.len() as f32 / dt) * 0.02;
        self.pending_points.clear();
        let post = {
            let cur = self.settings.post_for(self.director.current);
            match self.director.fade_state() {
                Some((to, t)) => cur.lerp(self.settings.post_for(to), t),
                None => cur,
            }
        };
        let mut flags = 0u32;
        if post.auto_exposure {
            flags |= 1;
        }
        if self.settings.reduce_flashing {
            flags |= 2;
        }
        self.post.render(&self.gpu.queue, &mut encoder, &view, PostParams { exposure: post.exposure, bloom: post.bloom, grain: post.grain, time, dt, flags, flash_rise: self.settings.flash_rise, ..Default::default() });

        // --- egui: overlays, chrome, strip, panel ---
        let fullscreen = self.gpu.window.fullscreen().is_some();
        let maximized = self.gpu.window.is_maximized();
        let overlay_on = self.settings.show_now_playing && self.settings.type_overlay && (self.director.current == STARFIELD_SCENE || self.director.is_fading());
        let calibrating = self.calibration.is_some();
        let empty = matches!(capture, CaptureMode::Endpoint { .. }) && self.first_run.is_none() && !calibrating;
        let ui_view = frame.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(self.gpu.ui_format), ..Default::default() });
        let raw_input = self.egui_state.take_egui_input(&self.gpu.window);
        let shader_error = match (&self.director.last_error, &self.post_error) {
            (None, None) => None,
            (a, b) => Some([a.clone(), b.clone()].into_iter().flatten().collect::<Vec<_>>().join("\n")),
        };
        let stats = Stats {
            fps: self.fps,
            points_per_sec: self.points_per_sec,
            capture: capture.clone(),
            shader_error,
            sample_rate: self.engine.sample_rate,
            monitors: self.monitors(),
            fullscreen,
            settings_path: config::path().map(|p| p.display().to_string()).unwrap_or_default(),
            scene_names: self.director.scenes.iter().map(|s| s.name()).collect(),
            scene_current: self.director.target(),
            scene_fading: self.director.is_fading(),
            last_cue: self.director.last_cue,
            now_playing: now_playing.as_ref().map(|n| format!("{} — {}{}", n.artist, n.title, if n.playing { "" } else { " (paused)" })),
            calibration: self.calibration.as_ref().map(|c| c.view()),
            looks: self.looks.clone(),
            update: self.update.lock().map(|u| u.clone()).unwrap_or_default(),
            diagnostics: self.diagnostics(),
            on_battery: self.battery,
        };
        let notices = self.notices();
        let pinned = self.settings.show_ui || calibrating || self.first_run.is_some() || self.show_shortcuts || self.start.elapsed().as_secs_f32() < 5.0;
        let chrome_wanted = !self.zen && self.chrome.wants_visible(&self.egui_ctx, fullscreen, self.settings.chrome_autohide, pinned);
        // Cursor: gone in fullscreen once everything is hidden and the mouse rests.
        let want_cursor_hidden = fullscreen && !chrome_wanted && !self.settings.show_ui && self.first_run.is_none() && !calibrating && self.chrome.last_mouse.elapsed().as_secs_f32() > theme::AUTOHIDE_S;
        if want_cursor_hidden != self.cursor_hidden {
            self.cursor_hidden = want_cursor_hidden;
            self.gpu.window.set_cursor_visible(!want_cursor_hidden);
        }
        let toast_on = !self.zen && self.settings.show_now_playing && self.settings.toast && !(overlay_on) && self.toast.as_ref().is_some_and(|(_, _, t)| t.elapsed().as_secs_f32() < TOAST_SECONDS);
        let accent = crate::chrome::accent(self.tint_current);
        let target_short = self.settings.target_process.trim_end_matches(".exe").to_lowercase();

        let mut chrome_actions = crate::chrome::ChromeActions::default();
        let mut panel_actions = panel::Actions::default();
        let mut strip_actions = crate::strip::StripActions::default();
        let mut empty_actions = overlays::EmptyActions::default();
        let mut cal_actions = overlays::CalActions::default();
        let mut fr_actions = overlays::FirstRunActions::default();
        let mut close_shortcuts = false;
        let mut regions = hittest::Regions::default();

        let settings = &mut self.settings;
        let director = &mut self.director;
        let overlay = &mut self.overlay;
        let chrome = &mut self.chrome;
        let strip = &mut self.strip;
        let panel_state = &mut self.panel_state;
        let first_run = &mut self.first_run;
        let show_shortcuts = self.show_shortcuts;
        let toast = self.toast.clone();
        let calibration_view = stats.calibration.clone();
        let current_delay = settings.audio_delay_ms;
        let spotify_installed = self.spotify_installed;
        let tint = self.tint_current;
        let beat = features.beat_pulse;
        let egui::FullOutput { platform_output, mut textures_delta, shapes, pixels_per_point, .. } = self.egui_ctx.run_pass(raw_input, |ctx| {
            let t_chrome = ctx.animate_bool_with_time(egui::Id::unique("t-chrome"), chrome_wanted, theme::ANIM_S);
            let t_panel = ctx.animate_bool_with_time(egui::Id::unique("t-panel"), settings.show_ui, theme::ANIM_S);
            let t_strip = ctx.animate_bool_with_time(egui::Id::unique("t-strip"), chrome_wanted && !empty && now_playing.is_some(), theme::ANIM_S);
            let t_toast = ctx.animate_bool_with_time(egui::Id::unique("t-toast"), toast_on, theme::ANIM_S * 2.0);
            let bottom = STRIP_H * t_strip;
            let right = PANEL_W * t_panel;

            if overlay_on {
                overlay.draw(ctx, now_playing.as_ref(), &features, tint, settings.type_scale, BAR_H * t_chrome, bottom, right);
            }
            if let Some((title, artist, _)) = &toast {
                let art = strip.art_texture(ctx, now_playing.as_ref());
                overlays::toast(ctx, art, title, artist, t_toast, bottom);
            }
            if empty {
                let device = match &capture {
                    CaptureMode::Endpoint { device } => device.clone(),
                    _ => String::new(),
                };
                empty_actions = overlays::empty_state(ctx, &settings.target_process, &device, spotify_installed);
            }
            strip_actions = strip.draw(ctx, now_playing.as_ref(), &mut settings.tint_from_art, settings.show_now_playing, accent, t_strip, right);
            let shown = director.target();
            panel_actions = panel::draw(ctx, settings, panel_state, &features, &stats, &mut *director.scenes[shown], t_panel);
            let input = ChromeInput {
                scene_names: &stats.scene_names,
                scene_current: stats.scene_current,
                auto: settings.auto_scenes,
                capture: &capture,
                target_short: &target_short,
                sync_ms: settings.audio_delay_ms,
                monitor_index: settings.monitor,
                fps: stats.fps,
                maximized,
                accent,
                layers: settings.layer_mode.then(|| (0..stats.scene_names.len()).map(|i| settings.layer(i).enabled && settings.layer(i).opacity > 0.0).collect()),
            };
            chrome_actions = chrome.draw(ctx, &input, t_chrome);
            if let Some(view) = &calibration_view {
                cal_actions = overlays::calibration(ctx, view, current_delay, beat);
            }
            if show_shortcuts {
                close_shortcuts = overlays::shortcuts(ctx);
            }
            if let Some(fr) = first_run {
                fr_actions = overlays::first_run(ctx, fr, &stats.monitors, settings.monitor, settings.start_fullscreen, &stats.scene_names, stats.scene_current);
            }
            overlays::notices(ctx, &notices, bottom);

            // Native hit regions for this frame.
            let ppp = ctx.pixels_per_point();
            regions = hittest::Regions {
                enabled: !fullscreen && t_chrome > 0.5,
                resizable: !fullscreen,
                bar: hittest::rect_px(chrome.regions.bar, ppp),
                interactive: chrome.regions.interactive.iter().map(|r| hittest::rect_px(*r, ppp)).collect(),
                max_button: hittest::rect_px(chrome.regions.max_button, ppp),
                border: (6.0 * ppp) as i32,
            };
        });
        hittest::publish(regions);
        self.egui_state.handle_platform_output(&self.gpu.window, platform_output);
        let primitives = self.egui_ctx.tessellate(shapes, pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [self.gpu.config.width, self.gpu.config.height], pixels_per_point };
        for (id, deltas) in textures_delta.set.drain() {
            for delta in deltas.iter() {
                self.egui_renderer.update_texture(&self.gpu.device, &self.gpu.queue, id, delta);
            }
        }
        let extra = self.egui_renderer.update_buffers(&self.gpu.device, &self.gpu.queue, &mut encoder, &primitives, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &ui_view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut pass = pass.forget_lifetime();
            self.egui_renderer.render(&mut pass, &primitives, &screen);
        }
        for id in textures_delta.free.drain() {
            self.egui_renderer.free_texture(&id);
        }
        self.gpu.queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        self.gpu.window.pre_present_notify();
        self.gpu.queue.present(frame);

        // --- actions ---
        if chrome_actions.start_drag {
            let _ = self.gpu.window.drag_window();
        }
        if chrome_actions.minimize {
            self.gpu.window.set_minimized(true);
        }
        if chrome_actions.toggle_maximize {
            let w = &self.gpu.window;
            w.set_maximized(!w.is_maximized());
        }
        if chrome_actions.close {
            self.quit = true;
        }
        if chrome_actions.toggle_auto {
            self.settings.auto_scenes = !self.settings.auto_scenes;
        }
        if chrome_actions.hide_ui || panel_actions.hide_ui {
            self.set_zen(true);
        }
        if let Some(s) = chrome_actions.open_section {
            self.settings.section = s;
            self.settings.show_ui = true;
        }
        if let Some(i) = chrome_actions.switch_scene.or(panel_actions.switch_scene) {
            self.select_scene(i);
        }
        if panel_actions.toggle_fullscreen {
            self.toggle_fullscreen();
        }
        if panel_actions.calibrate_toggle || cal_actions.cancel {
            self.toggle_calibration();
        }
        if panel_actions.calibrate_apply || cal_actions.apply {
            self.apply_calibration();
        }
        if let Some(look) = panel_actions.apply_look {
            self.apply_look(&look);
        }
        if let Some(name) = panel_actions.save_look {
            match looks::save(&name, &self.settings) {
                Ok(n) => {
                    self.settings.look = n;
                    self.panel_state.look_name.clear();
                    self.looks = looks::all();
                }
                Err(e) => self.calibration_result = Some((format!("Could not save look: {e}"), Instant::now())),
            }
        }
        if let Some(name) = panel_actions.delete_look {
            if let Err(e) = looks::delete(&name) {
                tracing::warn!("delete look: {e}");
            }
            self.looks = looks::all();
        }
        if let Some(url) = panel_actions.open_url {
            if let Err(e) = platform::open_url(&url) {
                tracing::warn!("open url: {e}");
            }
        }
        if panel_actions.open_looks_folder {
            if let Some(d) = looks::dir() {
                let _ = std::fs::create_dir_all(&d);
                let _ = platform::open_url(&d.display().to_string());
            }
        }
        if panel_actions.check_updates {
            self.check_updates();
        }
        if panel_actions.copy_diagnostics {
            self.egui_ctx.copy_text(self.diagnostics());
            self.calibration_result = Some(("Diagnostics copied.".into(), Instant::now()));
        }
        if panel_actions.launch_spotify || empty_actions.open_spotify {
            if let Err(e) = platform::launch_spotify() {
                self.capture_notice = Some((format!("Could not open Spotify: {e}"), Instant::now(), false));
            }
        }
        if empty_actions.pick_app {
            self.settings.section = Section::Sync;
            self.settings.show_ui = true;
        }
        if let Some(t) = panel_actions.set_target {
            self.set_target(t);
        }
        if panel_actions.show_shortcuts || strip_actions.shortcuts {
            self.show_shortcuts = true;
        }
        if close_shortcuts {
            self.show_shortcuts = false;
        }
        if panel_actions.rerun_first_run {
            self.first_run = Some(FirstRun::default());
        }
        if let Some(cmd) = strip_actions.command {
            self.metadata.control(cmd);
        }
        if let Some(m) = fr_actions.monitor {
            self.settings.monitor = m;
        }
        if let Some(sf) = fr_actions.start_fullscreen {
            self.settings.start_fullscreen = sf;
        }
        if let Some(i) = fr_actions.scene {
            self.select_scene(i);
        }
        if fr_actions.calibrate {
            self.first_run = None;
            self.settings.onboarded = true;
            if self.calibration.is_none() {
                self.toggle_calibration();
            }
        }
        if fr_actions.done {
            self.first_run = None;
            self.settings.onboarded = true;
        }
        self.settings.scene = self.director.target();
    }

    /// Bottom-left one-liners: capture failure, calibration result, look name.
    fn notices(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some((msg, since, persistent)) = &self.capture_notice {
            if *persistent || since.elapsed().as_secs_f32() < NOTICE_SECONDS {
                out.push(msg.clone());
            }
        }
        if let Some((msg, since)) = &self.calibration_result {
            if since.elapsed().as_secs_f32() < NOTICE_SECONDS {
                out.push(msg.clone());
            }
        }
        out
    }

    /// Hide (or show) every UI element. Hiding also closes the panel.
    fn set_zen(&mut self, on: bool) {
        self.zen = on;
        if on {
            self.settings.show_ui = false;
            self.show_shortcuts = false;
            self.calibration_result = Some(("UI hidden · H or Tab shows it again".into(), Instant::now() - Duration::from_secs_f32(NOTICE_SECONDS - 2.5)));
        }
    }

    /// Keys / pips / panel: switch scene, or in layer mode just focus it for editing.
    fn select_scene(&mut self, i: usize) {
        if self.settings.layer_mode {
            self.director.focus(i);
        } else {
            self.director.switch_to(i);
        }
    }

    fn toggle_calibration(&mut self) {
        self.calibration = match self.calibration {
            Some(_) => None,
            None => Some(Calibration::new()),
        };
    }

    fn apply_calibration(&mut self) {
        let Some(c) = &self.calibration else { return };
        let view = c.view();
        let Some(offset) = view.offset_ms else { return };
        let before = self.settings.audio_delay_ms;
        self.settings.audio_delay_ms = (before + offset).clamp(0.0, 300.0);
        self.calibration_result = Some((format!("Audio delay {:.0} → {:.0} ms ({} taps, ±{:.0} ms)", before, self.settings.audio_delay_ms, view.matched, view.spread_ms), Instant::now()));
        tracing::info!("calibration applied: delay {:.1} -> {:.1} ms from {} matched taps (spread {:.1} ms)", before, self.settings.audio_delay_ms, view.matched, view.spread_ms);
        self.calibration = None;
    }

    fn key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        match code {
            // Esc steps outward: sheet → panel → fullscreen → quit.
            KeyCode::Escape => {
                if self.show_shortcuts {
                    self.show_shortcuts = false;
                } else if self.calibration.is_some() {
                    self.calibration = None;
                } else if self.first_run.is_some() {
                    self.first_run = None;
                    self.settings.onboarded = true;
                } else if self.settings.show_ui {
                    self.settings.show_ui = false;
                } else if self.gpu.window.fullscreen().is_some() {
                    self.set_fullscreen(false);
                } else {
                    event_loop.exit();
                }
            }
            KeyCode::F11 => self.toggle_fullscreen(),
            KeyCode::Tab => {
                if self.zen {
                    self.zen = false;
                    self.settings.show_ui = true;
                } else {
                    self.settings.show_ui = !self.settings.show_ui;
                }
            }
            KeyCode::KeyH => self.set_zen(!self.zen),
            KeyCode::F5 => {
                self.shaders = ShaderSet::load();
                self.reload_shaders();
            }
            KeyCode::Space => match &mut self.calibration {
                Some(c) => c.tap(),
                None => {
                    let n = self.director.scenes.len();
                    let next = (self.director.target() + 1) % n.max(1);
                    self.select_scene(next);
                }
            },
            KeyCode::Enter | KeyCode::NumpadEnter => self.apply_calibration(),
            KeyCode::KeyC => self.toggle_calibration(),
            KeyCode::KeyA => self.settings.auto_scenes = !self.settings.auto_scenes,
            KeyCode::KeyT => self.settings.type_overlay = !self.settings.type_overlay,
            KeyCode::KeyN => self.settings.show_now_playing = !self.settings.show_now_playing,
            KeyCode::KeyL => self.next_look(),
            KeyCode::Slash | KeyCode::F1 => self.show_shortcuts = !self.show_shortcuts,
            KeyCode::Digit1 => self.select_scene(0),
            KeyCode::Digit2 => self.select_scene(1),
            KeyCode::Digit3 => self.select_scene(2),
            KeyCode::Digit4 => self.select_scene(3),
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let (w, h) = std::env::var("AUDIOVIZ_SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)))).unwrap_or((1280.0, 800.0));
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes()
            .with_title("audioviz")
            .with_inner_size(winit::dpi::PhysicalSize::new(w, h))
            .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 400.0))
            .with_decorations(false)
            .with_window_icon(window_icon());
        #[cfg(windows)]
        {
            use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
            attrs = attrs.with_undecorated_shadow(true).with_corner_preference(CornerPreference::Round);
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        match Running::new(window, self.start_in_tray) {
            Ok(r) => self.state = Some(r),
            Err(e) => {
                tracing::error!("startup failed: {e:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(st) = self.state.as_mut() else { return };

        let resp = st.egui_state.on_window_event(&st.gpu.window, &event);
        if matches!(event, WindowEvent::CursorMoved { .. }) {
            st.chrome.note_mouse();
        }
        if resp.consumed && !matches!(event, WindowEvent::RedrawRequested | WindowEvent::Resized(_) | WindowEvent::CloseRequested | WindowEvent::Occluded(_)) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Occluded(o) => st.occluded = o,
            WindowEvent::Resized(size) => {
                st.gpu.resize(size.width, size.height);
                let (w, h) = (st.gpu.config.width, st.gpu.config.height);
                st.director.resize(&st.gpu.device, w, h);
                st.post.resize(&st.gpu.device, w, h);
            }
            WindowEvent::RedrawRequested => {
                st.render();
                if st.quit {
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(code), state: ElementState::Pressed, repeat: false, .. }, .. } => {
                if !st.egui_ctx.egui_wants_keyboard_input() {
                    st.key(code, event_loop);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(st) = &self.state else { return };
        match st.frame_period() {
            None => {
                event_loop.set_control_flow(ControlFlow::Poll);
                st.gpu.window.request_redraw();
            }
            Some(period) => {
                let next = st.last_frame + period;
                if Instant::now() >= next {
                    st.gpu.window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(next));
            }
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(st) = self.state.as_mut() {
            st.maybe_save(true);
        }
    }
}
