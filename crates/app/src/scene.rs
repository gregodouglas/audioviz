//! Scene abstraction. A scene renders *linear HDR color* into its own texture;
//! the director cross-fades scenes and the post stack (bloom, exposure, AgX)
//! turns the result into pixels.

use audioviz_analysis::FeatureFrame;

use crate::shaders::ShaderSet;
use crate::settings::Settings;

pub struct FrameCtx<'a> {
    pub features: &'a FeatureFrame,
    pub settings: &'a Settings,
    pub dt: f32,
    pub time: f32,
    pub width: u32,
    pub height: u32,
    /// Raw (L, R) samples since the last frame, newest last (for the scope).
    pub points: &'a [[f32; 2]],
    /// Scene-level auto gain (0.9 / recent peak), smoothed.
    pub auto_gain: f32,
}

pub trait Scene {
    fn name(&self) -> &'static str;
    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32);
    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String>;
    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx);
    /// The scene's HDR color output for this frame.
    fn output(&self) -> &wgpu::TextureView;
    /// Scene-specific controls in the panel.
    fn ui(&mut self, _ui: &mut egui::Ui, _settings: &mut Settings) {}
}

/// Helpers shared by scenes.
pub fn hdr_texture(device: &wgpu::Device, label: &str, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub fn linear_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("linear clamp"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

/// Layout: 0 texture, 1 sampler, 2 uniform (VERTEX_FRAGMENT), then `extra`
/// more textures at bindings 3.. .
pub fn tex_layout(device: &wgpu::Device, label: &str, extra_textures: u32) -> wgpu::BindGroupLayout {
    let tex = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        count: None,
    };
    let mut entries = vec![
        tex(0),
        wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        },
    ];
    for i in 0..extra_textures {
        entries.push(tex(3 + i));
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries: &entries })
}

pub fn tex_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, textures: &[&wgpu::TextureView], sampler: &wgpu::Sampler, uniform: &wgpu::Buffer) -> wgpu::BindGroup {
    let mut entries = vec![
        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(textures[0]) },
        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
    ];
    for (i, t) in textures.iter().enumerate().skip(1) {
        entries.push(wgpu::BindGroupEntry { binding: 2 + i as u32, resource: wgpu::BindingResource::TextureView(t) });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("tex group"), layout, entries: &entries })
}

pub fn uniform_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

pub fn begin_pass<'a>(encoder: &'a mut wgpu::CommandEncoder, label: &str, view: &'a wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>) -> wgpu::RenderPass<'a> {
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

pub fn fullscreen_pass(encoder: &mut wgpu::CommandEncoder, label: &str, view: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup) {
    let mut pass = begin_pass(encoder, label, view, load);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..3, 0..1);
}

pub const ADDITIVE: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
};

/// Build a render pipeline with a fullscreen-style vertex shader (`vs_main`)
/// and `fs_main`, no vertex buffers.
pub fn pipeline(device: &wgpu::Device, label: &str, layout: &wgpu::BindGroupLayout, module: &wgpu::ShaderModule, format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
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
}

pub fn shader(device: &wgpu::Device, shaders: &ShaderSet, name: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(name), source: wgpu::ShaderSource::Wgsl(shaders.source(name).into()) })
}

/// Run `build` inside a validation error scope; returns the error text instead
/// of panicking so a broken shader keeps the last good pipeline.
pub fn guarded<T>(device: &wgpu::Device, build: impl FnOnce() -> T) -> Result<T, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let out = build();
    match pollster::block_on(scope.pop()) {
        Some(err) => Err(err.to_string()),
        None => Ok(out),
    }
}
