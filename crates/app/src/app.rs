use std::sync::Arc;
use std::time::{Duration, Instant};

use audioviz_audio::{CaptureMode, CaptureTarget};
use audioviz_metadata::MetadataHandle;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::config;
use crate::director::Director;
use crate::engine::Engine;
use crate::gpu::Gpu;
use crate::overlay::Overlay;
use crate::post::{Post, PostParams};
use crate::scene::FrameCtx;
use crate::scenes::ribbons::Ribbons;
use crate::scenes::scope::{MAX_POINTS, Scope};
use crate::scenes::starfield::Starfield;
use crate::shaders::ShaderSet;
use crate::ui::{CALIBRATION_MIN_TAPS, CalibrationView, Settings, Stats};

/// Scene index of the starfield (the one with the type overlay).
const STARFIELD_SCENE: usize = 2;
/// How long the first-run hint and a capture-fallback notice stay on screen.
const HINT_SECONDS: f32 = 12.0;
const NOTICE_SECONDS: f32 = 10.0;

/// Tap-along audio-delay calibration. The user taps Space on beats they hear;
/// each tap is matched to the nearest onset the analyzer detected (after the
/// current delay line), and the median residual is the correction to apply.
struct Calibration {
    /// Taps not yet matched (we wait a little in case the onset lands late).
    pending: Vec<Instant>,
    /// Matched residuals, ms (positive = tap after onset = visuals lead the ear).
    residuals: Vec<f32>,
    taps: usize,
}

impl Calibration {
    const MATCH_WINDOW_MS: f32 = 180.0;
    const SETTLE_MS: u64 = 250;

    fn new() -> Self {
        Self { pending: Vec::new(), residuals: Vec::new(), taps: 0 }
    }

    fn tap(&mut self) {
        self.taps += 1;
        self.pending.push(Instant::now());
    }

    /// Resolve taps older than the settle window against the onset log.
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
            let best = onsets
                .iter()
                .map(|o| signed_ms(tap, *o))
                .filter(|d| d.abs() <= Self::MATCH_WINDOW_MS)
                .min_by(|a, b| a.abs().total_cmp(&b.abs()));
            if let Some(d) = best {
                self.residuals.push(d);
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
            offset_ms: (self.residuals.len() >= CALIBRATION_MIN_TAPS).then_some(offset),
            spread_ms: median(&dev),
        }
    }
}

/// `a - b` in milliseconds, signed.
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

    engine: Engine,
    metadata: MetadataHandle,
    /// (L, R) points waiting to be drawn, newest last.
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
    first_run: bool,
    calibration: Option<Calibration>,
    /// One-line result shown briefly after applying a calibration.
    calibration_result: Option<(String, Instant)>,
    last_capture: CaptureMode,
    /// (message, since, persistent) for capture fallback / failure.
    capture_notice: Option<(String, Instant, bool)>,
}

fn time_floor(t: f32) -> i32 {
    t.floor() as i32
}

/// The taskbar / title-bar icon, embedded from `assets/icon-256.png`
/// (regenerate with `cargo xtask icon`).
fn window_icon() -> Option<winit::window::Icon> {
    const PNG: &[u8] = include_bytes!("../../../assets/icon-256.png");
    let img = image::load_from_memory_with_format(PNG, image::ImageFormat::Png).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), w, h).ok()
}

pub struct App {
    state: Option<Running>,
}

impl App {
    pub fn new() -> Self {
        Self { state: None }
    }
}

impl Running {
    fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let config::Loaded { settings, first_run } = config::load();
        let gpu = Gpu::new(window.clone())?;
        let shaders = ShaderSet::load();
        let (w, h) = (gpu.config.width, gpu.config.height);
        let scope = Scope::new(&gpu.device, w, h, &shaders).map_err(|e| anyhow::anyhow!("scope: {e}"))?;
        let ribbons = Ribbons::new(&gpu.device, w, h, &shaders).map_err(|e| anyhow::anyhow!("ribbons: {e}"))?;
        let starfield = Starfield::new(&gpu.device, w, h, &shaders).map_err(|e| anyhow::anyhow!("starfield: {e}"))?;
        let mut director = Director::new(&gpu.device, &shaders, vec![Box::new(scope), Box::new(ribbons), Box::new(starfield)]).map_err(|e| anyhow::anyhow!("director: {e}"))?;
        // Start on the remembered scene; AUDIOVIZ_SCENE=n overrides (dev).
        let start_scene = std::env::var("AUDIOVIZ_SCENE").ok().and_then(|v| v.parse().ok()).unwrap_or(settings.scene);
        director.current = start_scene.min(director.scenes.len() - 1);
        director.auto = settings.auto_scenes;
        let post = Post::new(&gpu.device, gpu.config.format, w, h, &shaders).map_err(|e| anyhow::anyhow!("post: {e}"))?;

        let egui_ctx = egui::Context::default();
        crate::overlay::install_fonts(&egui_ctx);
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(gpu.device.limits().max_texture_dimension_2d as usize),
        );
        let egui_renderer = egui_wgpu::Renderer::new(&gpu.device, gpu.ui_format, egui_wgpu::RendererOptions::default());

        let engine = Engine::start(CaptureTarget::Process(settings.target_process.clone()))?;
        if settings.audio_delay_ms > 0.0 {
            engine.set_delay_ms(settings.audio_delay_ms);
        }
        let app_filter = settings.target_process.trim_end_matches(".exe").to_lowercase();
        let metadata = audioviz_metadata::default_source().start(&app_filter)?;

        let mut running = Self {
            gpu,
            director,
            post,
            shaders,
            egui_ctx,
            egui_state,
            egui_renderer,
            overlay: Overlay::new(),
            engine,
            metadata,
            pending_points: Vec::with_capacity(MAX_POINTS),
            sent_delay_ms: settings.audio_delay_ms,
            applied_monitor: settings.monitor,
            last_saved: settings.clone(),
            last_save_check: Instant::now(),
            settings,
            start: Instant::now(),
            last_frame: Instant::now(),
            fps: 0.0,
            points_per_sec: 0.0,
            auto_gain_value: 1.0,
            post_error: None,
            first_run,
            calibration: None,
            calibration_result: None,
            last_capture: CaptureMode::Starting,
            capture_notice: None,
        };
        if running.settings.start_fullscreen {
            running.set_fullscreen(true);
        }
        Ok(running)
    }

    fn monitor_names(&self) -> Vec<String> {
        self.gpu
            .window
            .available_monitors()
            .enumerate()
            .map(|(i, m)| {
                let size = m.size();
                let hz = m.refresh_rate_millihertz().map(|r| format!(" @ {} Hz", r / 1000)).unwrap_or_default();
                format!("{}: {} ({}x{}{})", i, m.name().unwrap_or_else(|| "display".into()), size.width, size.height, hz)
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

    /// F11 / panel button: fullscreen on the monitor the window is on right
    /// now, and remember that monitor for "start fullscreen".
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

    fn frame_period(&self) -> Option<Duration> {
        (self.settings.fps_cap > 0).then(|| Duration::from_secs_f64(1.0 / self.settings.fps_cap as f64))
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
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for p in &self.pending_points {
                lo = lo.min(p[0]).min(p[1]);
                hi = hi.max(p[0]).max(p[1]);
            }
            tracing::debug!("peak {:.3} rms {:.3} auto_gain {:.2} points {} range [{:.3}, {:.3}] silent {}", features.peak, features.rms, self.auto_gain_value, self.pending_points.len(), lo, hi, features.silent);
        }
        // Spotify's own volume scales what loopback captures, so the trace must
        // be normalized over a wide range (a -45 dBFS signal still fills the screen).
        let target = if features.peak > 0.0005 { (0.9 / features.peak).clamp(0.3, 200.0) } else { self.auto_gain_value };
        self.auto_gain_value += (target - self.auto_gain_value) * (1.0 - (-dt / 1.5).exp());
        self.director.consider(&features, now_playing.as_ref().map(|n| n.track_id), dt);

        // Capture fallback / failure notice (shown briefly on each change; failures stay).
        let capture = self.engine.capture_mode();
        if capture != self.last_capture {
            self.capture_notice = match &capture {
                CaptureMode::Endpoint { device } => Some((format!("{} is not playing: visualizing '{}'", self.settings.target_process, device), Instant::now(), false)),
                CaptureMode::Failed(e) => Some((format!("No audio: {e}"), Instant::now(), true)),
                CaptureMode::Process { .. } | CaptureMode::Starting => None,
            };
            self.last_capture = capture.clone();
        }
        if let Some(c) = &mut self.calibration {
            c.resolve(&self.engine.recent_onsets());
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
            let ctx = FrameCtx {
                features: &features,
                settings: &self.settings,
                dt,
                time,
                width: self.gpu.config.width,
                height: self.gpu.config.height,
                points: &self.pending_points,
                auto_gain: self.auto_gain_value,
            };
            self.director.render(&self.gpu.device, &self.gpu.queue, &mut encoder, &ctx, self.post.input());
        }
        self.points_per_sec = self.points_per_sec * 0.98 + (self.pending_points.len() as f32 / dt) * 0.02;
        self.pending_points.clear();
        // Per-scene post settings, blended along the cross-fade.
        let post = {
            let cur = self.settings.post_for(self.director.current);
            match self.director.fade_state() {
                Some((to, t)) => cur.lerp(self.settings.post_for(to), t),
                None => cur,
            }
        };
        self.post.render(
            &self.gpu.queue,
            &mut encoder,
            &view,
            PostParams {
                exposure: post.exposure,
                bloom: post.bloom,
                grain: post.grain,
                time,
                dt,
                flags: if post.auto_exposure { 1 } else { 0 },
                _pad: [0; 2],
            },
        );

        // --- egui: type overlay (starfield scene) and the debug panel ---
        let overlay_on = self.settings.type_overlay && (self.director.current == STARFIELD_SCENE || self.director.is_fading());
        let mut actions = crate::ui::Actions::default();
        let notices = self.notices();
        if self.settings.show_ui || overlay_on || !notices.is_empty() {
            let ui_view = frame.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(self.gpu.ui_format), ..Default::default() });
            let raw_input = self.egui_state.take_egui_input(&self.gpu.window);
            let shader_error = match (&self.director.last_error, &self.post_error) {
                (None, None) => None,
                (a, b) => Some([a.clone(), b.clone()].into_iter().flatten().collect::<Vec<_>>().join("\n")),
            };
            let stats = Stats {
                fps: self.fps,
                points_per_sec: self.points_per_sec,
                capture: self.engine.capture_mode(),
                shader_error,
                sample_rate: self.engine.sample_rate,
                monitors: self.monitor_names(),
                fullscreen: self.gpu.window.fullscreen().is_some(),
                settings_path: config::path().map(|p| p.display().to_string()).unwrap_or_default(),
                scene_names: self.director.scenes.iter().map(|s| s.name()).collect(),
                scene_current: self.director.current,
                scene_fading: self.director.is_fading(),
                last_cue: self.director.last_cue,
                now_playing: now_playing.as_ref().map(|n| format!("♪ {} — {}{}", n.artist, n.title, if n.playing { "" } else { " (paused)" })),
                calibration: self.calibration.as_ref().map(|c| c.view()),
            };
            let show_ui = self.settings.show_ui;
            let settings = &mut self.settings;
            let director = &mut self.director;
            let overlay = &mut self.overlay;
            let tint = settings.tint;
            let type_scale = settings.type_scale;
            let egui::FullOutput { platform_output, mut textures_delta, shapes, pixels_per_point, .. } = self.egui_ctx.run_pass(raw_input, |ctx| {
                if overlay_on {
                    overlay.draw(ctx, now_playing.as_ref(), &features, tint, type_scale);
                }
                if show_ui {
                    actions = crate::ui::draw(ctx, settings, &features, &stats, &mut *director.scenes[director.current]);
                }
                crate::ui::draw_notices(ctx, &notices);
            });
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
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &ui_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
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
        } else {
            self.gpu.queue.submit(std::iter::once(encoder.finish()));
        }

        self.gpu.window.pre_present_notify();
        self.gpu.queue.present(frame);

        if actions.toggle_fullscreen {
            self.toggle_fullscreen();
        }
        if let Some(i) = actions.switch_scene {
            self.director.switch_to(i);
        }
        if actions.calibrate_toggle {
            self.toggle_calibration();
        }
        if actions.calibrate_apply {
            self.apply_calibration();
        }
        if !self.director.is_fading() {
            self.settings.scene = self.director.current;
        }
    }

    /// Bottom-left one-liners: first-run hint, capture fallback, calibration
    /// progress when the panel is hidden, and the last calibration result.
    fn notices(&self) -> Vec<String> {
        let mut out = Vec::new();
        let elapsed = self.start.elapsed().as_secs_f32();
        if self.first_run && !self.settings.show_ui && elapsed < HINT_SECONDS {
            out.push("Tab for settings · C to calibrate audio delay · F11 fullscreen".into());
        }
        if let Some((msg, since, persistent)) = &self.capture_notice {
            if *persistent || since.elapsed().as_secs_f32() < NOTICE_SECONDS {
                out.push(msg.clone());
            }
        }
        if let Some(c) = &self.calibration {
            out.push(format!("Calibrating: tap Space on the beat · {} · Enter to apply, C to cancel", crate::ui::calibration_status(&c.view())));
        }
        if let Some((msg, since)) = &self.calibration_result {
            if since.elapsed().as_secs_f32() < NOTICE_SECONDS {
                out.push(msg.clone());
            }
        }
        out
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
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        // AUDIOVIZ_SIZE=WxH overrides the initial window size (dev).
        let (w, h) = std::env::var("AUDIOVIZ_SIZE")
            .ok()
            .and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?))))
            .unwrap_or((1280.0, 800.0));
        let attrs = Window::default_attributes().with_title("audioviz").with_inner_size(winit::dpi::PhysicalSize::new(w, h)).with_window_icon(window_icon());
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        match Running::new(window) {
            Ok(r) => self.state = Some(r),
            Err(e) => {
                tracing::error!("startup failed: {e:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(st) = self.state.as_mut() else { return };

        if st.settings.show_ui {
            let resp = st.egui_state.on_window_event(&st.gpu.window, &event);
            if resp.consumed && !matches!(event, WindowEvent::RedrawRequested | WindowEvent::Resized(_) | WindowEvent::CloseRequested) {
                return;
            }
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                st.gpu.resize(size.width, size.height);
                let (w, h) = (st.gpu.config.width, st.gpu.config.height);
                st.director.resize(&st.gpu.device, w, h);
                st.post.resize(&st.gpu.device, w, h);
            }
            WindowEvent::RedrawRequested => st.render(),
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(code), state: ElementState::Pressed, repeat: false, .. }, .. } => match code {
                KeyCode::Escape => event_loop.exit(),
                KeyCode::F11 => st.toggle_fullscreen(),
                KeyCode::Tab => st.settings.show_ui = !st.settings.show_ui,
                KeyCode::F5 => {
                    st.shaders = ShaderSet::load();
                    st.reload_shaders();
                }
                // While calibrating, Space is the tap key and Enter applies.
                KeyCode::Space => match &mut st.calibration {
                    Some(c) => c.tap(),
                    None => st.director.next(),
                },
                KeyCode::Enter | KeyCode::NumpadEnter => st.apply_calibration(),
                KeyCode::KeyC => st.toggle_calibration(),
                KeyCode::KeyA => st.settings.auto_scenes = !st.settings.auto_scenes,
                KeyCode::KeyT => st.settings.type_overlay = !st.settings.type_overlay,
                KeyCode::Digit1 => st.director.switch_to(0),
                KeyCode::Digit2 => st.director.switch_to(1),
                KeyCode::Digit3 => st.director.switch_to(2),
                KeyCode::Digit4 => st.director.switch_to(3),
                _ => {}
            },
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
