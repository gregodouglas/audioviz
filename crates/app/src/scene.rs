//! Visual abstraction. A visual renders *linear HDR color* into its own
//! texture from a `Layer`'s parameters; the director adds the enabled layers
//! together and the post stack (bloom, exposure, AgX) turns the result into
//! pixels.

use audioviz_analysis::FeatureFrame;
use audioviz_metadata::ArtImage;

use crate::settings::Layer;
use crate::shaders::ShaderSet;

pub struct FrameCtx<'a> {
    /// Features for this layer (already band-filtered when the layer asks).
    pub features: &'a FeatureFrame,
    /// Base colour, linear 0..1.
    pub tint: [f32; 3],
    pub dt: f32,
    pub time: f32,
    /// Sample rate of `points`, Hz.
    pub sample_rate: u32,
    pub width: u32,
    pub height: u32,
    /// (L, R) samples since the last frame, newest last (band-passed for the layer).
    pub points: &'a [[f32; 2]],
    /// Scope auto gain (0.9 / recent peak), smoothed.
    pub auto_gain: f32,
    /// Album art of the playing track, and an id that changes with it.
    pub art: Option<&'a ArtImage>,
    pub art_id: u64,
    /// Folder for the kaleidoscope's photo slideshow (may be empty).
    pub photos_folder: &'a str,
    /// The layer's baked palette when one is chosen, with its current shift
    /// (0..1 along the table) and how far the visual should lean on it.
    pub palette: Option<&'a crate::color::Table>,
    pub palette_shift: f32,
    pub palette_amount: f32,
}

pub trait Scene {
    /// Registry id (`visuals::get`).
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str {
        crate::visuals::get(self.id()).map_or("visual", |d| d.name)
    }
    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32);
    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String>;
    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer);
    /// The visual's HDR color output for this frame.
    fn output(&self) -> &wgpu::TextureView;
    /// Called every frame with the device: a visual loaded from a folder
    /// checks its files here and recompiles when they changed.
    fn poll(&mut self, _device: &wgpu::Device, _shaders: &ShaderSet) {}
    /// The layer's settings, every frame before `poll` (a visual with
    /// variants picks its program from its values here, before it draws).
    fn values(&mut self, _layer: &Layer) {}
    /// Whether it can draw yet (a loaded visual's pipelines compile on a
    /// worker thread; the director holds its layer dark until then).
    fn ready(&self) -> bool {
        true
    }
    /// Whether the layer's levelling should measure only the bright part of
    /// the picture, so faint fill across the frame (plankton, glow in the
    /// water) does not count as lit and lift the bright things.
    fn level_bright(&self) -> bool {
        false
    }
    /// A shared space (ADR-0002): whether this visual can join one now (it
    /// asks to, and its pipelines for one are built).
    fn joins_space(&self) -> bool {
        false
    }
    /// Whether it says where the space's camera is (`fn camera`).
    fn leads_space(&self) -> bool {
        false
    }
    /// Its frame in the space, before anything draws: uniforms (with the
    /// layer's `opacity`, which it applies inside its draws), prepare step,
    /// line points.
    fn space_update(&mut self, _queue: &wgpu::Queue, _encoder: &mut wgpu::CommandEncoder, _ctx: &FrameCtx, _layer: &Layer, _opacity: f32) {}
    /// Its shader's error, when the code it was given does not build (it
    /// keeps drawing the last version that did, if any).
    fn error(&self) -> Option<&str> {
        None
    }
    /// Whether it writes light that diffuses through the space (`write_glow`).
    fn space_glows(&self) -> bool {
        false
    }
    /// Hand its camera to the space (the leading layer, after its update).
    fn space_camera(&self, _encoder: &mut wgpu::CommandEncoder) {}
    /// Draw into the space's pass.
    fn space_draw(&self, _pass: &mut wgpu::RenderPass<'_>) {}
    /// Where the director writes what its light goes through on the way
    /// to the screen this frame (layer_seen.wgsl), if it reads that.
    fn seen(&self) -> Option<&wgpu::TextureView> {
        None
    }
    /// Visual-specific controls in the panel.
    fn ui(&mut self, _ui: &mut egui::Ui, _layer: &mut Layer) {}
}

/// Helpers shared by visuals.
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

/// An HDR texture with a mip chain: one view per level to render into and a
/// view of the whole chain to sample with an explicit level.
pub struct Pyramid {
    pub levels: Vec<wgpu::TextureView>,
    pub all: wgpu::TextureView,
}

pub fn hdr_pyramid(device: &wgpu::Device, label: &str, width: u32, height: u32, max_levels: u32) -> Pyramid {
    let (width, height) = (width.max(1), height.max(1));
    let levels = (width.min(height).ilog2() + 1).min(max_levels).max(1);
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let level = |i: u32| tex.create_view(&wgpu::TextureViewDescriptor { base_mip_level: i, mip_level_count: Some(1), ..Default::default() });
    Pyramid { levels: (0..levels).map(level).collect(), all: tex.create_view(&wgpu::TextureViewDescriptor::default()) }
}

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
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout, entries: &entries })
}

pub fn uniform_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

pub fn begin_pass<'a>(encoder: &'a mut wgpu::CommandEncoder, label: &str, view: &'a wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load, store: wgpu::StoreOp::Store } })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// A pass that only clears `view` to black.
pub fn clear_pass(encoder: &mut wgpu::CommandEncoder, label: &str, view: &wgpu::TextureView) {
    let _ = begin_pass(encoder, label, view, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
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

/// A fullscreen-triangle pipeline (vs_main / fs_main) writing `format`.
pub fn pipeline(device: &wgpu::Device, label: &str, layout: &wgpu::BindGroupLayout, module: &wgpu::ShaderModule, format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>) -> wgpu::RenderPipeline {
    pipeline_samples(device, label, layout, module, format, blend, 1)
}

/// `pipeline` drawing into a target with `samples` samples per pixel.
pub fn pipeline_samples(device: &wgpu::Device, label: &str, layout: &wgpu::BindGroupLayout, module: &wgpu::ShaderModule, format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>, samples: u32) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
        vertex: wgpu::VertexState { module, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module, entry_point: Some("fs_main"), targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        multiview_mask: None,
        cache: None,
    })
}

/// A fullscreen-triangle pipeline writing two colour targets of `format`
/// (location 0 and 1), for passes that keep a state texture beside their picture.
pub fn pipeline2(device: &wgpu::Device, label: &str, layout: &wgpu::BindGroupLayout, module: &wgpu::ShaderModule, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
    let target = Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
        vertex: wgpu::VertexState { module, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module, entry_point: Some("fs_main"), targets: &[target.clone(), target], compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Fullscreen pass into two targets (both cleared to black).
pub fn fullscreen_pass2(encoder: &mut wgpu::CommandEncoder, label: &str, a: &wgpu::TextureView, b: &wgpu::TextureView, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup) {
    let att = |view| Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } });
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: Some(label), color_attachments: &[att(a), att(b)], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..3, 0..1);
}

pub fn shader(device: &wgpu::Device, shaders: &ShaderSet, name: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(name), source: wgpu::ShaderSource::Wgsl(shaders.source(name).into()) })
}

/// Build pipelines inside validation and internal error scopes so a broken
/// shader returns an error string instead of panicking (hot reload keeps the
/// old pipeline). Internal covers the backend's own compiler refusing code
/// naga accepted (D3D's FXC on some array writes).
pub fn guarded<T>(device: &wgpu::Device, build: impl FnOnce() -> T) -> Result<T, String> {
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let out = build();
    let v = pollster::block_on(validation.pop());
    let i = pollster::block_on(internal.pop());
    match v.or(i) {
        None => Ok(out),
        Some(e) => Err(e.to_string()),
    }
}
