//! Scene 1: phosphor XY oscilloscope.
//!
//!   1. decay — HDR ping-pong, new = old * decay   (phosphor persistence)
//!   2. beam  — additive instanced segments, tinted (the electron beam)
//!
//! Beam brightness per segment is inversely proportional to its on-screen
//! length, so a fast-moving beam is dim and a lingering one is bright.

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, Scene, begin_pass, fullscreen_pass, hdr_texture, HDR_FORMAT};
use crate::shaders::ShaderSet;
use crate::settings::Settings;

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
    pub _pad: f32,
    pub tint: [f32; 4],
    pub point_count: u32,
    pub _pad2: [u32; 3],
}

struct Pipelines {
    decay: wgpu::RenderPipeline,
    beam: wgpu::RenderPipeline,
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
    last_point: [f32; 2],
}

impl Scope {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let sampler = scene::linear_sampler(device);
        let params_buf = scene::uniform_buffer(device, "scope params", std::mem::size_of::<ScopeParams>() as u64);
        let points_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scope points"),
            size: (MAX_POINTS * 8) as u64,
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
        Ok(Self { hdr, current: 0, sampler, params_buf, points_buf, tex_layout, beam_layout, hdr_groups, beam_group, pipelines, last_point: [0.0; 2] })
    }
}

impl Scene for Scope {
    fn name(&self) -> &'static str {
        "Phosphor scope"
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

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx) {
        let s = ctx.settings;
        let f = ctx.features;
        let gain = if s.auto_gain { s.gain * ctx.auto_gain } else { s.gain };
        let pulse = 1.0 + s.beat_pulse * f.beat_pulse * f.beat_confidence;

        // Continuous trace: prepend last frame's final point.
        let mut points = Vec::with_capacity(ctx.points.len() + 1);
        points.push(self.last_point);
        points.extend_from_slice(ctx.points);
        if let Some(p) = ctx.points.last() {
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
            _pad: 0.0,
            tint: [s.tint[0], s.tint[1], s.tint[2], 1.0],
            point_count: n as u32,
            _pad2: [0; 3],
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

    fn ui(&mut self, ui: &mut egui::Ui, s: &mut Settings) {
        if ui.button("Reset scope to defaults").clicked() {
            s.reset_scope();
        }
        ui.add(egui::Slider::new(&mut s.gain, 0.05..=3.0).logarithmic(true).text("gain"));
        ui.checkbox(&mut s.auto_gain, "auto gain");
        ui.add(egui::Slider::new(&mut s.rotation_deg, -180.0..=180.0).text("rotation °"));
        ui.add(egui::Slider::new(&mut s.beam_width, 0.3..=6.0).text("beam width px"));
        ui.add(egui::Slider::new(&mut s.beam_intensity, 0.05..=20.0).logarithmic(true).text("beam intensity"));
        ui.add(egui::Slider::new(&mut s.persistence, 0.0001..=0.9).logarithmic(true).text("persistence /s"));
        ui.add(egui::Slider::new(&mut s.beat_pulse, 0.0..=1.0).text("beat pulse"));
        ui.horizontal(|ui| {
            ui.label("tint");
            ui.color_edit_button_rgb(&mut s.tint);
            if ui.button("P1 green").clicked() {
                s.tint = [0.40, 1.0, 0.55];
            }
            if ui.button("P4 white").clicked() {
                s.tint = [0.85, 0.9, 1.0];
            }
            if ui.button("amber").clicked() {
                s.tint = [1.0, 0.65, 0.2];
            }
            if ui.button("ice").clicked() {
                s.tint = [0.55, 0.8, 1.0];
            }
        });
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
