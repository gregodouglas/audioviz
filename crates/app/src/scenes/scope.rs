//! Scene 1: phosphor XY oscilloscope.
//!
//!   1. decay — HDR ping-pong, new = old * decay   (phosphor persistence)
//!   2. beam  — additive instanced segments, tinted (the electron beam)
//!
//! Beam brightness per segment is inversely proportional to its on-screen
//! length, so a fast-moving beam is dim and a lingering one is bright.
//!
//! With a palette the beam's colour comes from the key and its speed, or,
//! in frequency mode, from each point's own pitch: a zero-crossing estimate
//! of the mid signal, mapped by octave over `F_LO`..`F_HI` (`frequency_positions`).

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, Scene, begin_pass, fullscreen_pass, hdr_texture, HDR_FORMAT};
use crate::shaders::ShaderSet;
use crate::settings::{Layer, ScopeColour};

pub const MAX_POINTS: usize = 32_768;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct ScopeParams {
    pub resolution: [f32; 2],
    pub gain: f32,
    pub rotation: f32,
    pub beam_width: f32,
    pub beam_intensity: f32,
    pub decay: f32,
    /// 0 = key and speed pick the palette position, 1 = each point's frequency.
    pub colour_mode: f32,
    pub tint: [f32; 4],
    pub point_count: u32,
    pub _pad2: [u32; 3],
    /// Palette: how far to take it, its shift, the key's position, and how
    /// much beam speed moves along it.
    pub palette_mix: f32,
    pub palette_shift: f32,
    pub key_pos: f32,
    pub speed_spread: f32,
    pub palette: crate::color::Table,
}

struct Pipelines {
    decay: wgpu::RenderPipeline,
    beam: wgpu::RenderPipeline,
}

/// Frequency range the palette spans in frequency mode, bottom to top, by octave.
const F_LO: f32 = 55.0;
const F_HI: f32 = 7040.0;

/// Palette position (0..1) of a frequency: by octave over `F_LO`..`F_HI`.
fn frequency_position(f: f32) -> f32 {
    if f <= 0.0 {
        return 0.0;
    }
    ((f / F_LO).log2() / (F_HI / F_LO).log2()).clamp(0.0, 1.0)
}

/// Per-point palette positions from the trace's instantaneous frequency.
/// The mid signal's zero-crossing intervals give half periods; the points in
/// each interval take the smoothed frequency, so the colour settles instead
/// of flickering between crossings. `smoothed` carries across frames and is
/// held through silence.
pub fn frequency_positions(points: &[[f32; 2]], sample_rate: f32, smoothed: &mut f32, out: &mut Vec<f32>) {
    out.clear();
    out.resize(points.len(), 0.0);
    let mid = |p: &[f32; 2]| p[0] + p[1];
    let mut last_cross: Option<usize> = None;
    let mut start = 0;
    for i in 1..points.len() {
        let (a, b) = (mid(&points[i - 1]), mid(&points[i]));
        if (a < 0.0) != (b < 0.0) {
            if let Some(c) = last_cross {
                let f = sample_rate / (2.0 * (i - c) as f32);
                *smoothed = if *smoothed > 0.0 { *smoothed * 0.6 + f * 0.4 } else { f };
                let pos = frequency_position(*smoothed);
                out[start..i].fill(pos);
                start = i;
            }
            last_cross = Some(i);
        }
    }
    out[start..].fill(frequency_position(*smoothed));
}

pub struct Scope {
    hdr: [wgpu::TextureView; 2],
    current: usize,
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
    points_buf: wgpu::Buffer,
    tex_layout: wgpu::BindGroupLayout,
    beam_layout: wgpu::BindGroupLayout,
    hdr_groups: [wgpu::BindGroup; 2],
    beam_group: wgpu::BindGroup,
    pipelines: Pipelines,
    /// Last frame's final point (x, y, palette position, 0), so the trace is continuous.
    last_point: [f32; 4],
    /// Frequency mode: the smoothed estimate carried across frames.
    freq: f32,
    freq_pos: Vec<f32>,
}

impl Scope {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let sampler = scene::linear_sampler(device);
        let params_buf = scene::uniform_buffer(device, "scope params", std::mem::size_of::<ScopeParams>() as u64);
        let points_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scope points"),
            size: (MAX_POINTS * 16) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tex_layout = scene::tex_layout(device, "scope tex", 0);
        let beam_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scope beam layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let beam_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scope beam group"),
            layout: &beam_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: points_buf.as_entire_binding() },
            ],
        });
        let pipelines = build(device, shaders, &tex_layout, &beam_layout)?;
        let hdr = [hdr_texture(device, "scope hdr A", width, height), hdr_texture(device, "scope hdr B", width, height)];
        let hdr_groups = [
            scene::tex_group(device, &tex_layout, &[&hdr[0]], &sampler, &params_buf),
            scene::tex_group(device, &tex_layout, &[&hdr[1]], &sampler, &params_buf),
        ];
        Ok(Self { hdr, current: 0, sampler, params_buf, points_buf, tex_layout, beam_layout, hdr_groups, beam_group, pipelines, last_point: [0.0; 4], freq: 0.0, freq_pos: Vec::new() })
    }
}

/// Registry id: the layer's `visual`, its parameter key, the presets folder.
pub const ID: &str = "scope";

fn make(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
    Ok(Box::new(Scope::new(device, width, height, shaders)?))
}

pub fn def() -> crate::visuals::VisualDef {
    crate::visuals::VisualDef {
        id: ID,
        name: "Phosphor scope",
        short: "Scope",
        summary: "The waveform drawn as a glowing phosphor beam (left and right channels as X and Y). Lines, cheap.",
        make,
        params: crate::visuals::typed::<crate::settings::ScopeParams>,
        band_waveform: true,
    }
}

impl Scene for Scope {
    fn id(&self) -> &'static str {
        ID
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.hdr = [hdr_texture(device, "scope hdr A", width, height), hdr_texture(device, "scope hdr B", width, height)];
        self.hdr_groups = [
            scene::tex_group(device, &self.tex_layout, &[&self.hdr[0]], &self.sampler, &self.params_buf),
            scene::tex_group(device, &self.tex_layout, &[&self.hdr[1]], &self.sampler, &self.params_buf),
        ];
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipelines = build(device, shaders, &self.tex_layout, &self.beam_layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let s = &layer.get::<crate::settings::ScopeParams>(ID);
        let f = ctx.features;
        let gain = if s.auto_gain { s.gain * ctx.auto_gain } else { s.gain };
        let pulse = 1.0 + s.beat_pulse * f.beat_pulse * f.beat_confidence;

        // Continuous trace: prepend last frame's final point. Each point
        // carries its palette position (frequency mode) in z.
        let by_frequency = s.colour == ScopeColour::Frequency && ctx.palette.is_some();
        if by_frequency {
            frequency_positions(ctx.points, ctx.sample_rate as f32, &mut self.freq, &mut self.freq_pos);
        }
        let mut points: Vec<[f32; 4]> = Vec::with_capacity(ctx.points.len() + 1);
        points.push(self.last_point);
        points.extend(ctx.points.iter().enumerate().map(|(i, p)| [p[0], p[1], if by_frequency { self.freq_pos[i] } else { 0.0 }, 0.0]));
        if let Some(p) = points.last() {
            self.last_point = *p;
        }
        let n = points.len().min(MAX_POINTS);

        let params = ScopeParams {
            resolution: [ctx.width as f32, ctx.height as f32],
            gain,
            rotation: s.rotation_deg.to_radians(),
            beam_width: s.beam_width,
            beam_intensity: s.beam_intensity * pulse,
            decay: s.persistence.max(1e-6).powf(ctx.dt),
            colour_mode: by_frequency as u32 as f32,
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            point_count: n as u32,
            _pad2: [0; 3],
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            key_pos: audioviz_analysis::chroma::pitch_class_hue(f.key_class),
            speed_spread: 0.35,
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        if n > 0 {
            queue.write_buffer(&self.points_buf, 0, bytemuck::cast_slice(&points[..n]));
        }

        let prev = self.current;
        let cur = 1 - prev;
        fullscreen_pass(encoder, "scope decay", &self.hdr[cur], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipelines.decay, &self.hdr_groups[prev]);
        if n >= 2 {
            let mut pass = begin_pass(encoder, "scope beam", &self.hdr[cur], wgpu::LoadOp::Load);
            pass.set_pipeline(&self.pipelines.beam);
            pass.set_bind_group(0, &self.beam_group, &[]);
            pass.draw(0..6, 0..(n as u32 - 1));
        }
        self.current = cur;
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.hdr[self.current]
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        let mut params = layer.get::<crate::settings::ScopeParams>(ID);
        let s = &mut params;
        let d = crate::settings::ScopeParams::default();
        use crate::widgets::{ghost_button, group, hint, segmented, slider_def, small_caps, toggle_row};
        group(ui, "scope.beam", "Beam", true, 4, |ui| {
            slider_def(ui, "Gain", &mut s.gain, Some(d.gain), 0.05..=3.0, |v| format!("{v:.2}"), true);
            toggle_row(ui, "Auto gain", &mut s.auto_gain);
            slider_def(ui, "Beam width", &mut s.beam_width, Some(d.beam_width), 0.3..=6.0, |v| format!("{v:.1} px"), false);
            slider_def(ui, "Beam intensity", &mut s.beam_intensity, Some(d.beam_intensity), 0.05..=20.0, |v| format!("{v:.2}"), true);
        });
        group(ui, "scope.trace", "Trace", true, 3, |ui| {
            slider_def(ui, "Rotation", &mut s.rotation_deg, Some(d.rotation_deg), -180.0..=180.0, |v| format!("{v:.0}°"), false);
            slider_def(ui, "Persistence", &mut s.persistence, Some(d.persistence), 0.0001..=0.9, |v| format!("{v:.3} /s"), true);
            slider_def(ui, "Beat pulse", &mut s.beat_pulse, Some(d.beat_pulse), 0.0..=1.0, |v| format!("{v:.2}"), false);
        });
        group(ui, "scope.colour", "Colour", true, 1, |ui| {
            small_caps(ui, "Palette by");
            let names: Vec<&str> = ScopeColour::ALL.iter().map(|c| c.name()).collect();
            if let Some(i) = segmented(ui, &names, ScopeColour::ALL.iter().position(|c| *c == s.colour).unwrap_or(0)) {
                s.colour = ScopeColour::ALL[i];
            }
            hint(ui, "With a palette (Colour tab): the key picks the colour and fast travel moves along it, or each part of the trace takes the colour of its own pitch, bass at the start of the palette and treble at the end.");
        });
        ui.add_space(6.0);
        if ghost_button(ui, "Reset scope", false).clicked() {
            *s = Default::default();
        }
        layer.put(ID, &params);
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, tex_layout: &wgpu::BindGroupLayout, beam_layout: &wgpu::BindGroupLayout) -> Result<Pipelines, String> {
    scene::guarded(device, || {
        let decay = scene::shader(device, shaders, "decay.wgsl");
        let beam = scene::shader(device, shaders, "beam.wgsl");
        Pipelines {
            decay: scene::pipeline(device, "scope decay", tex_layout, &decay, HDR_FORMAT, None),
            beam: scene::pipeline(device, "scope beam", beam_layout, &beam, HDR_FORMAT, Some(scene::ADDITIVE)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_positions_follow_a_sine() {
        let sr = 48_000.0;
        let pts: Vec<[f32; 2]> = (0..4800).map(|i| { let v = (i as f32 / sr * 440.0 * std::f32::consts::TAU).sin(); [v, v] }).collect();
        let mut f = 0.0;
        let mut out = Vec::new();
        frequency_positions(&pts, sr, &mut f, &mut out);
        assert_eq!(out.len(), pts.len());
        assert!((f - 440.0).abs() < 5.0, "smoothed {f}");
        let want = frequency_position(440.0);
        assert!((out[2400] - want).abs() < 0.02, "{} vs {want}", out[2400]);
        assert!((want - 3.0 / 7.0).abs() < 1e-3, "three octaves above 55 Hz over seven");
    }

    #[test]
    fn frequency_positions_hold_through_silence() {
        let mut f = 1000.0;
        let mut out = Vec::new();
        frequency_positions(&[[0.0; 2]; 100], 48_000.0, &mut f, &mut out);
        assert_eq!(f, 1000.0);
        assert!(out.iter().all(|&p| (p - frequency_position(1000.0)).abs() < 1e-6));
    }
}
