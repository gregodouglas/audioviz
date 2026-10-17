//! Scene 3: gravitationally lensed star field with geodesic grid lines.
//! The Swiss type overlay (track metadata) is drawn by `overlay.rs` on top.

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, Scene, fullscreen_pass, hdr_texture, HDR_FORMAT};
use crate::shaders::ShaderSet;
use crate::settings::{Layer, VisualKind};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct StarParams {
    pub resolution: [f32; 2],
    pub time: f32,
    pub einstein: f32,
    pub tint: [f32; 4],
    pub bands: [[f32; 4]; 4],
    pub onset: f32,
    pub beat: f32,
    pub grid: f32,
    pub density: f32,
    pub twinkle: f32,
    pub drift: f32,
    pub _p0: f32,
    pub _p1: f32,
}

pub struct Starfield {
    out: wgpu::TextureView,
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    einstein: f32,
}

impl Starfield {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let sampler = scene::linear_sampler(device);
        let params_buf = scene::uniform_buffer(device, "starfield params", std::mem::size_of::<StarParams>() as u64);
        let layout = scene::tex_layout(device, "starfield", 0);
        let out = hdr_texture(device, "starfield out", width, height);
        // The shader needs no input texture; bind the output's sibling to satisfy the layout.
        let dummy = hdr_texture(device, "starfield dummy", 1, 1);
        let group = scene::tex_group(device, &layout, &[&dummy], &sampler, &params_buf);
        let pipeline = build(device, shaders, &layout)?;
        Ok(Self { out, sampler, params_buf, layout, group, pipeline, einstein: 0.2 })
    }
}

impl Scene for Starfield {
    fn kind(&self) -> VisualKind {
        VisualKind::Starfield
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.out = hdr_texture(device, "starfield out", width, height);
        let _ = &self.sampler;
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipeline = build(device, shaders, &self.layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let s = &layer.starfield;
        let f = ctx.features;
        // Lens mass follows the slow bass; slewed so the ring breathes rather than jitters.
        let target = (0.10 + 0.22 * (f.bass_att / 2.0).clamp(0.0, 1.0)) * s.mass;
        self.einstein += (target - self.einstein) * (1.0 - (-ctx.dt / 0.4).exp());
        let mut bands = [[0.0f32; 4]; 4];
        for i in 0..16 {
            // Fold 32 bands into 16 by pairs.
            let v = f.bands.get(i * 2).copied().unwrap_or(0.0).max(f.bands.get(i * 2 + 1).copied().unwrap_or(0.0));
            bands[i / 4][i % 4] = v;
        }
        let params = StarParams {
            resolution: [ctx.width as f32, ctx.height as f32],
            time: ctx.time,
            einstein: self.einstein,
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            bands,
            onset: f.onset_pulse[3].min(1.0),
            beat: f.beat_pulse * f.beat_confidence,
            grid: s.grid,
            density: s.density,
            twinkle: s.twinkle,
            drift: s.drift,
            _p0: 0.0,
            _p1: 0.0,
        };
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        fullscreen_pass(encoder, "starfield", &self.out, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipeline, &self.group);
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.out
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        let s = &mut layer.starfield;
        use crate::widgets::{ghost_button, hint, slider_row};
        slider_row(ui, "Lens mass", &mut s.mass, 0.2..=3.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Star density", &mut s.density, 0.1..=2.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Grid lines", &mut s.grid, 0.0..=3.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Onset twinkle", &mut s.twinkle, 0.0..=2.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Drift", &mut s.drift, 0.0..=4.0, |v| format!("{v:.2}"), false);
        hint(ui, "Lens mass follows the bass; the spectrum wobbles the ring; onsets twinkle.");
        ui.add_space(6.0);
        if ghost_button(ui, "Reset starfield", false).clicked() {
            *s = Default::default();
        }
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "starfield.wgsl");
        scene::pipeline(device, "starfield", layout, &m, HDR_FORMAT, None)
    })
}
