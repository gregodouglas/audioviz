//! Owns the layer instances (one visual per layer slot), renders the enabled
//! ones with their own parameters and audio band, and adds them into the
//! composite. Scene switches dissolve the previous picture into the new one
//! (a frozen snapshot, cheap). In auto mode it raises cues (track change,
//! drops, long quiet, dwell) and the app picks the next scene.

use std::time::Instant;

use audioviz_analysis::FeatureFrame;
use bytemuck::{Pod, Zeroable};

use crate::bandfilter::{BandPass, band_features};
use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, fullscreen_pass, hdr_texture};
use crate::scenes::ribbons::Ribbons;
use crate::scenes::scope::Scope;
use crate::scenes::starfield::Starfield;
use crate::settings::{MAX_LAYERS, Settings, VisualKind};
use crate::shaders::ShaderSet;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct LayerParams {
    opacity: f32,
    _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct MixParams {
    t: f32,
    _pad: [f32; 3],
}

/// Manual scene switches dissolve fast; the auto director takes its time.
pub const FADE_MANUAL_S: f32 = 0.5;
pub const FADE_AUTO_S: f32 = 4.0;
const MIN_DWELL_S: f32 = 90.0;
const MAX_DWELL_S: f32 = 360.0;
const QUIET_FOR_S: f32 = 6.0;

pub struct Director {
    pub layers: Vec<Box<dyn Scene>>,
    filters: Vec<BandPass>,
    layer_bufs: Vec<wgpu::Buffer>,
    sampler: wgpu::Sampler,
    layer_layout: wgpu::BindGroupLayout,
    add_pipeline: wgpu::RenderPipeline,
    copy_pipeline: wgpu::RenderPipeline,
    mix_layout: wgpu::BindGroupLayout,
    mix_pipeline: wgpu::RenderPipeline,
    mix_buf: wgpu::Buffer,
    one_buf: wgpu::Buffer,
    composite: wgpu::TextureView,
    prev: wgpu::TextureView,
    width: u32,
    height: u32,
    /// (progress 0..1, seconds)
    dissolve: Option<(f32, f32)>,
    pending_dissolve: Option<f32>,
    last_switch: Instant,
    // Auto-mode state.
    last_track: Option<u64>,
    quiet_for: f32,
    trend_peak: f32,
    pub last_cue: &'static str,
    pub last_error: Option<String>,
}

fn make(kind: VisualKind, device: &wgpu::Device, w: u32, h: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
    Ok(match kind {
        VisualKind::Scope => Box::new(Scope::new(device, w, h, shaders)?),
        VisualKind::Ribbons => Box::new(Ribbons::new(device, w, h, shaders)?),
        VisualKind::Starfield => Box::new(Starfield::new(device, w, h, shaders)?),
    })
}

impl Director {
    pub fn new(device: &wgpu::Device, shaders: &ShaderSet, width: u32, height: u32, settings: &Settings) -> Result<Self, String> {
        let sampler = scene::linear_sampler(device);
        let layer_layout = scene::tex_layout(device, "layer", 0);
        let (add_pipeline, copy_pipeline) = build_layer(device, shaders, &layer_layout)?;
        let mix_layout = scene::tex_layout(device, "mix", 1);
        let mix_pipeline = build_mix(device, shaders, &mix_layout)?;
        let mix_buf = scene::uniform_buffer(device, "mix params", std::mem::size_of::<MixParams>() as u64);
        let one_buf = scene::uniform_buffer(device, "layer one", std::mem::size_of::<LayerParams>() as u64);
        let mut layers = Vec::new();
        for i in 0..MAX_LAYERS {
            layers.push(make(settings.layer(i).visual, device, width, height, shaders)?);
        }
        let filters = (0..MAX_LAYERS).map(|_| BandPass::new(20.0, 400.0, 48_000.0)).collect();
        let layer_bufs = (0..MAX_LAYERS).map(|i| scene::uniform_buffer(device, &format!("layer {i}"), std::mem::size_of::<LayerParams>() as u64)).collect();
        Ok(Self {
            layers,
            filters,
            layer_bufs,
            sampler,
            layer_layout,
            add_pipeline,
            copy_pipeline,
            mix_layout,
            mix_pipeline,
            mix_buf,
            one_buf,
            composite: hdr_texture(device, "composite", width, height),
            prev: hdr_texture(device, "composite prev", width, height),
            width,
            height,
            dissolve: None,
            pending_dissolve: None,
            last_switch: Instant::now(),
            last_track: None,
            quiet_for: 0.0,
            trend_peak: 0.0,
            last_cue: "manual",
            last_error: None,
        })
    }

    /// Rebuild any layer whose visual kind changed in the settings.
    pub fn sync(&mut self, device: &wgpu::Device, shaders: &ShaderSet, settings: &Settings) {
        for i in 0..MAX_LAYERS {
            let want = settings.layer(i).visual;
            if self.layers[i].kind() != want {
                match make(want, device, self.width, self.height, shaders) {
                    Ok(s) => self.layers[i] = s,
                    Err(e) => self.last_error = Some(format!("{}: {e}", want.name())),
                }
            }
        }
    }

    pub fn seconds_since_switch(&self) -> f32 {
        self.last_switch.elapsed().as_secs_f32()
    }

    /// Dissolve the current picture into whatever renders next.
    pub fn begin_dissolve(&mut self, seconds: f32) {
        self.pending_dissolve = Some(seconds.max(0.05));
        self.last_switch = Instant::now();
    }

    pub fn is_fading(&self) -> bool {
        self.dissolve.is_some() || self.pending_dissolve.is_some()
    }

    /// Auto-mode cue evaluation. Call once per frame; returns a cue when the
    /// director wants a new scene.
    pub fn consider(&mut self, f: &FeatureFrame, track_id: Option<u64>, dt: f32, auto: bool) -> Option<&'static str> {
        let track_changed = matches!((self.last_track, track_id), (Some(a), Some(b)) if a != b);
        if track_id.is_some() {
            self.last_track = track_id;
        }
        self.quiet_for = if f.quiet || f.silent { self.quiet_for + dt } else { 0.0 };
        self.trend_peak = if f.energy_trend > self.trend_peak { f.energy_trend } else { self.trend_peak * (1.0 - dt * 0.2) };
        let drop = self.trend_peak > 0.45 && f.energy_trend < -0.35;
        if !auto || self.is_fading() {
            return None;
        }
        let since = self.seconds_since_switch();
        let cue = if track_changed && since > 30.0 {
            Some("track change")
        } else if drop && since > MIN_DWELL_S {
            Some("drop")
        } else if self.quiet_for > QUIET_FOR_S && since > MIN_DWELL_S {
            Some("quiet")
        } else if since > MAX_DWELL_S {
            Some("dwell limit")
        } else {
            None
        };
        if let Some(c) = cue {
            self.last_cue = c;
            self.trend_peak = 0.0;
            self.quiet_for = 0.0;
        }
        cue
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        for s in &mut self.layers {
            s.resize(device, width, height);
        }
        self.composite = hdr_texture(device, "composite", width, height);
        self.prev = hdr_texture(device, "composite prev", width, height);
        self.dissolve = None;
    }

    pub fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        let mut errors = Vec::new();
        match build_layer(device, shaders, &self.layer_layout) {
            Ok((a, c)) => {
                self.add_pipeline = a;
                self.copy_pipeline = c;
            }
            Err(e) => errors.push(format!("layer: {e}")),
        }
        if let Err(e) = build_mix(device, shaders, &self.mix_layout).map(|p| self.mix_pipeline = p) {
            errors.push(format!("mix: {e}"));
        }
        for s in &mut self.layers {
            if let Err(e) = s.reload_shaders(device, shaders) {
                errors.push(format!("{}: {e}", s.name()));
            }
        }
        self.last_error = (!errors.is_empty()).then(|| errors.join("\n"));
    }

    /// Render every enabled layer and composite into `target`.
    #[allow(clippy::too_many_arguments)]
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, settings: &Settings, features: &FeatureFrame, points: &[[f32; 2]], sample_rate: u32, tint: [f32; 3], dt: f32, time: f32, auto_gain: f32, target: &wgpu::TextureView) {
        queue.write_buffer(&self.one_buf, 0, bytemuck::bytes_of(&LayerParams { opacity: 1.0, _pad: [0.0; 3] }));
        // Snapshot last frame's composite before anything renders over it.
        if let Some(secs) = self.pending_dissolve.take() {
            let group = scene::tex_group(device, &self.layer_layout, &[&self.composite], &self.sampler, &self.one_buf);
            fullscreen_pass(encoder, "dissolve snapshot", &self.prev, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.copy_pipeline, &group);
            self.dissolve = Some((0.0, secs));
        }

        let mut first = true;
        for i in 0..MAX_LAYERS.min(self.layers.len()) {
            let layer = settings.layer(i);
            if !layer.enabled || layer.opacity <= 0.0 {
                continue;
            }
            let band = layer.band;
            let filtered_features;
            let feats: &FeatureFrame = if band.enabled {
                filtered_features = band_features(features, band.lo_hz, band.hi_hz);
                &filtered_features
            } else {
                features
            };
            let filtered_points;
            let pts: &[[f32; 2]] = if band.enabled && layer.visual == VisualKind::Scope {
                self.filters[i].retune(band.lo_hz, band.hi_hz, sample_rate as f32);
                filtered_points = self.filters[i].process(points);
                &filtered_points
            } else {
                points
            };
            let ctx = FrameCtx { features: feats, tint, dt, time, width: self.width, height: self.height, points: pts, auto_gain };
            self.layers[i].render(queue, encoder, &ctx, &layer);
            queue.write_buffer(&self.layer_bufs[i], 0, bytemuck::bytes_of(&LayerParams { opacity: layer.opacity, _pad: [0.0; 3] }));
            let group = scene::tex_group(device, &self.layer_layout, &[self.layers[i].output()], &self.sampler, &self.layer_bufs[i]);
            let load = if first { wgpu::LoadOp::Clear(wgpu::Color::BLACK) } else { wgpu::LoadOp::Load };
            fullscreen_pass(encoder, "layer", &self.composite, load, &self.add_pipeline, &group);
            first = false;
        }
        if first {
            scene::clear_pass(encoder, "layers (none)", &self.composite);
        }

        // Composite → target, through the dissolve when one is running.
        match &mut self.dissolve {
            Some((t, secs)) => {
                *t += dt / *secs;
                let k = smoothstep(*t);
                queue.write_buffer(&self.mix_buf, 0, bytemuck::bytes_of(&MixParams { t: k, _pad: [0.0; 3] }));
                let group = scene::tex_group(device, &self.mix_layout, &[&self.prev, &self.composite], &self.sampler, &self.mix_buf);
                fullscreen_pass(encoder, "dissolve", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.mix_pipeline, &group);
                if *t >= 1.0 {
                    self.dissolve = None;
                }
            }
            None => {
                let group = scene::tex_group(device, &self.layer_layout, &[&self.composite], &self.sampler, &self.one_buf);
                fullscreen_pass(encoder, "composite", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.copy_pipeline, &group);
            }
        }
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn build_layer(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<(wgpu::RenderPipeline, wgpu::RenderPipeline), String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "layer.wgsl");
        (scene::pipeline(device, "layer add", layout, &m, HDR_FORMAT, Some(scene::ADDITIVE)), scene::pipeline(device, "layer copy", layout, &m, HDR_FORMAT, None))
    })
}

fn build_mix(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "mix.wgsl");
        scene::pipeline(device, "mix", layout, &m, HDR_FORMAT, None)
    })
}
