use std::sync::Arc;
use std::time::{Duration, Instant};

use audioviz_audio::CaptureTarget;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::config;
use crate::engine::Engine;
use crate::gpu::Gpu;
use crate::scope::{MAX_POINTS, Params, Scope};
use crate::shaders::ShaderSet;
use crate::ui::{Settings, Stats};

struct Running {
    gpu: Gpu,
    scope: Scope,
    shaders: ShaderSet,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,

    engine: Engine,
    /// (L, R) points waiting to be drawn, newest last.
    pending_points: Vec<[f32; 2]>,
    last_point: [f32; 2],

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
        let settings = config::load();
        let gpu = Gpu::new(window.clone())?;
        let shaders = ShaderSet::load();
        let scope = Scope::new(&gpu.device, gpu.config.format, gpu.config.width, gpu.config.height, &shaders)?;

        let egui_ctx = egui::Context::default();
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

        let mut running = Self {
            gpu,
            scope,
            shaders,
            egui_ctx,
            egui_state,
            egui_renderer,
            engine,
            pending_points: Vec::with_capacity(MAX_POINTS),
            last_point: [0.0, 0.0],
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

    fn render(&mut self) {
        if self.shaders.poll() {
            self.scope.reload_shaders(&self.gpu.device, &self.shaders);
        }
        if (self.settings.audio_delay_ms - self.sent_delay_ms).abs() > 0.01 {
            self.sent_delay_ms = self.settings.audio_delay_ms;
            self.engine.set_delay_ms(self.sent_delay_ms);
        }
        if self.settings.monitor != self.applied_monitor && self.gpu.window.fullscreen().is_some() {
            self.set_fullscreen(true);
        }
        self.maybe_save(false);

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(1e-4, 0.25);
        self.last_frame = now;
        self.fps = self.fps * 0.95 + (1.0 / dt) * 0.05;

        // Pull audio.
        self.engine.drain_points(&mut self.pending_points);
        if self.pending_points.len() > MAX_POINTS - 1 {
            let drop = self.pending_points.len() - (MAX_POINTS - 1);
            self.pending_points.drain(..drop);
        }
        let features = self.engine.features();

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

        // --- scope ---
        let target = if features.peak > 0.02 { (0.9 / features.peak).clamp(0.3, 6.0) } else { self.auto_gain_value };
        self.auto_gain_value += (target - self.auto_gain_value) * (1.0 - (-dt / 1.5).exp());
        let gain = if self.settings.auto_gain { self.settings.gain * self.auto_gain_value } else { self.settings.gain };
        let pulse = 1.0 + self.settings.beat_pulse * features.beat_pulse * features.beat_confidence;

        let params = Params {
            resolution: [self.gpu.config.width as f32, self.gpu.config.height as f32],
            gain,
            rotation: self.settings.rotation_deg.to_radians(),
            beam_width: self.settings.beam_width,
            decay: self.settings.persistence.max(1e-6).powf(dt),
            beam_intensity: self.settings.beam_intensity,
            exposure: self.settings.exposure * pulse,
            tint: [self.settings.tint[0], self.settings.tint[1], self.settings.tint[2], 1.0],
            bloom: self.settings.bloom,
            grain: self.settings.grain,
            time: self.start.elapsed().as_secs_f32(),
            dt,
            point_count: 0,
            flags: if self.settings.auto_exposure { 1 } else { 0 },
            _pad: [0; 2],
        };
        let mut points = Vec::with_capacity(self.pending_points.len() + 1);
        points.push(self.last_point);
        points.extend_from_slice(&self.pending_points);
        if let Some(p) = self.pending_points.last() {
            self.last_point = *p;
        }
        self.points_per_sec = self.points_per_sec * 0.98 + (self.pending_points.len() as f32 / dt) * 0.02;
        self.pending_points.clear();
        self.scope.render(&self.gpu.queue, &mut encoder, &view, params, &points);

        // --- egui ---
        let mut toggle_fullscreen = false;
        if self.settings.show_ui {
            let ui_view = frame.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(self.gpu.ui_format), ..Default::default() });
            let raw_input = self.egui_state.take_egui_input(&self.gpu.window);
            let stats = Stats {
                fps: self.fps,
                points_per_sec: self.points_per_sec,
                capture: self.engine.capture_mode(),
                shader_error: self.scope.last_shader_error.clone(),
                sample_rate: self.engine.sample_rate,
                monitors: self.monitor_names(),
                fullscreen: self.gpu.window.fullscreen().is_some(),
                settings_path: config::path().map(|p| p.display().to_string()).unwrap_or_default(),
            };
            let settings = &mut self.settings;
            let mut actions = crate::ui::Actions::default();
            let egui::FullOutput { platform_output, mut textures_delta, shapes, pixels_per_point, .. } =
                self.egui_ctx.run_pass(raw_input, |ctx| actions = crate::ui::draw(ctx, settings, &features, &stats));
            toggle_fullscreen = actions.toggle_fullscreen;
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

        if toggle_fullscreen {
            self.toggle_fullscreen();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("audioviz")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
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
                st.scope.resize(&st.gpu.device, st.gpu.config.width, st.gpu.config.height);
            }
            WindowEvent::RedrawRequested => st.render(),
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(code), state: ElementState::Pressed, repeat: false, .. }, .. } => match code {
                KeyCode::Escape => event_loop.exit(),
                KeyCode::F11 => st.toggle_fullscreen(),
                KeyCode::Tab => st.settings.show_ui = !st.settings.show_ui,
                KeyCode::F5 => {
                    st.shaders = ShaderSet::load();
                    st.scope.reload_shaders(&st.gpu.device, &st.shaders);
                }
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
