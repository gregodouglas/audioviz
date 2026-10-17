//! Black hole visual: a Schwarzschild lens with an edge-on accretion disk in
//! front of a point starfield (see `starfield.wgsl`). Keeps the `Starfield`
//! kind and `StarfieldParams` name so settings carry over.
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
    pub rs: f32,
    pub tint: [f32; 4],
    pub bands: [[f32; 4]; 4],
    pub onset: f32,
    pub beat: f32,
    pub energy: f32,
    pub density: f32,
    pub twinkle: f32,
    pub streams: f32,
    pub disk: f32,
    pub doppler: f32,
    pub tilt: f32,
    pub phase: f32,
    pub turb: f32,
    pub flare: f32,
    pub az: f32,
    pub palette_mix: f32,
    pub palette_shift: f32,
    pub nebula: f32,
    pub palette: crate::color::Table,
}

pub struct Starfield {
    out: wgpu::TextureView,
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    /// Slewed bass, 0..1: the hole's mass breathes with it.
    breath: f32,
    /// Slewed broadband energy, ~1 = average.
    energy: f32,
    /// Disk rotation phase; advances faster when the music is louder.
    phase: f32,
    /// Camera azimuth; the slow orbit that keeps the sky moving.
    az: f32,
}

impl Starfield {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let sampler = scene::linear_sampler(device);
        let params_buf = scene::uniform_buffer(device, "starfield params", std::mem::size_of::<StarParams>() as u64);
        let layout = scene::tex_layout(device, "starfield", 0);
        let out = hdr_texture(device, "starfield out", width, height);
        // The shader needs no input texture; bind a sibling to satisfy the layout.
        let dummy = hdr_texture(device, "starfield dummy", 1, 1);
        let group = scene::tex_group(device, &layout, &[&dummy], &sampler, &params_buf);
        let pipeline = build(device, shaders, &layout)?;
        Ok(Self { out, sampler, params_buf, layout, group, pipeline, breath: 0.0, energy: 1.0, phase: 0.0, az: 0.0 })
    }
}

/// The default phosphor tint, for which the disk runs blackbody instead of green.
const DEFAULT_TINT: [f32; 3] = [0.40, 1.0, 0.55];

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
        let dt = ctx.dt.clamp(0.0, 0.1);
        // Mass follows the slow bass, slewed so the shadow breathes rather than jitters.
        let bass = (f.bass_att / 2.0).clamp(0.0, 1.0);
        self.breath += (bass - self.breath) * (1.0 - (-dt / 0.35).exp());
        let energy = ((f.bass_att + f.mid_att + f.treb_att) / 3.0).clamp(0.0, 2.5);
        self.energy += (energy - self.energy) * (1.0 - (-dt / 0.5).exp());
        self.phase += dt * (0.35 + 0.45 * self.energy);
        self.az += dt * 0.012 * s.drift;
        let rs = s.mass * (1.0 + 0.25 * s.breathe * self.breath);

        let mut bands = [[0.0f32; 4]; 4];
        for i in 0..16 {
            // Fold 32 bands into 16 by pairs.
            let v = f.bands.get(i * 2).copied().unwrap_or(0.0).max(f.bands.get(i * 2 + 1).copied().unwrap_or(0.0));
            bands[i / 4][i % 4] = v;
        }
        let dist = ctx.tint.iter().zip(DEFAULT_TINT.iter()).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        let warm = (1.0 - dist * 12.0).clamp(0.0, 1.0);
        let onset = f.onset_pulse[3].min(1.0);
        let params = StarParams {
            resolution: [ctx.width as f32, ctx.height as f32],
            time: ctx.time,
            rs,
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], warm],
            bands,
            onset,
            beat: f.beat_pulse * f.beat_confidence,
            energy: self.energy,
            density: s.density,
            twinkle: s.twinkle,
            streams: s.streams,
            disk: s.disk,
            doppler: s.doppler,
            tilt: s.tilt.to_radians(),
            phase: self.phase,
            turb: (0.35 + 0.65 * onset + 0.25 * (self.energy - 1.0)).clamp(0.0, 1.5),
            flare: f.onset_pulse[0].min(1.0),
            az: self.az,
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            nebula: s.nebula,
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        fullscreen_pass(encoder, "starfield", &self.out, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipeline, &self.group);
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.out
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        let s = &mut layer.starfield;
        let d = crate::settings::StarfieldParams::default();
        use crate::widgets::{ghost_button, group, hint, slider_def};
        group(ui, "hole.lens", "Lens and disk", true, 6, |ui| {
            slider_def(ui, "Mass", &mut s.mass, Some(d.mass), 0.3..=2.5, |v| format!("{v:.2}"), false);
            slider_def(ui, "Disk brightness", &mut s.disk, Some(d.disk), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Doppler", &mut s.doppler, Some(d.doppler), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Tilt", &mut s.tilt, Some(d.tilt), 0.0..=25.0, |v| format!("{v:.0}°"), false);
            slider_def(ui, "Infall streams", &mut s.streams, Some(d.streams), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Breathe with bass", &mut s.breathe, Some(d.breathe), 0.0..=1.0, |v| format!("{v:.2}"), false);
        });
        group(ui, "hole.sky", "Sky", false, 4, |ui| {
            slider_def(ui, "Nebula", &mut s.nebula, Some(d.nebula), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Star density", &mut s.density, Some(d.density), 0.1..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Onset twinkle", &mut s.twinkle, Some(d.twinkle), 0.0..=2.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Drift", &mut s.drift, Some(d.drift), 0.0..=4.0, |v| format!("{v:.2}"), false);
        });
        hint(ui, "The disk's brightness, turbulence and infall follow energy and onsets; stars shimmer slowly and a few flash on transients.");
        ui.add_space(6.0);
        if ghost_button(ui, "Reset black hole", false).clicked() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn params_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<StarParams>(), 160 + 512);
        assert_eq!(offset_of!(StarParams, tint), 16);
        assert_eq!(offset_of!(StarParams, bands), 32);
        assert_eq!(offset_of!(StarParams, onset), 96);
        assert_eq!(offset_of!(StarParams, twinkle), 112);
        assert_eq!(offset_of!(StarParams, tilt), 128);
        assert_eq!(offset_of!(StarParams, az), 144);
    }
}
