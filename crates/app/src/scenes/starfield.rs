//! Scene 3: gravitationally lensed star field with geodesic grid lines.
//! The Swiss type overlay (track metadata) is drawn by `overlay.rs` on top.

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, Scene, fullscreen_pass, hdr_texture, HDR_FORMAT};
use crate::shaders::ShaderSet;
use crate::ui::Settings;

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
    fn name(&self) -> &'static str {
        "Lensed starfield"
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.out = hdr_texture(device, "starfield out", width, height);
        let _ = &self.sampler;
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipeline = build(device, shaders, &self.layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx) {
        let s = ctx.settings;
        let f = ctx.features;
        // Lens mass follows the slow bass; slewed so the ring breathes rather than jitters.
        let target = (0.10 + 0.22 * (f.bass_att / 2.0).clamp(0.0, 1.0)) * s.star_mass;
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
            tint: [s.tint[0], s.tint[1], s.tint[2], 1.0],
            bands,
            onset: f.onset_pulse[3].min(1.0),
            beat: f.beat_pulse * f.beat_confidence,
            grid: s.star_grid,
            density: s.star_density,
            twinkle: s.star_twinkle,
            drift: s.star_drift,
            _p0: 0.0,
            _p1: 0.0,
        };
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        fullscreen_pass(encoder, "starfield", &self.out, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipeline, &self.group);
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.out
    }

    fn ui(&mut self, ui: &mut egui::Ui, s: &mut Settings) {
        ui.add(egui::Slider::new(&mut s.star_mass, 0.2..=3.0).text("lens mass"));
        ui.add(egui::Slider::new(&mut s.star_density, 0.1..=2.0).text("star density"));
        ui.add(egui::Slider::new(&mut s.star_grid, 0.0..=3.0).text("grid lines"));
        ui.add(egui::Slider::new(&mut s.star_twinkle, 0.0..=2.0).text("onset twinkle"));
        ui.add(egui::Slider::new(&mut s.star_drift, 0.0..=4.0).text("drift"));
        ui.checkbox(&mut s.type_overlay, "track title overlay (Swiss grid)");
        ui.add(egui::Slider::new(&mut s.type_scale, 0.5..=2.0).text("type scale"));
        ui.small("Lens mass follows the bass; the spectrum wobbles the ring; onsets twinkle.");
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "starfield.wgsl");
        scene::pipeline(device, "starfield", layout, &m, HDR_FORMAT, None)
    })
}
