//! Loop: concentric waveform rings. Every `1/rate` seconds the newest slice
//! of the waveform is bent into a closed ring at the centre; older rings sit
//! further out, so a sound travels outward ring by ring, each one lagging the
//! last. A ring's brightness follows its own loudness, so quiet moments leave
//! dark gaps in the ripple. Lines only, additive; the palette runs by age or
//! by level.
//!
//! After the Loop visualizer in Uberviz by Felix Turner / Airtight
//! Interactive, https://uberviz.io. No code used; written fresh from a
//! description of the effect (see docs/INSPIRATION.md).

use std::collections::VecDeque;

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, begin_pass};
use crate::shaders::ShaderSet;
use crate::settings::{Layer, LoopParams, WavesColour};

/// Points round each ring.
pub const COLS: usize = 256;
/// Ring capacity; the active count is the `rings` parameter.
pub const MAX_RINGS: usize = 120;
/// Each point averages this many samples.
const DECIM: usize = 3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    resolution: [f32; 2],
    rings: u32,
    cols: u32,
    head: u32,
    colour: u32,
    _pad0: [u32; 2],
    inner: f32,
    outer: f32,
    amp: f32,
    width: f32,
    fade: f32,
    level: f32,
    spin: f32,
    twist: f32,
    tint: [f32; 4],
    palette_mix: f32,
    palette_shift: f32,
    _pad1: [f32; 2],
    palette: crate::color::Table,
}

pub struct Loop {
    hdr: wgpu::TextureView,
    uniform: wgpu::Buffer,
    rings_buf: wgpu::Buffer,
    levels_buf: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    /// Newest ring's index.
    head: usize,
    /// Seconds owed to the ring clock.
    acc: f32,
    /// Accumulated turn, radians.
    angle: f32,
    /// Slewed beat kick on the amplitude.
    kick: f32,
    last_beat: f32,
    ring: VecDeque<f32>,
    row: Vec<f32>,
}

impl Loop {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let uniform = scene::uniform_buffer(device, "loop params", std::mem::size_of::<Uniform>() as u64);
        let storage = |label: &str, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
        };
        let rings_buf = storage("loop rings", (MAX_RINGS * COLS * 4) as u64);
        let levels_buf = storage("loop levels", (MAX_RINGS * 4) as u64);
        let entry = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loop layout"),
            entries: &[entry(0, wgpu::BufferBindingType::Uniform), entry(1, wgpu::BufferBindingType::Storage { read_only: true }), entry(2, wgpu::BufferBindingType::Storage { read_only: true })],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("loop group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: rings_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: levels_buf.as_entire_binding() },
            ],
        });
        let pipeline = build(device, shaders, &layout)?;
        Ok(Self { hdr: scene::hdr_texture(device, "loop hdr", width, height), uniform, rings_buf, levels_buf, layout, group, pipeline, head: 0, acc: 0.0, angle: 0.0, kick: 0.0, last_beat: 0.0, ring: VecDeque::with_capacity(COLS * DECIM * 2), row: vec![0.0; COLS] })
    }

    /// Bend the newest waveform window into a closed ring and push it.
    fn push_ring(&mut self, queue: &wgpu::Queue, ctx: &FrameCtx, p: &LoopParams, rings: usize) {
        let window = COLS * DECIM;
        let have = self.ring.len().min(window);
        let start = self.ring.len() - have;
        let gain = p.gain * ctx.auto_gain;
        let mut raw = vec![0.0f32; COLS];
        for (i, v) in raw.iter_mut().enumerate() {
            let lo = start + i * have / COLS;
            let hi = start + (i + 1) * have / COLS;
            let n = hi.saturating_sub(lo);
            *v = if n == 0 { 0.0 } else { self.ring.range(lo..hi).sum::<f32>() / n as f32 * gain };
        }
        // Close the ring without a seam: two copies half a turn apart under
        // sin² / cos² windows (they sum to one), so the ends meet the middle.
        for i in 0..COLS {
            let w = (std::f32::consts::PI * i as f32 / COLS as f32).sin().powi(2);
            self.row[i] = raw[i] * w + raw[(i + COLS / 2) % COLS] * (1.0 - w);
        }
        for _ in 0..p.smooth.min(8) {
            let prev = self.row.clone();
            for i in 0..COLS {
                self.row[i] = (prev[(i + COLS - 1) % COLS] + prev[i] + prev[(i + 1) % COLS]) / 3.0;
            }
        }
        let level = (self.row.iter().map(|v| v * v).sum::<f32>() / COLS as f32).sqrt();
        self.head = (self.head + 1) % rings.max(1);
        queue.write_buffer(&self.rings_buf, (self.head * COLS * 4) as u64, bytemuck::cast_slice(&self.row));
        queue.write_buffer(&self.levels_buf, (self.head * 4) as u64, bytemuck::bytes_of(&(level * 3.0).min(1.0)));
    }
}

/// Registry id: the layer's `visual`, its parameter key, the presets folder.
pub const ID: &str = "loop";

fn make(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
    Ok(Box::new(Loop::new(device, width, height, shaders)?))
}

pub fn def() -> crate::visuals::VisualDef {
    crate::visuals::VisualDef {
        id: ID,
        name: "Loop",
        short: "Loop",
        summary: "Waveform rings born at the centre and pushed outward, each lagging the last; quiet slices leave dark rings.",
        source: crate::visuals::Source::Native { make, params: crate::visuals::typed::<LoopParams> },
        band_waveform: false,
    }
}

impl Scene for Loop {
    fn id(&self) -> &'static str {
        ID
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.hdr = scene::hdr_texture(device, "loop hdr", width, height);
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipeline = build(device, shaders, &self.layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let p = layer.get::<LoopParams>(ID);
        let rings = (p.rings as usize).clamp(2, MAX_RINGS);
        for s in ctx.points {
            self.ring.push_back((s[0] + s[1]) * 0.5);
        }
        while self.ring.len() > COLS * DECIM * 2 {
            self.ring.pop_front();
        }
        self.acc += ctx.dt;
        let period = 1.0 / p.rate.max(1.0);
        let mut pushed = 0;
        while self.acc >= period && pushed < 4 {
            self.acc -= period;
            self.push_ring(queue, ctx, &p, rings);
            pushed += 1;
        }
        if self.acc > period * 4.0 {
            self.acc = 0.0;
        }
        if self.head >= rings {
            self.head = 0;
        }
        let f = ctx.features;
        let energy = ((f.bass_att + f.mid_att + f.treb_att) / 3.0).clamp(0.0, 2.5);
        self.angle = (self.angle + ctx.dt * p.spin * (0.5 + 0.5 * energy)).rem_euclid(std::f32::consts::TAU);
        let beat = f.beat_pulse * f.beat_confidence;
        if beat > self.last_beat + 0.4 {
            self.kick = 1.0;
        }
        self.last_beat = beat;
        self.kick *= (-ctx.dt / 0.25).exp();
        let u = Uniform {
            resolution: [ctx.width as f32, ctx.height as f32],
            rings: rings as u32,
            cols: COLS as u32,
            head: self.head as u32,
            colour: (p.colour == WavesColour::Level) as u32,
            _pad0: [0; 2],
            inner: p.inner,
            outer: p.outer.max(p.inner + 0.01),
            amp: p.amp * (1.0 + p.react * self.kick),
            width: p.width,
            fade: p.fade,
            level: p.level,
            spin: self.angle,
            twist: p.twist.to_radians(),
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            _pad1: [0.0; 2],
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        let mut pass = begin_pass(encoder, "loop", &self.hdr, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..6, 0..(rings * COLS) as u32);
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.hdr
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        use crate::widgets::{ghost_button, group, hint, segmented, slider_def, small_caps};
        let mut params = layer.get::<LoopParams>(ID);
        let p = &mut params;
        let d = LoopParams::default();
        group(ui, "loop.rings", "Rings", true, 7, |ui| {
            slider_def(ui, "Rings", &mut p.rings, Some(d.rings), 4..=MAX_RINGS as u32, |v| format!("{v:.0}"), false);
            slider_def(ui, "Rate", &mut p.rate, Some(d.rate), 2.0..=90.0, |v| format!("{v:.0} /s"), false);
            slider_def(ui, "Inner radius", &mut p.inner, Some(d.inner), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Outer radius", &mut p.outer, Some(d.outer), 0.1..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Amplitude", &mut p.amp, Some(d.amp), 0.0..=0.5, |v| format!("{v:.3}"), false);
            slider_def(ui, "Spin", &mut p.spin, Some(d.spin), -2.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Twist", &mut p.twist, Some(d.twist), -20.0..=20.0, |v| format!("{v:.1}°"), false);
        });
        group(ui, "loop.line", "Line", true, 7, |ui| {
            slider_def(ui, "Width", &mut p.width, Some(d.width), 0.5..=6.0, |v| format!("{v:.1} px"), false);
            slider_def(ui, "Gain", &mut p.gain, Some(d.gain), 0.1..=4.0, |v| format!("{v:.2}"), true);
            slider_def(ui, "Smoothing", &mut p.smooth, Some(d.smooth), 0..=8, |v| format!("{v:.0}"), false);
            slider_def(ui, "Brightness by level", &mut p.level, Some(d.level), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Age fade", &mut p.fade, Some(d.fade), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Reacts", &mut p.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            small_caps(ui, "Palette by");
            if let Some(i) = segmented(ui, &["Age", "Level"], (p.colour == WavesColour::Level) as usize) {
                p.colour = if i == 1 { WavesColour::Level } else { WavesColour::Age };
            }
            hint(ui, "Each ring is one slice of the waveform, born at the centre and pushed outward by the next, so a sound ripples out ring by ring; quiet slices leave dark rings. Twist turns each ring against the last for a spiral. After the Loop in Felix Turner's Uberviz.");
        });
        ui.add_space(6.0);
        if ghost_button(ui, "Reset loop", false).clicked() {
            *p = Default::default();
        }
        layer.put(ID, &params);
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "loop.wgsl");
        scene::pipeline(device, "loop", layout, &m, HDR_FORMAT, Some(scene::ADDITIVE))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_layout_matches_wgsl() {
        assert_eq!(std::mem::offset_of!(Uniform, inner), 32);
        assert_eq!(std::mem::offset_of!(Uniform, tint), 64);
        assert_eq!(std::mem::offset_of!(Uniform, palette), 96);
        assert_eq!(std::mem::size_of::<Uniform>(), 96 + 32 * 16);
    }
}
