//! Scene 1: phosphor XY oscilloscope.
//!
//! Three passes over a ping-pong pair of HDR (Rgba16Float) textures:
//!   1. decay   — new = old * decay           (phosphor persistence)
//!   2. beam    — additive instanced segments  (the electron beam)
//!   3. present — tonemap + tint to the swapchain
//!
//! Beam brightness per segment is inversely proportional to its on-screen
//! length, so a fast-moving beam is dim and a lingering one is bright, as on
//! a real CRT.

use anyhow::anyhow;
use bytemuck::{Pod, Zeroable};

use crate::shaders::ShaderSet;

pub const MAX_POINTS: usize = 32_768;

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
    pub point_count: u32,
    pub _pad: [u32; 3],
}

struct Pipelines {
    decay: wgpu::RenderPipeline,
    beam: wgpu::RenderPipeline,
    present: wgpu::RenderPipeline,
}

pub struct Scope {
    hdr_format: wgpu::TextureFormat,
    out_format: wgpu::TextureFormat,
    textures: [wgpu::Texture; 2],
    views: [wgpu::TextureView; 2],
    current: usize,
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
    points_buf: wgpu::Buffer,
    tex_layout: wgpu::BindGroupLayout,
    beam_layout: wgpu::BindGroupLayout,
    tex_groups: [wgpu::BindGroup; 2],
    beam_group: wgpu::BindGroup,
    pipelines: Pipelines,
    pub last_shader_error: Option<String>,
}

impl Scope {
    pub fn new(device: &wgpu::Device, out_format: wgpu::TextureFormat, width: u32, height: u32, shaders: &ShaderSet) -> anyhow::Result<Self> {
        let hdr_format = wgpu::TextureFormat::Rgba16Float;
        let (textures, views) = make_targets(device, hdr_format, width, height);

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

        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scope tex layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
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

        let tex_groups = make_tex_groups(device, &tex_layout, &views, &sampler, &params_buf);
        let beam_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scope beam group"),
            layout: &beam_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: points_buf.as_entire_binding() },
            ],
        });

        let pipelines = build_pipelines(device, shaders, &tex_layout, &beam_layout, hdr_format, out_format)?;

        Ok(Self {
            hdr_format,
            out_format,
            textures,
            views,
            current: 0,
            sampler,
            params_buf,
            points_buf,
            tex_layout,
            beam_layout,
            tex_groups,
            beam_group,
            pipelines,
            last_shader_error: None,
        })
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let (textures, views) = make_targets(device, self.hdr_format, width, height);
        self.textures = textures;
        self.views = views;
        self.tex_groups = make_tex_groups(device, &self.tex_layout, &self.views, &self.sampler, &self.params_buf);
    }

    /// Rebuild pipelines from (possibly edited) shader sources. On failure the
    /// old pipelines are kept and the error is recorded.
    pub fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        match build_pipelines(device, shaders, &self.tex_layout, &self.beam_layout, self.hdr_format, self.out_format) {
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
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scope decay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.views[cur],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines.decay);
            pass.set_bind_group(0, &self.tex_groups[prev], &[]);
            pass.draw(0..3, 0..1);
        }

        // 2. beam: additive segments into cur
        if n >= 2 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scope beam"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.views[cur],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines.beam);
            pass.set_bind_group(0, &self.beam_group, &[]);
            pass.draw(0..6, 0..(n as u32 - 1));
        }

        // 3. present
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scope present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines.present);
            pass.set_bind_group(0, &self.tex_groups[cur], &[]);
            pass.draw(0..3, 0..1);
        }

        self.current = cur;
    }
}

fn make_targets(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> ([wgpu::Texture; 2], [wgpu::TextureView; 2]) {
    let make = |label: &str| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    };
    let textures = [make("scope hdr A"), make("scope hdr B")];
    let views = [
        textures[0].create_view(&wgpu::TextureViewDescriptor::default()),
        textures[1].create_view(&wgpu::TextureViewDescriptor::default()),
    ];
    (textures, views)
}

fn make_tex_groups(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, views: &[wgpu::TextureView; 2], sampler: &wgpu::Sampler, params: &wgpu::Buffer) -> [wgpu::BindGroup; 2] {
    let make = |view: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scope tex group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: params.as_entire_binding() },
            ],
        })
    };
    [make(&views[0]), make(&views[1])]
}

fn build_pipelines(
    device: &wgpu::Device,
    shaders: &ShaderSet,
    tex_layout: &wgpu::BindGroupLayout,
    beam_layout: &wgpu::BindGroupLayout,
    hdr_format: wgpu::TextureFormat,
    out_format: wgpu::TextureFormat,
) -> anyhow::Result<Pipelines> {
    device.push_error_scope(wgpu::ErrorFilter::Validation);

    let module = |name: &str| {
        device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(name), source: wgpu::ShaderSource::Wgsl(shaders.source(name).into()) })
    };
    let decay_mod = module("decay.wgsl");
    let beam_mod = module("beam.wgsl");
    let present_mod = module("present.wgsl");

    let tex_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("scope tex pl"), bind_group_layouts: &[tex_layout], immediate_size: 0 });
    let beam_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("scope beam pl"), bind_group_layouts: &[beam_layout], immediate_size: 0 });

    let fullscreen = |label: &str, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, format: wgpu::TextureFormat| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState { module, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    };

    let decay = fullscreen("scope decay", &tex_pl, &decay_mod, hdr_format);
    let present = fullscreen("scope present", &tex_pl, &present_mod, out_format);

    let additive = wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    };
    let beam = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("scope beam"),
        layout: Some(&beam_pl),
        vertex: wgpu::VertexState { module: &beam_mod, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: &beam_mod,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format: hdr_format, blend: Some(additive), write_mask: wgpu::ColorWrites::ALL })],
        }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    if let Some(err) = pollster::block_on(device.pop_error_scope()) {
        return Err(anyhow!("{err}"));
    }
    Ok(Pipelines { decay, beam, present })
}
