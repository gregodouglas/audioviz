//! Scene 2: chroma-steered flow-field ribbons.
//!
//! A compute pass advects particles through curl noise plus twelve vortices
//! on a circle-of-fifths wheel whose strengths are the chroma vector, then an
//! instanced segment pass draws each particle's step into a persistence
//! buffer (shared decay shader).

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, Scene, begin_pass, fullscreen_pass, hdr_texture, HDR_FORMAT};
use crate::shaders::ShaderSet;
use crate::settings::{Layer, RibbonsParams};

pub const PARTICLES: u32 = 65_536;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct RibbonParams {
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
    pub chroma: [[f32; 4]; 3],
    pub time: f32,
    pub dt: f32,
    pub speed: f32,
    pub field_scale: f32,
    pub onset: f32,
    pub bass: f32,
    pub key_hue: f32,
    pub saturation: f32,
    pub seed: f32,
    pub vortex: f32,
    pub curl: f32,
    pub lifetime: f32,
    pub palette_mix: f32,
    pub palette_shift: f32,
    pub spread: f32,
    pub _pp: f32,
    pub palette: crate::color::Table,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct Particle {
    pos: [f32; 2],
    prev: [f32; 2],
    age: f32,
    life: f32,
    seed: f32,
    pitch: f32,
}

struct Pipelines {
    update: wgpu::ComputePipeline,
    draw: wgpu::RenderPipeline,
    decay: wgpu::RenderPipeline,
}

pub struct Ribbons {
    hdr: [wgpu::TextureView; 2],
    current: usize,
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
    _particles: wgpu::Buffer,
    tex_layout: wgpu::BindGroupLayout,
    update_layout: wgpu::BindGroupLayout,
    draw_layout: wgpu::BindGroupLayout,
    hdr_groups: [wgpu::BindGroup; 2],
    update_group: wgpu::BindGroup,
    draw_group: wgpu::BindGroup,
    pipelines: Pipelines,
    seed: f32,
}

impl Ribbons {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let sampler = scene::linear_sampler(device);
        let params_buf = scene::uniform_buffer(device, "ribbons params", std::mem::size_of::<RibbonParams>() as u64);

        // Particles start scattered with staggered ages so they don't all respawn together.
        let aspect = width as f32 / height.max(1) as f32;
        let mut rng = 0x2545_F491u32;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            (rng >> 8) as f32 / (1u32 << 24) as f32
        };
        let init: Vec<Particle> = (0..PARTICLES)
            .map(|i| {
                let pos = [(next() * 2.0 - 1.0) * aspect, next() * 2.0 - 1.0];
                let life = 3.0 + next() * 5.0;
                Particle { pos, prev: pos, age: next() * life, life, seed: next(), pitch: (i % 12) as f32 }
            })
            .collect();
        let particles = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ribbons particles"),
            size: (PARTICLES as usize * std::mem::size_of::<Particle>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        particles.slice(..).get_mapped_range_mut().map_err(|e| e.to_string())?.copy_from_slice(bytemuck::cast_slice(&init));
        particles.unmap();

        let tex_layout = scene::tex_layout(device, "ribbons tex", 0);
        let storage_layout = |label: &str, read_only: bool, vis: wgpu::ShaderStages| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: vis,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: vis,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
                        count: None,
                    },
                ],
            })
        };
        let update_layout = storage_layout("ribbons update", false, wgpu::ShaderStages::COMPUTE);
        let draw_layout = storage_layout("ribbons draw", true, wgpu::ShaderStages::VERTEX_FRAGMENT);
        let group = |layout: &wgpu::BindGroupLayout| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ribbons group"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: params_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: particles.as_entire_binding() },
                ],
            })
        };
        let update_group = group(&update_layout);
        let draw_group = group(&draw_layout);

        let pipelines = build(device, shaders, &tex_layout, &update_layout, &draw_layout)?;
        let hdr = [hdr_texture(device, "ribbons hdr A", width, height), hdr_texture(device, "ribbons hdr B", width, height)];
        let hdr_groups = [
            scene::tex_group(device, &tex_layout, &[&hdr[0]], &sampler, &params_buf),
            scene::tex_group(device, &tex_layout, &[&hdr[1]], &sampler, &params_buf),
        ];
        Ok(Self { hdr, current: 0, sampler, params_buf, _particles: particles, tex_layout, update_layout, draw_layout, hdr_groups, update_group, draw_group, pipelines, seed: next() * 100.0 })
    }
}

/// Registry id: the layer's `visual`, its parameter key, the presets folder.
pub const ID: &str = "ribbons";

fn make(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
    Ok(Box::new(Ribbons::new(device, width, height, shaders)?))
}

pub fn def() -> crate::visuals::VisualDef {
    crate::visuals::VisualDef {
        id: ID,
        name: "Flow-field ribbons",
        short: "Ribbons",
        summary: "Thousands of particles riding a flow field the music stirs; slow persistence turns them into ribbons or a starfield.",
        source: crate::visuals::Source::Native { make, params: crate::visuals::typed::<RibbonsParams> },
        band_waveform: false,
    }
}

impl Scene for Ribbons {
    fn id(&self) -> &'static str {
        ID
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.hdr = [hdr_texture(device, "ribbons hdr A", width, height), hdr_texture(device, "ribbons hdr B", width, height)];
        self.hdr_groups = [
            scene::tex_group(device, &self.tex_layout, &[&self.hdr[0]], &self.sampler, &self.params_buf),
            scene::tex_group(device, &self.tex_layout, &[&self.hdr[1]], &self.sampler, &self.params_buf),
        ];
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipelines = build(device, shaders, &self.tex_layout, &self.update_layout, &self.draw_layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let s = &layer.get::<RibbonsParams>(ID);
        let f = ctx.features;
        let mut chroma = [[0.0f32; 4]; 3];
        for (i, c) in f.chroma.iter().enumerate() {
            chroma[i / 4][i % 4] = *c;
        }
        let params = RibbonParams {
            resolution: [ctx.width as f32, ctx.height as f32],
            gain: 1.0,
            rotation: ctx.time * 0.02,
            beam_width: s.width,
            beam_intensity: s.intensity * 0.004,
            decay: s.persistence.max(1e-6).powf(ctx.dt),
            _pad: 0.0,
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            point_count: s.count.clamp(256, PARTICLES),
            _pad2: [0; 3],
            chroma,
            time: ctx.time,
            dt: ctx.dt.min(0.05),
            speed: s.speed,
            field_scale: s.scale * (1.0 + 0.3 * (f.bass_att - 1.0).clamp(-0.5, 1.0)),
            onset: f.onset_pulse[3].min(1.0),
            bass: f.bass_att.min(2.0),
            key_hue: audioviz_analysis::chroma::pitch_class_hue(f.key_class),
            saturation: s.saturation,
            seed: self.seed,
            vortex: s.vortex,
            curl: s.curl,
            lifetime: 5.0,
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            spread: s.spread,
            _pp: 0.0,
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("ribbons update"), timestamp_writes: None });
            pass.set_pipeline(&self.pipelines.update);
            pass.set_bind_group(0, &self.update_group, &[]);
            pass.dispatch_workgroups(params.point_count.div_ceil(256), 1, 1);
        }

        let prev = self.current;
        let cur = 1 - prev;
        fullscreen_pass(encoder, "ribbons decay", &self.hdr[cur], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipelines.decay, &self.hdr_groups[prev]);
        {
            let mut pass = begin_pass(encoder, "ribbons draw", &self.hdr[cur], wgpu::LoadOp::Load);
            pass.set_pipeline(&self.pipelines.draw);
            pass.set_bind_group(0, &self.draw_group, &[]);
            pass.draw(0..6, 0..params.point_count);
        }
        self.current = cur;
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.hdr[self.current]
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        let mut params = layer.get::<RibbonsParams>(ID);
        let s = &mut params;
        let d = RibbonsParams::default();
        use crate::widgets::{ghost_button, group, hint, slider_def};
        group(ui, "ribbons.field", "Field", true, 5, |ui| {
            slider_def(ui, "Particles", &mut s.count, Some(d.count), 256..=PARTICLES, |v| format!("{v:.0}"), true);
            slider_def(ui, "Speed", &mut s.speed, Some(d.speed), 0.02..=2.0, |v| format!("{v:.2}"), true);
            slider_def(ui, "Field scale", &mut s.scale, Some(d.scale), 0.3..=6.0, |v| format!("{v:.2}"), true);
            slider_def(ui, "Curl", &mut s.curl, Some(d.curl), 0.0..=3.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Chroma vortices", &mut s.vortex, Some(d.vortex), 0.0..=3.0, |v| format!("{v:.2}"), false);
        });
        group(ui, "ribbons.look", "Look", true, 4, |ui| {
            slider_def(ui, "Line width", &mut s.width, Some(d.width), 0.3..=4.0, |v| format!("{v:.1} px"), false);
            slider_def(ui, "Intensity", &mut s.intensity, Some(d.intensity), 0.05..=10.0, |v| format!("{v:.2}"), true);
            slider_def(ui, "Persistence", &mut s.persistence, Some(d.persistence), 0.0001..=0.95, |v| format!("{v:.3} /s"), true);
            slider_def(ui, "Saturation", &mut s.saturation, Some(d.saturation), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Colour spread", &mut s.spread, Some(d.spread), 0.0..=1.0, |v| format!("{v:.2}"), false);
            hint(ui, "Without a palette: pitch class on a circle of fifths, pulled toward the key. With one: the chord sits around the palette's current position, spread this far; cycling moves it.");
        });
        ui.add_space(6.0);
        if ghost_button(ui, "Reset ribbons", false).clicked() {
            *s = Default::default();
        }
        layer.put(ID, &params);
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, tex_layout: &wgpu::BindGroupLayout, update_layout: &wgpu::BindGroupLayout, draw_layout: &wgpu::BindGroupLayout) -> Result<Pipelines, String> {
    scene::guarded(device, || {
        let update_mod = scene::shader(device, shaders, "ribbons_update.wgsl");
        let draw_mod = scene::shader(device, shaders, "ribbons_draw.wgsl");
        let decay_mod = scene::shader(device, shaders, "decay.wgsl");
        let update_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ribbons update pl"), bind_group_layouts: &[Some(update_layout)], immediate_size: 0 });
        let update = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ribbons update"),
            layout: Some(&update_pl),
            module: &update_mod,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Pipelines {
            update,
            draw: scene::pipeline(device, "ribbons draw", draw_layout, &draw_mod, HDR_FORMAT, Some(scene::ADDITIVE)),
            decay: scene::pipeline(device, "ribbons decay", tex_layout, &decay_mod, HDR_FORMAT, None),
        }
    })
}
