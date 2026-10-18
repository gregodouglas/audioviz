//! Wheel: a disc of spectrum wedges, brightness by band level, colours
//! running round the palette, wedges carrying on as rays past the rim. It
//! turns slowly, jumps on some beats, and can step its band mapping round
//! on every beat so the pattern walks. A backdrop layer.
//!
//! Inspired by the Wheel in Felix Turner's Uberviz (https://uberviz.io, no
//! published licence): the idea only, written fresh. See `docs/INSPIRATION.md`.

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, fullscreen_pass};
use crate::shaders::ShaderSet;
use crate::settings::{Layer, VisualKind};

pub const MAX_WEDGES: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    resolution: [f32; 2],
    count: f32,
    spin: f32,
    radius: f32,
    soft: f32,
    rays: f32,
    curve: f32,
    range: f32,
    hole: f32,
    palette_mix: f32,
    palette_shift: f32,
    tint: [f32; 4],
    levels: [[f32; 4]; 16],
    palette: crate::color::Table,
}

pub struct Wheel {
    hdr: wgpu::TextureView,
    uniform: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    spin: f32,
    /// Band-to-wedge offset, stepped on beats.
    step: usize,
    last_beat: f32,
    levels: Vec<f32>,
    seed: u32,
}

impl Wheel {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let uniform = scene::uniform_buffer(device, "wheel params", std::mem::size_of::<Uniform>() as u64);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wheel layout"),
            entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("wheel group"), layout: &layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }] });
        let pipeline = build(device, shaders, &layout)?;
        Ok(Self { hdr: scene::hdr_texture(device, "wheel hdr", width, height), uniform, layout, group, pipeline, spin: 0.0, step: 0, last_beat: 0.0, levels: vec![0.0; MAX_WEDGES], seed: 0x9E37_79B9 })
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

impl Scene for Wheel {
    fn kind(&self) -> VisualKind {
        VisualKind::Wheel
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.hdr = scene::hdr_texture(device, "wheel hdr", width, height);
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipeline = build(device, shaders, &self.layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let p = &layer.wheel;
        let f = ctx.features;
        let n = (p.count as usize).clamp(4, MAX_WEDGES);
        let dt = ctx.dt;
        // Beats: a chance of a jump, and the band mapping walks one wedge.
        let beat = f.beat_pulse * f.beat_confidence;
        if beat > self.last_beat + 0.4 && p.react > 0.0 {
            if self.rand() < p.jump {
                self.spin = self.rand();
            }
            if p.walk {
                self.step = (self.step + 1) % n;
            }
        }
        self.last_beat = beat;
        self.spin = (self.spin - dt * p.speed * 0.05).rem_euclid(1.0);
        // Wedge levels from the bands (resampled round the wheel), fast
        // attack and a slow release so the wheel breathes instead of flickering.
        let bands = &f.bands;
        let nb = bands.len();
        let attack = 1.0 - (-dt / 0.03).exp();
        let release = 1.0 - (-dt / 0.25).exp();
        for w in 0..n {
            let src = (w + self.step) % n;
            let x = src as f32 / n as f32 * nb.max(1) as f32;
            let (k, fr) = (x.floor() as usize, x.fract());
            let v = if nb == 0 { 0.0 } else { let a = bands[k.min(nb - 1)]; let b = bands[(k + 1).min(nb - 1)]; (a + (b - a) * fr).max(0.0) * p.gain };
            let cur = self.levels[w];
            self.levels[w] = cur + (v - cur) * if v > cur { attack } else { release };
        }
        let mut levels = [[0.0f32; 4]; 16];
        for (w, v) in self.levels.iter().enumerate().take(n) {
            levels[w / 4][w % 4] = *v;
        }
        let u = Uniform {
            resolution: [ctx.width as f32, ctx.height as f32],
            count: n as f32,
            spin: self.spin,
            radius: p.radius,
            soft: p.soft,
            rays: p.rays,
            curve: p.curve,
            range: p.range,
            hole: p.hole,
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            levels,
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        fullscreen_pass(encoder, "wheel", &self.hdr, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipeline, &self.group);
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.hdr
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        use crate::widgets::{ghost_button, group, hint, slider_def, toggle_row};
        let p = &mut layer.wheel;
        let d = crate::settings::WheelParams::default();
        group(ui, "wheel.disc", "Disc", true, 5, |ui| {
            slider_def(ui, "Wedges", &mut p.count, Some(d.count), 4..=MAX_WEDGES as u32, |v| format!("{v:.0}"), false);
            slider_def(ui, "Radius", &mut p.radius, Some(d.radius), 0.1..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Rays", &mut p.rays, Some(d.rays), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Centre hole", &mut p.hole, Some(d.hole), 0.0..=0.95, |v| format!("{v:.2}"), false);
            slider_def(ui, "Edge softness", &mut p.soft, Some(d.soft), 0.0..=0.5, |v| format!("{v:.2}"), false);
        });
        group(ui, "wheel.music", "Music", true, 6, |ui| {
            slider_def(ui, "Gain", &mut p.gain, Some(d.gain), 0.2..=4.0, |v| format!("{v:.2}"), true);
            slider_def(ui, "Curve", &mut p.curve, Some(d.curve), 0.3..=4.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Palette span", &mut p.range, Some(d.range), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Turn", &mut p.speed, Some(d.speed), -4.0..=4.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Jump on beats", &mut p.jump, Some(d.jump), 0.0..=1.0, |v| format!("{:.0}%", v * 100.0), false);
            toggle_row(ui, "Bands walk round on beats", &mut p.walk);
            slider_def(ui, "Reacts", &mut p.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Each wedge is a band of the spectrum; quiet bands go dark. A backdrop for other layers. After the Wheel in Uberviz.");
        });
        ui.add_space(6.0);
        if ghost_button(ui, "Reset wheel", false).clicked() {
            *p = Default::default();
        }
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "wheel.wgsl");
        scene::pipeline(device, "wheel", layout, &m, HDR_FORMAT, None)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_layout_matches_wgsl() {
        assert_eq!(std::mem::offset_of!(Uniform, tint), 48);
        assert_eq!(std::mem::offset_of!(Uniform, levels), 64);
        assert_eq!(std::mem::offset_of!(Uniform, palette), 64 + 256);
    }
}
