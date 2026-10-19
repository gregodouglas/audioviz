//! Eclipse: a black disc ringed by glowing dots, one per spectrum band, with
//! a corona at the rim that flares on the bass. Each dot's brightness and
//! size follow its band; loud dots push outward a little. Colours run round
//! the palette. One fragment pass.
//!
//! After the Eclipse in Uberviz by Felix Turner / Airtight Interactive,
//! https://uberviz.io. No code used; written fresh from a description of the
//! effect (see docs/INSPIRATION.md).

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, fullscreen_pass};
use crate::settings::{EclipseParams, Layer};
use crate::shaders::ShaderSet;

pub const MAX_DOTS: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    resolution: [f32; 2],
    count: f32,
    spin: f32,
    radius: f32,
    disc: f32,
    size: f32,
    glow: f32,
    corona: f32,
    flare: f32,
    curve: f32,
    range: f32,
    react: f32,
    mirror: f32,
    palette_mix: f32,
    palette_shift: f32,
    tint: [f32; 4],
    levels: [[f32; 4]; 16],
    palette: crate::color::Table,
}

pub struct Eclipse {
    hdr: wgpu::TextureView,
    uniform: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    spin: f32,
    flare: f32,
    levels: Vec<f32>,
}

impl Eclipse {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let uniform = scene::uniform_buffer(device, "eclipse params", std::mem::size_of::<Uniform>() as u64);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("eclipse layout"),
            entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("eclipse group"), layout: &layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }] });
        let pipeline = build(device, shaders, &layout)?;
        Ok(Self { hdr: scene::hdr_texture(device, "eclipse hdr", width, height), uniform, layout, group, pipeline, spin: 0.0, flare: 0.0, levels: vec![0.0; MAX_DOTS] })
    }
}

/// Registry id: the layer's `visual`, its parameter key, the presets folder.
pub const ID: &str = "eclipse";

fn make(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
    Ok(Box::new(Eclipse::new(device, width, height, shaders)?))
}

pub fn def() -> crate::visuals::VisualDef {
    crate::visuals::VisualDef {
        id: ID,
        name: "Eclipse",
        short: "Eclipse",
        summary: "A black disc ringed by one glowing dot per band, with a corona that flares on the bass.",
        make,
        params: crate::visuals::typed::<EclipseParams>,
        band_waveform: false,
    }
}

impl Scene for Eclipse {
    fn id(&self) -> &'static str {
        ID
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.hdr = scene::hdr_texture(device, "eclipse hdr", width, height);
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipeline = build(device, shaders, &self.layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let p = &layer.get::<EclipseParams>(ID);
        let f = ctx.features;
        let n = (p.count as usize).clamp(4, MAX_DOTS);
        let dt = ctx.dt;
        let energy = ((f.bass_att + f.mid_att + f.treb_att) / 3.0).clamp(0.0, 2.5);
        self.spin = (self.spin + dt * p.spin * (0.5 + 0.5 * energy)).rem_euclid(1.0);
        // Corona: the bass above its average, plus a kick on low onsets.
        let want = ((f.bass_att - 1.0).max(0.0) * 0.8 + f.onset_pulse[0].min(1.0) * 0.6) * p.react;
        let k = 1.0 - (-dt / if want > self.flare { 0.04 } else { 0.35 }).exp();
        self.flare += (want - self.flare) * k;
        // Dot levels from the bands: mirrored, the bands run up both sides
        // from the bottom, so each band has two dots.
        let bands = &f.bands;
        let nb = bands.len();
        let attack = 1.0 - (-dt / 0.03).exp();
        let release = 1.0 - (-dt / 0.2).exp();
        let span = if p.mirror { n.div_ceil(2) } else { n };
        for d in 0..n {
            let src = if p.mirror { if d < span { d } else { n - 1 - d } } else { d };
            let x = src as f32 / span.max(1) as f32 * nb.max(1) as f32;
            let (k, fr) = (x.floor() as usize, x.fract());
            let v = if nb == 0 {
                0.0
            } else {
                let a = bands[k.min(nb - 1)];
                let b = bands[(k + 1).min(nb - 1)];
                (a + (b - a) * fr).max(0.0) * p.gain
            };
            let cur = self.levels[d];
            self.levels[d] = cur + (v - cur) * if v > cur { attack } else { release };
        }
        let mut levels = [[0.0f32; 4]; 16];
        for (d, v) in self.levels.iter().enumerate().take(n) {
            levels[d / 4][d % 4] = *v;
        }
        let u = Uniform {
            resolution: [ctx.width as f32, ctx.height as f32],
            count: n as f32,
            spin: self.spin * std::f32::consts::TAU,
            radius: p.radius,
            disc: p.disc,
            size: p.size,
            glow: p.glow,
            corona: p.corona,
            flare: self.flare,
            curve: p.curve,
            range: p.range,
            react: p.react,
            mirror: p.mirror as u32 as f32,
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            levels,
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        fullscreen_pass(encoder, "eclipse", &self.hdr, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipeline, &self.group);
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.hdr
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        use crate::widgets::{ghost_button, group, hint, slider_def, toggle_row};
        let mut params = layer.get::<EclipseParams>(ID);
        let p = &mut params;
        let d = EclipseParams::default();
        group(ui, "eclipse.ring", "Ring", true, 6, |ui| {
            slider_def(ui, "Dots", &mut p.count, Some(d.count), 4..=MAX_DOTS as u32, |v| format!("{v:.0}"), false);
            slider_def(ui, "Radius", &mut p.radius, Some(d.radius), 0.1..=1.2, |v| format!("{v:.2}"), false);
            slider_def(ui, "Disc", &mut p.disc, Some(d.disc), 0.0..=1.2, |v| format!("{v:.2}"), false);
            slider_def(ui, "Dot size", &mut p.size, Some(d.size), 0.005..=0.15, |v| format!("{v:.3}"), false);
            slider_def(ui, "Spin", &mut p.spin, Some(d.spin), -1.0..=1.0, |v| format!("{v:.2}"), false);
            toggle_row(ui, "Mirror the spectrum", &mut p.mirror);
        });
        group(ui, "eclipse.light", "Light", true, 6, |ui| {
            slider_def(ui, "Glow", &mut p.glow, Some(d.glow), 0.0..=4.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Corona", &mut p.corona, Some(d.corona), 0.0..=3.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Gain", &mut p.gain, Some(d.gain), 0.1..=4.0, |v| format!("{v:.2}"), true);
            slider_def(ui, "Curve", &mut p.curve, Some(d.curve), 0.5..=4.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Palette span", &mut p.range, Some(d.range), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Reacts", &mut p.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "One dot per band round a black disc: lows at the bottom, highs at the top when mirrored. Loud dots swell and push out; the corona flares on the bass. After the Eclipse in Felix Turner's Uberviz.");
        });
        ui.add_space(6.0);
        if ghost_button(ui, "Reset eclipse", false).clicked() {
            *p = Default::default();
        }
        layer.put(ID, &params);
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "eclipse.wgsl");
        scene::pipeline(device, "eclipse", layout, &m, HDR_FORMAT, None)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_layout_matches_wgsl() {
        assert_eq!(std::mem::offset_of!(Uniform, tint), 64);
        assert_eq!(std::mem::offset_of!(Uniform, levels), 80);
        assert_eq!(std::mem::offset_of!(Uniform, palette), 80 + 256);
    }
}
