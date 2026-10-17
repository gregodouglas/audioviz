use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use audioviz_analysis::{Analyzer, AnalyzerConfig};
use audioviz_audio::{AudioSource, CaptureHandle, CaptureMode, CaptureTarget};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::gpu::Gpu;
use crate::scope::{MAX_POINTS, Params, Scope};
use crate::shaders::ShaderSet;
use crate::ui::{Settings, Stats};

const CHUNK_FRAMES: usize = 256;

struct Running {
    gpu: Gpu,
    scope: Scope,
    shaders: ShaderSet,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    show_ui: bool,

    _source: Box<dyn AudioSource>,
    capture: CaptureHandle,
    capture_mode: CaptureMode,
    analyzer: Analyzer,
    /// Samples held back to implement the user-set audio delay (interleaved).
    delay_line: VecDeque<f32>,
    /// (L, R) points waiting to be drawn, newest last.
    pending_points: Vec<[f32; 2]>,
    last_point: [f32; 2],
    sample_rate: u32,

    settings: Settings,
    last_frame: Instant,
    fps: f32,
    points_last_frame: usize,
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
        let egui_renderer = egui_wgpu::Renderer::new(&gpu.device, gpu.config.format, egui_wgpu::RendererOptions::default());

        let settings = Settings::default();
        let source = audioviz_audio::default_source();
        let capture = source.start(CaptureTarget::Process(settings.target_process.clone()), CHUNK_FRAMES)?;
        let sample_rate = 48_000;
        let analyzer = Analyzer::new(AnalyzerConfig { sample_rate, ..Default::default() });

        Ok(Self {
            gpu,
            scope,
            shaders,
            egui_ctx,
            egui_state,
            egui_renderer,
            show_ui: true,
            _source: source,
            capture,
            capture_mode: CaptureMode::Starting,
            analyzer,
            delay_line: VecDeque::with_capacity(48_000),
            pending_points: Vec::with_capacity(MAX_POINTS),
            last_point: [0.0, 0.0],
            sample_rate,
            settings,
            last_frame: Instant::now(),
            fps: 0.0,
            points_last_frame: 0,
            auto_gain_value: 1.0,
        })
    }

    /// Drain captured audio: apply the delay line, feed the analyzer, queue scope points.
    fn ingest_audio(&mut self) {
        while let Ok(mode) = self.capture.mode.try_recv() {
            self.capture_mode = mode;
        }
        let delay_samples = ((self.settings.audio_delay_ms / 1000.0) * self.sample_rate as f32) as usize * 2;
        while let Ok(chunk) = self.capture.chunks.try_recv() {
            if chunk.sample_rate != self.sample_rate {
                self.sample_rate = chunk.sample_rate;
                self.analyzer = Analyzer::new(AnalyzerConfig { sample_rate: self.sample_rate, ..Default::default() });
            }
            self.delay_line.extend(chunk.samples.iter().copied());
        }
        // Release everything older than the delay.
        let releasable = self.delay_line.len().saturating_sub(delay_samples) & !1usize;
        if releasable == 0 {
            return;
        }
        let released: Vec<f32> = self.delay_line.drain(..releasable).collect();
        self.analyzer.push(&released);
        for pair in released.chunks_exact(2) {
            self.pending_points.push([pair[0], pair[1]]);
        }
        // Never let a stall pile up more than one buffer's worth of history.
        if self.pending_points.len() > MAX_POINTS - 1 {
            let drop = self.pending_points.len() - (MAX_POINTS - 1);
            self.pending_points.drain(..drop);
        }
    }

    fn render(&mut self) {
        self.ingest_audio();
        if self.shaders.poll() {
            self.scope.reload_shaders(&self.gpu.device, &self.shaders);
        }

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(1e-4, 0.25);
        self.last_frame = now;
        self.fps = self.fps * 0.95 + (1.0 / dt) * 0.05;

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
        let features = self.analyzer.frame().clone();
        // Auto gain: aim the decaying peak at ~0.9 of the square.
        let target = if features.peak > 0.02 { (0.9 / features.peak).clamp(0.3, 6.0) } else { self.auto_gain_value };
        self.auto_gain_value += (target - self.auto_gain_value) * (1.0 - (-dt / 1.5).exp());
        let gain = if self.settings.auto_gain { self.settings.gain * self.auto_gain_value } else { self.settings.gain };

        let params = Params {
            resolution: [self.gpu.config.width as f32, self.gpu.config.height as f32],
            gain,
            rotation: self.settings.rotation_deg.to_radians(),
            beam_width: self.settings.beam_width,
            // Per-frame multiplier from the per-second persistence fraction.
            decay: self.settings.persistence.max(1e-6).powf(dt),
            beam_intensity: self.settings.beam_intensity * 40.0,
            exposure: self.settings.exposure,
            tint: [self.settings.tint[0], self.settings.tint[1], self.settings.tint[2], 1.0],
            point_count: 0,
            _pad: [0; 3],
        };
        // Prepend last frame's final point so the trace is continuous.
        let mut points = Vec::with_capacity(self.pending_points.len() + 1);
        points.push(self.last_point);
        points.extend_from_slice(&self.pending_points);
        if let Some(p) = self.pending_points.last() {
            self.last_point = *p;
        }
        self.points_last_frame = self.pending_points.len();
        self.pending_points.clear();
        self.scope.render(&self.gpu.queue, &mut encoder, &view, params, &points);

        // --- egui ---
        if self.show_ui {
            let raw_input = self.egui_state.take_egui_input(&self.gpu.window);
            let stats = Stats {
                fps: self.fps,
                points_per_frame: self.points_last_frame,
                capture: self.capture_mode.clone(),
                shader_error: self.scope.last_shader_error.clone(),
                sample_rate: self.sample_rate,
            };
            let settings = &mut self.settings;
            let output = self.egui_ctx.run(raw_input, |ctx| crate::ui::draw(ctx, settings, &features, &stats));
            self.egui_state.handle_platform_output(&self.gpu.window, output.platform_output);
            let primitives = self.egui_ctx.tessellate(output.shapes, output.pixels_per_point);
            let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [self.gpu.config.width, self.gpu.config.height], pixels_per_point: output.pixels_per_point };
            for (id, delta) in &output.textures_delta.set {
                self.egui_renderer.update_texture(&self.gpu.device, &self.gpu.queue, *id, delta);
            }
            let extra = self.egui_renderer.update_buffers(&self.gpu.device, &self.gpu.queue, &mut encoder, &primitives, &screen);
            {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
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
            for id in &output.textures_delta.free {
                self.egui_renderer.free_texture(id);
            }
            self.gpu.queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        } else {
            self.gpu.queue.submit(std::iter::once(encoder.finish()));
        }

        self.gpu.window.pre_present_notify();
        self.gpu.queue.present(frame);
    }

    fn toggle_fullscreen(&self) {
        let w = &self.gpu.window;
        if w.fullscreen().is_some() {
            w.set_fullscreen(None);
        } else {
            w.set_fullscreen(Some(Fullscreen::Borderless(None)));
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

        if st.show_ui {
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
                KeyCode::Tab => st.show_ui = !st.show_ui,
                KeyCode::F5 => {
                    st.shaders = ShaderSet::load();
                    st.scope.reload_shaders(&st.gpu.device, &st.shaders);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(st) = &self.state {
            st.gpu.window.request_redraw();
        }
    }
}
