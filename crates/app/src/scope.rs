//! Scene 1: phosphor XY oscilloscope.
//!
//! Passes over a ping-pong pair of HDR (Rgba16Float) textures:
//!   1. decay      — new = old * decay                (phosphor persistence)
//!   2. beam       — additive instanced segments      (the electron beam)
//!   3. bloom down — dual-filter chain to 1/64 res
//!   4. bloom up   — tent upsample, accumulated additively back to 1/2 res
//!   5. present    — tint + bloom, AgX tonemap, grain, dither, to the swapchain
//!
//! Beam brightness per segment is inversely proportional to its on-screen
//! length, so a fast-moving beam is dim and a lingering one is bright, as on
//! a real CRT.

use anyhow::anyhow;
use bytemuck::{Pod, Zeroable};

use crate::shaders::ShaderSet;

pub const MAX_POINTS: usize = 32_768;
const BLOOM_LEVELS: usize = 6;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Params {
    pub resolution: [f32; 2],
    pub gain: f32,
    pub rotation: f32,
    pub beam_width: f32,
    pub beam_intensity: f32,
    pub decay: f32,
    pub exposure: f32,
    pub tint: [f32; 4],
    pub bloom: f32,
    pub grain: f32,
    pub time: f32,
    pub dt: f32,
    pub point_count: u32,
    /// bit0: auto exposure
    pub flags: u32,
    pub _pad: [u32; 2],
}

struct Pipelines {
    decay: wgpu::RenderPipeline,
    beam: wgpu::RenderPipeline,
    bloom_down: wgpu::RenderPipeline,
    bloom_up: wgpu::RenderPipeline,
    expo: wgpu::RenderPipeline,
    present: wgpu::RenderPipeline,
}

pub struct Scope {
    hdr_format: wgpu::TextureFormat,
    out_format: wgpu::TextureFormat,
    hdr: [wgpu::TextureView; 2],
    current: usize,
    bloom: Vec<wgpu::TextureView>,
    expo: [wgpu::TextureView; 2],
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
    points_buf: wgpu::Buffer,
    tex_layout: wgpu::BindGroupLayout,
    beam_layout: wgpu::BindGroupLayout,
    present_layout: wgpu::BindGroupLayout,
    hdr_groups: [wgpu::BindGroup; 2],
    bloom_groups: Vec<wgpu::BindGroup>,
    present_groups: [wgpu::BindGroup; 2],
    expo_groups: [wgpu::BindGroup; 2],
    beam_group: wgpu::BindGroup,
    pipelines: Pipelines,
    pub last_shader_error: Option<String>,
}

impl Scope {
    pub fn new(device: &wgpu::Device, out_format: wgpu::TextureFormat, width: u32, height: u32, shaders: &ShaderSet) -> anyhow::Result<Self> {
        let hdr_format = wgpu::TextureFormat::Rgba16Float;

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scope sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scope params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let points_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scope points"),
            size: (MAX_POINTS * 8) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let tex_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let sampler_entry = wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None };
        let uniform_entry = |binding: u32, visibility: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };

        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scope tex layout"),
            entries: &[tex_entry(0), sampler_entry, uniform_entry(2, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let present_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scope present layout"),
            entries: &[tex_entry(0), sampler_entry, uniform_entry(2, wgpu::ShaderStages::VERTEX_FRAGMENT), tex_entry(3), tex_entry(4)],
        });
        let beam_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scope beam layout"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
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

        let pipelines = build_pipelines(device, shaders, &tex_layout, &beam_layout, &present_layout, hdr_format, out_format)?;

        let mut scope = Self {
            hdr_format,
            out_format,
            hdr: make_hdr(device, hdr_format, width, height),
            current: 0,
            bloom: Vec::new(),
            expo: [make_texture(device, "expo A", hdr_format, 1, 1), make_texture(device, "expo B", hdr_format, 1, 1)],
            sampler,
            params_buf,
            points_buf,
            tex_layout,
            beam_layout,
            present_layout,
            // Placeholders, replaced by rebuild_targets below.
            hdr_groups: [dummy_group(device, &tex_layout_clone(device), &params_buf_clone(device)), dummy_group(device, &tex_layout_clone(device), &params_buf_clone(device))],
            bloom_groups: Vec::new(),
            present_groups: [dummy_group(device, &tex_layout_clone(device), &params_buf_clone(device)), dummy_group(device, &tex_layout_clone(device), &params_buf_clone(device))],
            expo_groups: [dummy_group(device, &tex_layout_clone(device), &params_buf_clone(device)), dummy_group(device, &tex_layout_clone(device), &params_buf_clone(device))],
            beam_group,
            pipelines,
            last_shader_error: None,
        };
        scope.rebuild_targets(device, width, height);
        Ok(scope)
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.rebuild_targets(device, width, height);
    }

    fn rebuild_targets(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.hdr = make_hdr(device, self.hdr_format, width, height);
        self.bloom = make_bloom(device, self.hdr_format, width, height);
        let tex_group = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scope tex group"),
                layout: &self.tex_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: self.params_buf.as_entire_binding() },
                ],
            })
        };
        self.hdr_groups = [tex_group(&self.hdr[0]), tex_group(&self.hdr[1])];
        self.bloom_groups = self.bloom.iter().map(tex_group).collect();
        let five = |label: &str, main: &wgpu::TextureView, aux_a: &wgpu::TextureView, aux_b: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &self.present_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(main) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: self.params_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(aux_a) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(aux_b) },
                ],
            })
        };
        let smallest = &self.bloom[self.bloom.len() - 1];
        // Present reads hdr[i], bloom[0] and the *current* exposure state; the
        // exposure pass reads the smallest bloom level and the *previous* state.
        self.present_groups = [five("present A", &self.hdr[0], &self.bloom[0], &self.expo[0]), five("present B", &self.hdr[1], &self.bloom[0], &self.expo[1])];
        self.expo_groups = [five("expo from A", smallest, &self.expo[0], &self.expo[0]), five("expo from B", smallest, &self.expo[1], &self.expo[1])];
    }

    /// Rebuild pipelines from (possibly edited) shader sources. On failure the
    /// old pipelines are kept and the error is recorded.
    pub fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        match build_pipelines(device, shaders, &self.tex_layout, &self.beam_layout, &self.present_layout, self.hdr_format, self.out_format) {
            Ok(p) => {
                self.pipelines = p;
                self.last_shader_error = None;
                tracing::info!("shaders reloaded");
            }
            Err(e) => {
                tracing::warn!("shader reload failed: {e}");
                self.last_shader_error = Some(e.to_string());
            }
        }
    }

    /// `points` are raw (L, R) pairs in -1..1; gain/rotation are applied on the GPU.
    pub fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, mut params: Params, points: &[[f32; 2]]) {
        let n = points.len().min(MAX_POINTS);
        params.point_count = n as u32;
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        if n > 0 {
            queue.write_buffer(&self.points_buf, 0, bytemuck::cast_slice(&points[..n]));
        }

        let prev = self.current;
        let cur = 1 - prev;

        // 1. decay: cur = prev * decay
        fullscreen_pass(encoder, "scope decay", &self.hdr[cur], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipelines.decay, &self.hdr_groups[prev]);

        // 2. beam: additive segments into cur
        if n >= 2 {
            let mut pass = begin_pass(encoder, "scope beam", &self.hdr[cur], wgpu::LoadOp::Load);
            pass.set_pipeline(&self.pipelines.beam);
            pass.set_bind_group(0, &self.beam_group, &[]);
            pass.draw(0..6, 0..(n as u32 - 1));
        }

        // 3. bloom down chain
        for j in 0..self.bloom.len() {
            let input = if j == 0 { &self.hdr_groups[cur] } else { &self.bloom_groups[j - 1] };
            fullscreen_pass(encoder, "bloom down", &self.bloom[j], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipelines.bloom_down, input);
        }
        // 4. bloom up chain (additive)
        for j in (1..self.bloom.len()).rev() {
            fullscreen_pass(encoder, "bloom up", &self.bloom[j - 1], wgpu::LoadOp::Load, &self.pipelines.bloom_up, &self.bloom_groups[j]);
        }

        // 5. auto exposure state: expo[cur] from expo[prev] + mean of the smallest level
        fullscreen_pass(encoder, "auto exposure", &self.expo[cur], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipelines.expo, &self.expo_groups[prev]);

        // 6. present
        fullscreen_pass(encoder, "scope present", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipelines.present, &self.present_groups[cur]);

        self.current = cur;
    }
}

fn begin_pass<'a>(encoder: &'a mut wgpu::CommandEncoder, label: &str, view: &'a wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

fn fullscreen_pass(encoder: &mut wgpu::CommandEncoder, label: &str, view: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup) {
    let mut pass = begin_pass(encoder, label, view, load);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..3, 0..1);
}

fn make_texture(device: &wgpu::Device, label: &str, format: wgpu::TextureFormat, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn make_hdr(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> [wgpu::TextureView; 2] {
    [make_texture(device, "scope hdr A", format, width, height), make_texture(device, "scope hdr B", format, width, height)]
}

fn make_bloom(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Vec<wgpu::TextureView> {
    let (mut w, mut h) = (width, height);
    let mut out = Vec::with_capacity(BLOOM_LEVELS);
    for i in 0..BLOOM_LEVELS {
        w = (w / 2).max(1);
        h = (h / 2).max(1);
        out.push(make_texture(device, &format!("bloom {i}"), format, w, h));
        if w == 1 && h == 1 {
            break;
        }
    }
    out
}

// Tiny helpers so `Scope::new` can build a struct before targets exist.
fn tex_layout_clone(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("placeholder"), entries: &[] })
}
fn params_buf_clone(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some("placeholder"), size: 16, usage: wgpu::BufferUsages::UNIFORM, mapped_at_creation: false })
}
fn dummy_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, _buf: &wgpu::Buffer) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("placeholder"), layout, entries: &[] })
}

fn build_pipelines(
    device: &wgpu::Device,
    shaders: &ShaderSet,
    tex_layout: &wgpu::BindGroupLayout,
    beam_layout: &wgpu::BindGroupLayout,
    present_layout: &wgpu::BindGroupLayout,
    hdr_format: wgpu::TextureFormat,
    out_format: wgpu::TextureFormat,
) -> anyhow::Result<Pipelines> {
    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

    let module = |name: &str| device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(name), source: wgpu::ShaderSource::Wgsl(shaders.source(name).into()) });
    let decay_mod = module("decay.wgsl");
    let beam_mod = module("beam.wgsl");
    let down_mod = module("bloom_down.wgsl");
    let up_mod = module("bloom_up.wgsl");
    let expo_mod = module("expo.wgsl");
    let present_mod = module("present.wgsl");

    let layout = |label: &str, bgl: &wgpu::BindGroupLayout| device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(bgl)], immediate_size: 0 });
    let tex_pl = layout("scope tex pl", tex_layout);
    let beam_pl = layout("scope beam pl", beam_layout);
    let present_pl = layout("scope present pl", present_layout);

    let additive = wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    };

    let pipeline = |label: &str, pl: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(pl),
            vertex: wgpu::VertexState { module, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    };

    let pipelines = Pipelines {
        decay: pipeline("scope decay", &tex_pl, &decay_mod, hdr_format, None),
        beam: pipeline("scope beam", &beam_pl, &beam_mod, hdr_format, Some(additive)),
        bloom_down: pipeline("bloom down", &tex_pl, &down_mod, hdr_format, None),
        bloom_up: pipeline("bloom up", &tex_pl, &up_mod, hdr_format, Some(additive)),
        expo: pipeline("auto exposure", &present_pl, &expo_mod, hdr_format, None),
        present: pipeline("scope present", &present_pl, &present_mod, out_format, None),
    };

    if let Some(err) = pollster::block_on(error_scope.pop()) {
        return Err(anyhow!("{err}"));
    }
    Ok(pipelines)
}
