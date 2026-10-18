//! Crystal: translucent tetrahedral shards tumbling in a group. The group
//! swells with loudness; on each beat one shard eases to a new pose (rotation,
//! scale, opacity, colour); `scatter` flings the shards apart on the bar.
//! Facets are additive glass, edges glow in the palette.
//!
//! Inspired by the Crystal visualizer in Felix Turner's Uberviz
//! (https://uberviz.io, no published licence): the idea only, written fresh.
//! See `docs/INSPIRATION.md`.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};

use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, begin_pass};
use crate::shaders::ShaderSet;
use crate::settings::{Layer, VisualKind};

pub const MAX_SHARDS: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_proj: [[f32; 4]; 4],
    resolution: [f32; 2],
    width: f32,
    time: f32,
    tint: [f32; 4],
    palette_mix: f32,
    palette_shift: f32,
    facets: f32,
    edges: f32,
    palette: crate::color::Table,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct ShardGpu {
    model: [[f32; 4]; 4],
    colour: [f32; 4],
    extra: [f32; 4],
}

/// One shard's pose, present and target.
#[derive(Clone, Copy)]
struct Pose {
    rot: Quat,
    scale: Vec3,
    opacity: f32,
    /// Palette position (also seeds the tint variation).
    hue: f32,
}

struct Shard {
    from: Pose,
    to: Pose,
    /// Tween progress 0..1 (1 = settled).
    t: f32,
    /// Rest position inside the group and the flung-out position.
    rest: Vec3,
    far: Vec3,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
    fn quat(&mut self) -> Quat {
        Quat::from_euler(glam::EulerRot::XYZ, self.range(0.0, std::f32::consts::TAU), self.range(0.0, std::f32::consts::TAU), self.range(0.0, std::f32::consts::TAU))
    }
}

fn random_pose(rng: &mut Rng) -> Pose {
    Pose { rot: rng.quat(), scale: Vec3::new(rng.range(0.5, 3.0), rng.range(0.5, 3.0), rng.range(0.5, 3.0)), opacity: rng.range(0.1, 0.6), hue: rng.next() }
}

fn ease_in_expo(t: f32) -> f32 {
    if t <= 0.0 { 0.0 } else if t >= 1.0 { 1.0 } else { (2.0f32).powf(10.0 * (t - 1.0)) }
}

pub struct Crystal {
    hdr: wgpu::TextureView,
    uniform: wgpu::Buffer,
    shards_buf: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    faces: wgpu::RenderPipeline,
    edges: wgpu::RenderPipeline,
    shards: Vec<Shard>,
    rng: Rng,
    tumble: Vec3,
    /// Group tilt re-rolled every few beats, eased.
    tilt: Quat,
    tilt_to: Quat,
    last_beat: f32,
    beats: u32,
    energy: f32,
    gpu: Vec<ShardGpu>,
}

impl Crystal {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let uniform = scene::uniform_buffer(device, "crystal params", std::mem::size_of::<Uniform>() as u64);
        let shards_buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("crystal shards"), size: (MAX_SHARDS * std::mem::size_of::<ShardGpu>()) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let entry = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("crystal layout"), entries: &[entry(0, wgpu::BufferBindingType::Uniform), entry(1, wgpu::BufferBindingType::Storage { read_only: true })] });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crystal group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: shards_buf.as_entire_binding() }],
        });
        let (faces, edges) = build(device, shaders, &layout)?;
        let mut rng = Rng(0x5EED_C0DE_0000_0001);
        let shards = (0..MAX_SHARDS)
            .map(|_| {
                let p = random_pose(&mut rng);
                Shard { from: p, to: p, t: 1.0, rest: Vec3::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3), rng.range(-0.3, 0.3)), far: Vec3::new(rng.range(-2.0, 2.0), rng.range(-2.0, 2.0), rng.range(-2.0, 2.0)) }
            })
            .collect();
        Ok(Self { hdr: scene::hdr_texture(device, "crystal hdr", width, height), uniform, shards_buf, layout, group, faces, edges, shards, rng, tumble: Vec3::ZERO, tilt: Quat::IDENTITY, tilt_to: Quat::IDENTITY, last_beat: 0.0, beats: 0, energy: 1.0, gpu: vec![ShardGpu::default(); MAX_SHARDS] })
    }
}

impl Scene for Crystal {
    fn kind(&self) -> VisualKind {
        VisualKind::Crystal
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.hdr = scene::hdr_texture(device, "crystal hdr", width, height);
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        let (f, e) = build(device, shaders, &self.layout)?;
        self.faces = f;
        self.edges = e;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let p = &layer.crystal;
        let f = ctx.features;
        let dt = ctx.dt;
        let count = (p.count as usize).clamp(1, MAX_SHARDS);
        // Loudness, smoothed, around 1 at an average passage.
        let energy = ((f.bass_att + f.mid_att + f.treb_att) / 3.0).clamp(0.0, 2.5);
        self.energy += (energy - self.energy) * (1.0 - (-dt / 0.25).exp());
        let onset = f.onset_pulse[3].min(1.0);
        // Beats: one shard eases to a new pose; every fourth, the group tilts anew.
        let beat = f.beat_pulse * f.beat_confidence;
        let hit = beat > self.last_beat + 0.4;
        self.last_beat = beat;
        let beat_len = (60.0 / f.bpm.max(40.0)).clamp(0.2, 2.0);
        if hit && p.react > 0.0 {
            self.beats += 1;
            if self.beats % p.every.max(1) == 0 {
                let i = (self.rng.next() * count as f32) as usize % count;
                let s = &mut self.shards[i];
                s.from = current_pose(s);
                s.to = random_pose(&mut self.rng);
                s.t = 0.0;
            }
            if self.beats % 4 == 0 {
                self.tilt_to = self.rng.quat();
            }
        }
        // Tumble and tilt.
        self.tumble += Vec3::new(dt * p.spin * 0.6, dt * p.spin * 0.15, dt * p.spin * 0.6) * (0.6 + 0.4 * self.energy * p.react.min(1.0));
        self.tilt = self.tilt.slerp(self.tilt_to, 1.0 - (-dt / 0.6).exp()).normalize();
        let group_rot = self.tilt * Quat::from_euler(glam::EulerRot::XYZ, self.tumble.x, self.tumble.y, self.tumble.z);
        let group_scale = p.size * (0.55 + 0.45 * self.energy * p.react.min(1.5) + 0.25 * onset * p.react) * 0.4;
        // Scatter: the shards fly to their far places and back over a bar.
        let bar = ((f.beat_count % 4) as f32 + f.beat_phase.clamp(0.0, 1.0)) / 4.0;
        let fling = p.scatter * (bar * std::f32::consts::PI).sin().abs();

        for (i, s) in self.shards.iter_mut().enumerate().take(count) {
            s.t = (s.t + dt / (2.0 * beat_len)).min(1.0);
            let pose = current_pose(s);
            let pos = s.rest.lerp(s.far, fling);
            let model = Mat4::from_scale_rotation_translation(Vec3::splat(group_scale), group_rot, Vec3::ZERO) * Mat4::from_scale_rotation_translation(pose.scale * Vec3::new(p.stretch, 1.0, 1.0), pose.rot, pos);
            // Without a palette the shards vary the tint a little in brightness.
            let b = 0.7 + 0.6 * pose.hue;
            self.gpu[i] = ShardGpu { model: model.to_cols_array_2d(), colour: [ctx.tint[0] * b, ctx.tint[1] * b, ctx.tint[2] * b, pose.opacity], extra: [1.0, pose.hue, 0.0, 0.0] };
        }
        queue.write_buffer(&self.shards_buf, 0, bytemuck::cast_slice(&self.gpu[..count]));

        let aspect = ctx.width as f32 / ctx.height.max(1) as f32;
        let proj = perspective(45f32.to_radians(), aspect, 0.1, 50.0);
        let view = Mat4::from_translation(Vec3::new(0.0, 0.0, -6.0));
        let u = Uniform {
            view_proj: (proj * view).to_cols_array_2d(),
            resolution: [ctx.width as f32, ctx.height as f32],
            width: p.width,
            time: ctx.time,
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            facets: p.facets,
            edges: p.edges,
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));

        let mut pass = begin_pass(encoder, "crystal", &self.hdr, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_pipeline(&self.faces);
        pass.draw(0..3, 0..(count as u32 * 4));
        pass.set_pipeline(&self.edges);
        pass.draw(0..6, 0..(count as u32 * 6));
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.hdr
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        use crate::widgets::{ghost_button, group, hint, slider_def};
        let p = &mut layer.crystal;
        let d = crate::settings::CrystalParams::default();
        group(ui, "crystal.shards", "Shards", true, 6, |ui| {
            slider_def(ui, "Count", &mut p.count, Some(d.count), 1..=MAX_SHARDS as u32, |v| format!("{v:.0}"), false);
            slider_def(ui, "Size", &mut p.size, Some(d.size), 0.2..=3.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Stretch", &mut p.stretch, Some(d.stretch), 0.3..=4.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Tumble", &mut p.spin, Some(d.spin), 0.0..=4.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Scatter", &mut p.scatter, Some(d.scatter), 0.0..=1.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "New pose every", &mut p.every, Some(d.every), 1..=8, |v| if v < 1.5 { "1 beat".into() } else { format!("{v:.0} beats") }, false);
        });
        group(ui, "crystal.look", "Look", true, 4, |ui| {
            slider_def(ui, "Facets", &mut p.facets, Some(d.facets), 0.0..=3.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Edges", &mut p.edges, Some(d.edges), 0.0..=3.0, |v| format!("{v:.2}"), false);
            slider_def(ui, "Edge width", &mut p.width, Some(d.width), 0.5..=6.0, |v| format!("{v:.1} px"), false);
            slider_def(ui, "Reacts", &mut p.react, Some(d.react), 0.0..=3.0, |v| format!("{v:.2}"), false);
            hint(ui, "Glass shards tumbling as a cluster: loudness swells it, each beat eases one shard into a new pose, scatter flings them apart and back over the bar. After the Crystal in Uberviz.");
        });
        ui.add_space(6.0);
        if ghost_button(ui, "Reset crystal", false).clicked() {
            *p = Default::default();
        }
    }
}

/// Right-handed perspective, camera looking down -z, clip z in 0..1.
fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols_array_2d(&[[f / aspect, 0.0, 0.0, 0.0], [0.0, f, 0.0, 0.0], [0.0, 0.0, far / (near - far), -1.0], [0.0, 0.0, near * far / (near - far), 0.0]])
}

fn current_pose(s: &Shard) -> Pose {
    let k = ease_in_expo(s.t);
    Pose { rot: s.from.rot.slerp(s.to.rot, k), scale: s.from.scale.lerp(s.to.scale, k), opacity: s.from.opacity + (s.to.opacity - s.from.opacity) * k, hue: if s.t >= 0.999 { s.to.hue } else { s.from.hue + (s.to.hue - s.from.hue) * k } }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<(wgpu::RenderPipeline, wgpu::RenderPipeline), String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "crystal.wgsl");
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("crystal"), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
        let make = |label: &str, vs: &str, fs: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &m, entry_point: Some(vs), buffers: &[], compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState { module: &m, entry_point: Some(fs), targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(scene::ADDITIVE), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        (make("crystal facets", "vs_main", "fs_main"), make("crystal edges", "vs_edge", "fs_edge"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_layout_matches_wgsl() {
        assert_eq!(std::mem::offset_of!(Uniform, resolution), 64);
        assert_eq!(std::mem::offset_of!(Uniform, tint), 80);
        assert_eq!(std::mem::offset_of!(Uniform, palette), 112);
        assert_eq!(std::mem::size_of::<ShardGpu>(), 96);
    }

    #[test]
    fn tween_settles() {
        let mut rng = Rng(7);
        let (a, b) = (random_pose(&mut rng), random_pose(&mut rng));
        let s = Shard { from: a, to: b, t: 1.0, rest: Vec3::ZERO, far: Vec3::ZERO };
        let p = current_pose(&s);
        assert_eq!(p.scale, b.scale);
        assert_eq!(p.hue, b.hue);
    }
}
