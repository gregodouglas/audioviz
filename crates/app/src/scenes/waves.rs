//! Waves: a landscape of waveform (or spectrum) rows in perspective, the
//! Unknown Pleasures look. Every `1/rate` seconds the newest slice of the
//! signal becomes a row at the front; older rows recede toward the horizon.
//! Each row's fill hides the rows behind it, so loud passages become ridges.
//! The lines run through the palette by age or by the row's loudness.

use std::collections::VecDeque;

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, begin_pass};
use crate::shaders::ShaderSet;
use crate::settings::{Layer, VisualKind, WavesColour, WavesSource};

/// Samples per row (points along x).
pub const COLS: usize = 256;
/// Ring capacity; the active count is the `rows` parameter.
pub const MAX_ROWS: usize = 160;
/// Waveform rows average this many samples per point.
const DECIM: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    resolution: [f32; 2],
    rows: u32,
    cols: u32,
    head: u32,
    source: u32,
    colour: u32,
    _pad0: u32,
    height: f32,
    spread: f32,
    persp: f32,
    near_y: f32,
    far_y: f32,
    width: f32,
    fade: f32,
    time: f32,
    tint: [f32; 4],
    palette_mix: f32,
    palette_shift: f32,
    _pad1: [f32; 2],
    palette: crate::color::Table,
}

/// Premultiplied alpha: the fill covers, the line blends by its coverage.
const PREMULTIPLIED: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
};

pub struct Waves {
    hdr: wgpu::TextureView,
    uniform: wgpu::Buffer,
    rows_buf: wgpu::Buffer,
    levels_buf: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    /// Newest row's ring index.
    head: usize,
    /// Seconds owed to the row clock.
    acc: f32,
    /// Recent mid-signal samples for waveform rows.
    ring: VecDeque<f32>,
    row: Vec<f32>,
}

impl Waves {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let uniform = scene::uniform_buffer(device, "waves params", std::mem::size_of::<Uniform>() as u64);
        let storage = |label: &str, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
        };
        let rows_buf = storage("waves rows", (MAX_ROWS * COLS * 4) as u64);
        let levels_buf = storage("waves levels", (MAX_ROWS * 4) as u64);
        let entry = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("waves layout"),
            entries: &[entry(0, wgpu::BufferBindingType::Uniform), entry(1, wgpu::BufferBindingType::Storage { read_only: true }), entry(2, wgpu::BufferBindingType::Storage { read_only: true })],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("waves group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: rows_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: levels_buf.as_entire_binding() },
            ],
        });
        let pipeline = build(device, shaders, &layout)?;
        Ok(Self { hdr: scene::hdr_texture(device, "waves hdr", width, height), uniform, rows_buf, levels_buf, layout, group, pipeline, head: 0, acc: 0.0, ring: VecDeque::with_capacity(COLS * DECIM * 2), row: vec![0.0; COLS] })
    }

    /// Build the next row from the signal and push it into the ring.
    fn push_row(&mut self, queue: &wgpu::Queue, ctx: &FrameCtx, layer: &Layer, rows: usize) {
        let p = &layer.waves;
        match p.source {
            WavesSource::Waveform => {
                let window = COLS * DECIM;
                let have = self.ring.len().min(window);
                let start = self.ring.len() - have;
                let gain = p.gain * ctx.auto_gain;
                for (i, v) in self.row.iter_mut().enumerate() {
                    let lo = start + i * have / COLS;
                    let hi = start + (i + 1) * have / COLS;
                    let n = hi.saturating_sub(lo);
                    *v = if n == 0 { 0.0 } else { self.ring.range(lo..hi).sum::<f32>() / n as f32 * gain };
                }
            }
            WavesSource::Spectrum => {
                let bands = &ctx.features.bands;
                let n = bands.len();
                for (i, v) in self.row.iter_mut().enumerate() {
                    *v = if n == 0 {
                        0.0
                    } else {
                        let x = i as f32 / (COLS - 1) as f32 * (n - 1) as f32;
                        let (k, f) = (x.floor() as usize, x.fract());
                        let a = bands[k.min(n - 1)];
                        let b = bands[(k + 1).min(n - 1)];
                        ((a + (b - a) * f).max(0.0).sqrt() * p.gain).min(2.0)
                    };
                }
            }
        }
        // Smoothing: a running box over the row, applied `smooth` times.
        for _ in 0..p.smooth.min(8) {
            let prev = self.row.clone();
            for i in 1..COLS - 1 {
                self.row[i] = (prev[i - 1] + prev[i] + prev[i + 1]) / 3.0;
            }
        }
        let level = (self.row.iter().map(|v| v * v).sum::<f32>() / COLS as f32).sqrt();
        self.head = (self.head + 1) % rows.max(1);
        queue.write_buffer(&self.rows_buf, (self.head * COLS * 4) as u64, bytemuck::cast_slice(&self.row));
        queue.write_buffer(&self.levels_buf, (self.head * 4) as u64, bytemuck::bytes_of(&(level * 2.5).min(1.0)));
    }
}

impl Scene for Waves {
    fn kind(&self) -> VisualKind {
        VisualKind::Waves
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.hdr = scene::hdr_texture(device, "waves hdr", width, height);
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipeline = build(device, shaders, &self.layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let p = &layer.waves;
        let rows = (p.rows as usize).clamp(2, MAX_ROWS);
        // Keep the newest signal; a row is a box-filtered window of it.
        for s in ctx.points {
            self.ring.push_back((s[0] + s[1]) * 0.5);
        }
        while self.ring.len() > COLS * DECIM * 2 {
            self.ring.pop_front();
        }
        // The row clock runs at `rate` rows per second whatever the frame rate.
        self.acc += ctx.dt;
        let period = 1.0 / p.rate.max(1.0);
        let mut pushed = 0;
        while self.acc >= period && pushed < 4 {
            self.acc -= period;
            self.push_row(queue, ctx, layer, rows);
            pushed += 1;
        }
        if self.acc > period * 4.0 {
            self.acc = 0.0;
        }
        if self.head >= rows {
            self.head = 0;
        }
        let u = Uniform {
            resolution: [ctx.width as f32, ctx.height as f32],
            rows: rows as u32,
            cols: COLS as u32,
            head: self.head as u32,
            source: (p.source == WavesSource::Spectrum) as u32,
            colour: (p.colour == WavesColour::Level) as u32,
            _pad0: 0,
            height: p.height,
            spread: p.spread,
            persp: p.perspective,
            near_y: p.near,
            far_y: p.horizon,
            width: p.width,
            fade: p.fade,
            time: ctx.time,
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            _pad1: [0.0; 2],
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        let mut pass = begin_pass(encoder, "waves", &self.hdr, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..6, 0..(rows as u32 * (COLS as u32 - 1) * 2));
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.hdr
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        use crate::widgets::{ghost_button, group, hint, segmented, slider_def, small_caps};
        let p = &mut layer.waves;
        let d = crate::settings::WavesParams::default();
        group(ui, "waves.landscape", "Landscape", true, 7, |ui| {
            small_caps(ui, "Rows show");
            if let Some(i) = segmented(ui, &["Waveform", "Spectrum"], (p.source == WavesSource::Spectrum) as usize) {
                p.source = if i == 1 { WavesSource::Spectrum } else { WavesSource::Waveform };
            }
            slider_def(ui, "Rows", &mut p.rows, Some(d.rows), 4..=MAX_ROWS as u32, |v| format!("{v:.0}"), false);
            slider_def(ui, "Rate", &mut p.rate, Some(d.rate), 2.0..=90.0, |v| format!("{v:.0} /s"), false);
            slider_def(ui, "Height", &mut p.height, Some(d.height), 0.0..=1.5, |v| format!("{v:.2}"), false);
            slider_def(ui, "Spread", &mut p.spread, Some(d.spread), 0.1..=1.5, |v| format!("{v:.2}"), false);
            slider_def(ui, "Perspective", &mut p.perspective, Some(d.perspective), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Horizon", &mut p.horizon, Some(d.horizon), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Near edge", &mut p.near, Some(d.near), 0.0..=1.0, |v| format!("{v:.2}"), false);
        });
        group(ui, "waves.line", "Line", true, 5, |ui| {
            slider_def(ui, "Width", &mut p.width, Some(d.width), 0.5..=6.0, |v| format!("{v:.1} px"), false);
            slider_def(ui, "Gain", &mut p.gain, Some(d.gain), 0.1..=4.0, |v| format!("{v:.2}"), true);
            slider_def(ui, "Smoothing", &mut p.smooth, Some(d.smooth), 0..=8, |v| format!("{v:.0}"), false);
            slider_def(ui, "Distance fade", &mut p.fade, Some(d.fade), 0.0..=1.0, |v| format!("{v:.2}"), false);
            small_caps(ui, "Palette by");
            if let Some(i) = segmented(ui, &["Age", "Level"], (p.colour == WavesColour::Level) as usize) {
                p.colour = if i == 1 { WavesColour::Level } else { WavesColour::Age };
            }
            hint(ui, "Each row is one slice of the signal; rows march toward the horizon and hide what is behind them. With a palette, age runs the colours front to back, or level colours the loud rows.");
        });
        ui.add_space(6.0);
        if ghost_button(ui, "Reset waves", false).clicked() {
            *p = Default::default();
        }
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "waves.wgsl");
        scene::pipeline(device, "waves", layout, &m, HDR_FORMAT, Some(PREMULTIPLIED))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_layout_matches_wgsl() {
        assert_eq!(std::mem::offset_of!(Uniform, tint), 64);
        assert_eq!(std::mem::offset_of!(Uniform, palette), 96);
        assert_eq!(std::mem::size_of::<Uniform>(), 96 + 32 * 16);
    }
}
