//! Ripples: a plane in perspective displaced by flowing value noise, printed
//! with contour lines of its height (or dots, or a grid), darker with
//! distance. Beats drop rings into the surface at seeded places; the bass
//! swells the hills. The plane is a grid mesh made in the vertex shader and
//! drawn far row to near row, opaque, so near hills hide what is behind them
//! without a depth buffer.
//!
//! After the Ripples in Uberviz (BBNG, Lantern) by Felix Turner / Airtight
//! Interactive, https://uberviz.io. No code used; written fresh from a
//! description of the effect (see docs/INSPIRATION.md).

use bytemuck::{Pod, Zeroable};

use crate::rng::Rng;
use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, begin_pass};
use crate::settings::{Layer, RipplesParams, RipplesPrint};
use crate::shaders::ShaderSet;

/// Grid cells per side of the mesh.
pub const GRID: u32 = 160;
const RINGS: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    resolution: [f32; 2],
    time: f32,
    print: u32,
    height: f32,
    scale: f32,
    flow: f32,
    density: f32,
    width: f32,
    tilt: f32,
    fade: f32,
    grid: u32,
    tint: [f32; 4],
    palette_mix: f32,
    palette_shift: f32,
    _pad: [f32; 2],
    /// (x, z, birth time, strength) per ring.
    rings: [[f32; 4]; RINGS],
    palette: crate::color::Table,
}

pub struct Ripples {
    hdr: wgpu::TextureView,
    uniform: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    flow: f32,
    swell: f32,
    rings: [[f32; 4]; RINGS],
    ring_i: usize,
    drops: u64,
    last_beat: f32,
}

impl Ripples {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let uniform = scene::uniform_buffer(device, "ripples params", std::mem::size_of::<Uniform>() as u64);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ripples layout"),
            entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("ripples group"), layout: &layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }] });
        let pipeline = build(device, shaders, &layout)?;
        Ok(Self { hdr: scene::hdr_texture(device, "ripples hdr", width, height), uniform, layout, group, pipeline, flow: 0.0, swell: 1.0, rings: [[0.0, 0.0, -100.0, 0.0]; RINGS], ring_i: 0, drops: 0, last_beat: 0.0 })
    }
}

/// Registry id: the layer's `visual`, its parameter key, the presets folder.
pub const ID: &str = "ripples";

fn make(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
    Ok(Box::new(Ripples::new(device, width, height, shaders)?))
}

pub fn def() -> crate::visuals::VisualDef {
    crate::visuals::VisualDef {
        id: ID,
        name: "Ripples",
        short: "Ripples",
        summary: "A flowing surface seen from low over it, printed as contours, dots or a grid; beats drop rings in it.",
        source: crate::visuals::Source::Native { make, params: crate::visuals::typed::<RipplesParams> },
        band_waveform: false,
    }
}

impl Scene for Ripples {
    fn id(&self) -> &'static str {
        ID
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.hdr = scene::hdr_texture(device, "ripples hdr", width, height);
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipeline = build(device, shaders, &self.layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let p = &layer.get::<RipplesParams>(ID);
        let f = ctx.features;
        let dt = ctx.dt;
        let energy = ((f.bass_att + f.mid_att + f.treb_att) / 3.0).clamp(0.0, 2.5);
        self.flow += dt * p.speed * (0.5 + 0.5 * energy);
        // The bass swells the hills, slewed so it rolls rather than jumps.
        let want = 1.0 + p.react * 0.6 * (f.bass_att - 1.0).clamp(-0.6, 1.5);
        self.swell += (want - self.swell) * (1.0 - (-dt / 0.2).exp());
        // A beat drops a ring at a seeded place in the near half of the plane.
        let beat = f.beat_pulse * f.beat_confidence;
        if beat > self.last_beat + 0.4 && p.rings > 0.0 {
            self.drops += 1;
            let mut rng = Rng::seeded(&[ctx.art_id, self.drops]);
            let x = (rng.next() * 2.0 - 1.0) * 1.2;
            let z = 0.6 + rng.next() * 1.8;
            self.rings[self.ring_i] = [x, z, ctx.time, p.rings * (0.6 + 0.4 * beat.min(1.0))];
            self.ring_i = (self.ring_i + 1) % RINGS;
        }
        self.last_beat = beat;
        let print = match p.print {
            RipplesPrint::Contours => 0,
            RipplesPrint::Dots => 1,
            RipplesPrint::Grid => 2,
        };
        let u = Uniform {
            resolution: [ctx.width as f32, ctx.height as f32],
            time: ctx.time,
            print,
            height: p.height * self.swell,
            scale: p.scale.max(0.05),
            flow: self.flow,
            density: p.density.max(0.5),
            width: p.width.max(0.3),
            tilt: p.tilt.clamp(0.0, 1.0),
            fade: p.fade,
            grid: GRID,
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            _pad: [0.0; 2],
            rings: self.rings,
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        let mut pass = begin_pass(encoder, "ripples", &self.hdr, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        // One instance per row of cells, far row first.
        pass.draw(0..GRID * 6, 0..GRID);
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.hdr
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        use crate::widgets::{ghost_button, group, hint, segmented, slider_def, small_caps};
        let mut params = layer.get::<RipplesParams>(ID);
        let p = &mut params;
        let d = RipplesParams::default();
        group(ui, "ripples.surface", "Surface", true, 6, |ui| {
            slider_def(ui, "Height", &mut p.height, Some(d.height), 0.0..=1.5, |v| format!("{v:.2}"), false);
            slider_def(ui, "Scale", &mut p.scale, Some(d.scale), 0.2..=6.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Flow", &mut p.speed, Some(d.speed), 0.0..=1.5, |v| format!("{v:.2}"), false);
            slider_def(ui, "Rings on beats", &mut p.rings, Some(d.rings), 0.0..=3.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Camera tilt", &mut p.tilt, Some(d.tilt), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Reacts", &mut p.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
        });
        group(ui, "ripples.print", "Print", true, 4, |ui| {
            small_caps(ui, "Printed as");
            let names: Vec<&str> = RipplesPrint::ALL.iter().map(|m| m.name()).collect();
            if let Some(i) = segmented(ui, &names, RipplesPrint::ALL.iter().position(|m| *m == p.print).unwrap_or(0)) {
                p.print = RipplesPrint::ALL[i];
            }
            slider_def(ui, "Density", &mut p.density, Some(d.density), 1.0..=40.0, |v| format!("{v:.1}"), false);
            slider_def(ui, "Line width", &mut p.width, Some(d.width), 0.5..=5.0, |v| format!("{v:.1} px"), false);
            slider_def(ui, "Distance fade", &mut p.fade, Some(d.fade), 0.0..=1.0, |v| format!("{v:.2}"), false);
            hint(ui, "A flowing surface seen from low over it: contours draw its height like a map, dots and the grid ride on it. Beats drop rings into it; the bass swells the hills. With a palette, the colour follows the height. After the Ripples in Felix Turner's Uberviz.");
        });
        ui.add_space(6.0);
        if ghost_button(ui, "Reset ripples", false).clicked() {
            *p = Default::default();
        }
        layer.put(ID, &params);
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "ripples.wgsl");
        scene::pipeline(device, "ripples", layout, &m, HDR_FORMAT, None)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_layout_matches_wgsl() {
        assert_eq!(std::mem::offset_of!(Uniform, tint), 48);
        assert_eq!(std::mem::offset_of!(Uniform, rings), 80);
        assert_eq!(std::mem::offset_of!(Uniform, palette), 80 + 64);
    }
}
