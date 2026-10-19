//! Spiral: one long polyline through points stepped round by an angle that
//! drifts slowly, so the figure morphs between spiral arms, rosettes and star
//! meshes as the step passes near simple fractions of a turn. Points sit on
//! an even-density spiral (radius by the square root of the index) or on a
//! rose curve joined in step order (a Maurer rose). The waveform pushes each
//! point in and out; beats kick it, energy speeds the drift. Colours run
//! along the line through the palette. Drawn additively, segment by segment,
//! in the vertex shader.
//!
//! After the spiral family in audible-visuals by Sonia Boller,
//! https://github.com/soniaboller/audible-visuals, Apache License 2.0. The
//! idea only; no code ported (see docs/INSPIRATION.md).

use std::collections::VecDeque;

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, begin_pass};
use crate::settings::{Layer, SpiralParams, SpiralShape};
use crate::shaders::ShaderSet;

pub const MAX_POINTS: u32 = 4096;
/// Waveform samples the line reads along its length.
const WAVE: usize = 1024;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    resolution: [f32; 2],
    points: u32,
    shape: u32,
    wave: u32,
    petals: f32,
    step: f32,
    spin: f32,
    radius: f32,
    amp: f32,
    width: f32,
    intensity: f32,
    tint: [f32; 4],
    palette_mix: f32,
    palette_shift: f32,
    _pad: [f32; 2],
    palette: crate::color::Table,
}

pub struct Spiral {
    hdr: wgpu::TextureView,
    uniform: wgpu::Buffer,
    wave_buf: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    /// Accumulated drift of the step, degrees.
    drift: f32,
    angle: f32,
    kick: f32,
    last_beat: f32,
    ring: VecDeque<f32>,
    wave: Vec<f32>,
}

impl Spiral {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let uniform = scene::uniform_buffer(device, "spiral params", std::mem::size_of::<Uniform>() as u64);
        let wave_buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("spiral wave"), size: (WAVE * 4) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let entry = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("spiral layout"),
            entries: &[entry(0, wgpu::BufferBindingType::Uniform), entry(1, wgpu::BufferBindingType::Storage { read_only: true })],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("spiral group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wave_buf.as_entire_binding() }],
        });
        let pipeline = build(device, shaders, &layout)?;
        Ok(Self { hdr: scene::hdr_texture(device, "spiral hdr", width, height), uniform, wave_buf, layout, group, pipeline, drift: 0.0, angle: 0.0, kick: 0.0, last_beat: 0.0, ring: VecDeque::with_capacity(WAVE * 2), wave: vec![0.0; WAVE] })
    }
}

/// Registry id: the layer's `visual`, its parameter key, the presets folder.
pub const ID: &str = "spiral";

fn make(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
    Ok(Box::new(Spiral::new(device, width, height, shaders)?))
}

pub fn def() -> crate::visuals::VisualDef {
    crate::visuals::VisualDef {
        id: ID,
        name: "Spiral",
        short: "Spiral",
        summary: "One line through points stepped by a slowly drifting angle; morphs between arms, rosettes and stars.",
        make,
        params: crate::visuals::typed::<SpiralParams>,
        band_waveform: false,
    }
}

impl Scene for Spiral {
    fn id(&self) -> &'static str {
        ID
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.hdr = scene::hdr_texture(device, "spiral hdr", width, height);
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipeline = build(device, shaders, &self.layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let p = &layer.get::<SpiralParams>(ID);
        let f = ctx.features;
        let dt = ctx.dt;
        for s in ctx.points {
            self.ring.push_back((s[0] + s[1]) * 0.5);
        }
        while self.ring.len() > WAVE {
            self.ring.pop_front();
        }
        let gain = p.gain * ctx.auto_gain;
        let have = self.ring.len();
        for (i, v) in self.wave.iter_mut().enumerate() {
            *v = if i < have { self.ring[have - 1 - i] * gain } else { 0.0 };
        }
        queue.write_buffer(&self.wave_buf, 0, bytemuck::cast_slice(&self.wave));
        let energy = ((f.bass_att + f.mid_att + f.treb_att) / 3.0).clamp(0.0, 2.5);
        self.drift = (self.drift + dt * p.drift * (1.0 + p.react * (energy - 1.0)).max(0.1)).rem_euclid(360.0);
        self.angle = (self.angle + dt * p.spin * (0.5 + 0.5 * energy)).rem_euclid(std::f32::consts::TAU);
        let beat = f.beat_pulse * f.beat_confidence;
        if beat > self.last_beat + 0.4 {
            self.kick = 1.0;
        }
        self.last_beat = beat;
        self.kick *= (-dt / 0.25).exp();
        let n = p.points.clamp(16, MAX_POINTS);
        let u = Uniform {
            resolution: [ctx.width as f32, ctx.height as f32],
            points: n,
            shape: (p.shape == SpiralShape::Rose) as u32,
            wave: WAVE as u32,
            petals: p.petals.clamp(1, 24) as f32,
            step: (p.step + self.drift).to_radians(),
            spin: self.angle,
            radius: p.radius,
            amp: p.amp * (1.0 + p.react * self.kick),
            width: p.width,
            intensity: p.intensity,
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            _pad: [0.0; 2],
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        let mut pass = begin_pass(encoder, "spiral", &self.hdr, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..6, 0..n - 1);
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.hdr
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        use crate::widgets::{ghost_button, group, hint, segmented, slider_def, small_caps};
        let mut params = layer.get::<SpiralParams>(ID);
        let p = &mut params;
        let d = SpiralParams::default();
        group(ui, "spiral.figure", "Figure", true, 7, |ui| {
            small_caps(ui, "Shape");
            let names: Vec<&str> = SpiralShape::ALL.iter().map(|s| s.name()).collect();
            if let Some(i) = segmented(ui, &names, SpiralShape::ALL.iter().position(|s| *s == p.shape).unwrap_or(0)) {
                p.shape = SpiralShape::ALL[i];
            }
            slider_def(ui, "Points", &mut p.points, Some(d.points), 64..=MAX_POINTS, |v| format!("{v:.0}"), true);
            slider_def(ui, "Angle step", &mut p.step, Some(d.step), 0.0..=360.0, |v| format!("{v:.2}°"), false);
            slider_def(ui, "Drift", &mut p.drift, Some(d.drift), -2.0..=2.0, |v| format!("{v:.2}°/s"), false);
            if p.shape == SpiralShape::Rose {
                slider_def(ui, "Petals", &mut p.petals, Some(d.petals), 1..=12, |v| format!("{v:.0}"), false);
            }
            slider_def(ui, "Size", &mut p.radius, Some(d.radius), 0.1..=1.5, |v| format!("{v:.2}"), false);
            slider_def(ui, "Spin", &mut p.spin, Some(d.spin), -1.0..=1.0, |v| format!("{v:.2}"), false);
        });
        group(ui, "spiral.line", "Line", true, 5, |ui| {
            slider_def(ui, "Displacement", &mut p.amp, Some(d.amp), 0.0..=0.5, |v| format!("{v:.3}"), false);
            slider_def(ui, "Gain", &mut p.gain, Some(d.gain), 0.1..=4.0, |v| format!("{v:.2}"), true);
            slider_def(ui, "Width", &mut p.width, Some(d.width), 0.5..=4.0, |v| format!("{v:.1} px"), false);
            slider_def(ui, "Intensity", &mut p.intensity, Some(d.intensity), 0.05..=2.0, |v| format!("{v:.2}"), true);
            slider_def(ui, "Reacts", &mut p.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "One line through points stepped round by an angle; as the step drifts, the figure passes through arms, rosettes and stars. The waveform pushes the points in and out; beats kick it. After the spirals in Sonia Boller's audible-visuals.");
        });
        ui.add_space(6.0);
        if ghost_button(ui, "Reset spiral", false).clicked() {
            *p = Default::default();
        }
        layer.put(ID, &params);
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "spiral.wgsl");
        scene::pipeline(device, "spiral", layout, &m, HDR_FORMAT, Some(scene::ADDITIVE))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_layout_matches_wgsl() {
        assert_eq!(std::mem::offset_of!(Uniform, tint), 48);
        assert_eq!(std::mem::offset_of!(Uniform, palette), 80);
    }
}
