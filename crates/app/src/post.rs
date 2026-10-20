//! Post stack shared by all scenes: bloom chain, auto-exposure, AgX tonemap,
//! grain and dither. Input is the compositor's HDR color texture.

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, fullscreen_pass, hdr_texture, HDR_FORMAT};
use crate::shaders::ShaderSet;

const BLOOM_LEVELS: usize = 6;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct PostParams {
    pub exposure: f32,
    pub bloom: f32,
    pub grain: f32,
    pub time: f32,
    pub dt: f32,
    /// bit0 (1): auto exposure.
    /// bit1 (2): reduce flashing (photosensitivity): cap how fast the displayed
    /// mean luminance may rise frame to frame; see `flash_rise` and expo.wgsl.
    pub flags: u32,
    /// Reduce flashing: the displayed mean luminance may grow by at most a
    /// factor of `1 + flash_rise` per second (2.0 = at most 3x per second),
    /// applied per frame as `pow(1 + flash_rise, dt)`. Decreases are never
    /// limited. Only read when `flags` bit1 is set.
    pub flash_rise: f32,
    /// How far the finished picture is dimmed toward black, 0..1 (0: not
    /// at all): the screen fading up when a scene first arrives.
    pub dark: f32,
    /// Saturation before the tonemapper.
    pub saturation: f32,
    /// 0 = film (AgX), 1 = neon (hue-preserving).
    pub look: u32,
    pub _pad1: [u32; 2],
}

struct Pipelines {
    bloom_down: wgpu::RenderPipeline,
    bloom_up: wgpu::RenderPipeline,
    expo: wgpu::RenderPipeline,
    present: wgpu::RenderPipeline,
}

pub struct Post {
    out_format: wgpu::TextureFormat,
    /// Scenes composite into this; post reads it.
    input: wgpu::TextureView,
    bloom: Vec<wgpu::TextureView>,
    expo: [wgpu::TextureView; 2],
    expo_cur: usize,
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
    layout1: wgpu::BindGroupLayout,
    layout3: wgpu::BindGroupLayout,
    input_group: wgpu::BindGroup,
    bloom_groups: Vec<wgpu::BindGroup>,
    present_groups: [wgpu::BindGroup; 2],
    expo_groups: [wgpu::BindGroup; 2],
    pipelines: Pipelines,
}

impl Post {
    pub fn new(device: &wgpu::Device, out_format: wgpu::TextureFormat, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let sampler = scene::linear_sampler(device);
        let params_buf = scene::uniform_buffer(device, "post params", std::mem::size_of::<PostParams>() as u64);
        let layout1 = scene::tex_layout(device, "post tex1", 0);
        let layout3 = scene::tex_layout(device, "post tex3", 2);
        let pipelines = build(device, shaders, &layout1, &layout3, out_format)?;

        let input = hdr_texture(device, "composite", width, height);
        let bloom = make_bloom(device, width, height);
        let expo = [hdr_texture(device, "expo A", 1, 1), hdr_texture(device, "expo B", 1, 1)];
        let input_group = scene::tex_group(device, &layout1, &[&input], &sampler, &params_buf);
        let bloom_groups = bloom.iter().map(|v| scene::tex_group(device, &layout1, &[v], &sampler, &params_buf)).collect();
        let smallest = &bloom[bloom.len() - 1];
        let present_groups = [
            scene::tex_group(device, &layout3, &[&input, &bloom[0], &expo[0]], &sampler, &params_buf),
            scene::tex_group(device, &layout3, &[&input, &bloom[0], &expo[1]], &sampler, &params_buf),
        ];
        let expo_groups = [
            scene::tex_group(device, &layout3, &[smallest, &expo[0], &expo[0]], &sampler, &params_buf),
            scene::tex_group(device, &layout3, &[smallest, &expo[1], &expo[1]], &sampler, &params_buf),
        ];
        Ok(Self { out_format, input, bloom, expo, expo_cur: 0, sampler, params_buf, layout1, layout3, input_group, bloom_groups, present_groups, expo_groups, pipelines })
    }

    pub fn input(&self) -> &wgpu::TextureView {
        &self.input
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.input = hdr_texture(device, "composite", width, height);
        self.bloom = make_bloom(device, width, height);
        self.input_group = scene::tex_group(device, &self.layout1, &[&self.input], &self.sampler, &self.params_buf);
        self.bloom_groups = self.bloom.iter().map(|v| scene::tex_group(device, &self.layout1, &[v], &self.sampler, &self.params_buf)).collect();
        let smallest = &self.bloom[self.bloom.len() - 1];
        self.present_groups = [
            scene::tex_group(device, &self.layout3, &[&self.input, &self.bloom[0], &self.expo[0]], &self.sampler, &self.params_buf),
            scene::tex_group(device, &self.layout3, &[&self.input, &self.bloom[0], &self.expo[1]], &self.sampler, &self.params_buf),
        ];
        self.expo_groups = [
            scene::tex_group(device, &self.layout3, &[smallest, &self.expo[0], &self.expo[0]], &self.sampler, &self.params_buf),
            scene::tex_group(device, &self.layout3, &[smallest, &self.expo[1], &self.expo[1]], &self.sampler, &self.params_buf),
        ];
    }

    pub fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipelines = build(device, shaders, &self.layout1, &self.layout3, self.out_format)?;
        Ok(())
    }

    /// The auto exposure state as of the last frame (expo.wgsl).
    pub fn exposure(&self) -> &wgpu::TextureView {
        &self.expo[self.expo_cur]
    }

    pub fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, params: PostParams) {
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        let prev = self.expo_cur;
        let cur = 1 - prev;

        for j in 0..self.bloom.len() {
            let input = if j == 0 { &self.input_group } else { &self.bloom_groups[j - 1] };
            fullscreen_pass(encoder, "bloom down", &self.bloom[j], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipelines.bloom_down, input);
        }
        for j in (1..self.bloom.len()).rev() {
            fullscreen_pass(encoder, "bloom up", &self.bloom[j - 1], wgpu::LoadOp::Load, &self.pipelines.bloom_up, &self.bloom_groups[j]);
        }
        fullscreen_pass(encoder, "auto exposure", &self.expo[cur], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipelines.expo, &self.expo_groups[prev]);
        fullscreen_pass(encoder, "present", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipelines.present, &self.present_groups[cur]);
        self.expo_cur = cur;
    }
}

fn make_bloom(device: &wgpu::Device, width: u32, height: u32) -> Vec<wgpu::TextureView> {
    let (mut w, mut h) = (width, height);
    let mut out = Vec::with_capacity(BLOOM_LEVELS);
    for i in 0..BLOOM_LEVELS {
        w = (w / 2).max(1);
        h = (h / 2).max(1);
        out.push(hdr_texture(device, &format!("bloom {i}"), w, h));
        if w == 1 && h == 1 {
            break;
        }
    }
    out
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout1: &wgpu::BindGroupLayout, layout3: &wgpu::BindGroupLayout, out_format: wgpu::TextureFormat) -> Result<Pipelines, String> {
    scene::guarded(device, || {
        let down = scene::shader(device, shaders, "bloom_down.wgsl");
        let up = scene::shader(device, shaders, "bloom_up.wgsl");
        let expo = scene::shader(device, shaders, "expo.wgsl");
        let present = scene::shader(device, shaders, "present.wgsl");
        Pipelines {
            bloom_down: scene::pipeline(device, "bloom down", layout1, &down, HDR_FORMAT, None),
            bloom_up: scene::pipeline(device, "bloom up", layout1, &up, HDR_FORMAT, Some(scene::ADDITIVE)),
            expo: scene::pipeline(device, "auto exposure", layout3, &expo, HDR_FORMAT, None),
            present: scene::pipeline(device, "present", layout3, &present, out_format, None),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::PostParams;
    use std::mem::{offset_of, size_of};

    /// `PostParams` in shaders/params.wgsl mirrors this struct byte for byte;
    /// these are the offsets the WGSL side relies on.
    #[test]
    fn post_params_layout() {
        assert_eq!(size_of::<PostParams>(), 48);
        assert_eq!(offset_of!(PostParams, saturation), 32);
        assert_eq!(size_of::<PostParams>() % 16, 0);
        assert_eq!(offset_of!(PostParams, exposure), 0);
        assert_eq!(offset_of!(PostParams, bloom), 4);
        assert_eq!(offset_of!(PostParams, grain), 8);
        assert_eq!(offset_of!(PostParams, time), 12);
        assert_eq!(offset_of!(PostParams, dt), 16);
        assert_eq!(offset_of!(PostParams, flags), 20);
        assert_eq!(offset_of!(PostParams, flash_rise), 24);
        assert_eq!(offset_of!(PostParams, dark), 28);
    }
}
