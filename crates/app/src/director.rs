//! Owns the scenes, decides which is showing, and cross-fades between them
//! into the post stack's input texture. In auto mode it changes scenes on
//! musical cues (track change, drops, long quiet) with dwell limits and
//! anti-repetition.

use std::time::Instant;

use audioviz_analysis::FeatureFrame;
use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, Scene, fullscreen_pass, HDR_FORMAT};
use crate::shaders::ShaderSet;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct MixParams {
    t: f32,
    _pad: [f32; 3],
}

const MIN_DWELL_S: f32 = 90.0;
const MAX_DWELL_S: f32 = 360.0;
const QUIET_FOR_S: f32 = 6.0;

pub struct Director {
    pub scenes: Vec<Box<dyn Scene>>,
    pub current: usize,
    previous: Option<usize>,
    /// (target scene, progress 0..1)
    fade: Option<(usize, f32)>,
    pub fade_seconds: f32,
    last_switch: Instant,
    // Auto-mode state.
    pub auto: bool,
    last_track: Option<u64>,
    quiet_for: f32,
    trend_peak: f32,
    rng: u32,
    pub last_cue: &'static str,
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    pub last_error: Option<String>,
}

impl Director {
    pub fn new(device: &wgpu::Device, shaders: &ShaderSet, scenes: Vec<Box<dyn Scene>>) -> Result<Self, String> {
        let sampler = scene::linear_sampler(device);
        let params_buf = scene::uniform_buffer(device, "mix params", std::mem::size_of::<MixParams>() as u64);
        let layout = scene::tex_layout(device, "mix", 1);
        let pipeline = build(device, shaders, &layout)?;
        Ok(Self {
            scenes,
            current: 0,
            previous: None,
            fade: None,
            fade_seconds: 4.0,
            last_switch: Instant::now(),
            auto: false,
            last_track: None,
            quiet_for: 0.0,
            trend_peak: 0.0,
            rng: 0x9E37_79B9,
            last_cue: "manual",
            sampler,
            params_buf,
            layout,
            pipeline,
            last_error: None,
        })
    }

    pub fn seconds_since_switch(&self) -> f32 {
        self.last_switch.elapsed().as_secs_f32()
    }

    /// Start a cross-fade to `index` (no-op if already there or fading).
    pub fn switch_to(&mut self, index: usize) {
        if index >= self.scenes.len() || index == self.current || self.fade.is_some() {
            return;
        }
        self.fade = Some((index, 0.0));
        self.last_switch = Instant::now();
    }

    pub fn next(&mut self) {
        let n = self.scenes.len();
        if n > 1 {
            self.switch_to((self.current + 1) % n);
        }
    }

    pub fn is_fading(&self) -> bool {
        self.fade.is_some()
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Pick a scene that is neither current nor the one before it.
    fn pick_next(&mut self) -> Option<usize> {
        let n = self.scenes.len();
        if n < 2 {
            return None;
        }
        let candidates: Vec<usize> = (0..n).filter(|i| *i != self.current && (n <= 2 || Some(*i) != self.previous)).collect();
        if candidates.is_empty() {
            return None;
        }
        let k = (self.rand() * candidates.len() as f32) as usize;
        Some(candidates[k.min(candidates.len() - 1)])
    }

    /// Auto-mode cue evaluation. Call once per frame.
    pub fn consider(&mut self, f: &FeatureFrame, track_id: Option<u64>, dt: f32) {
        // Track the cues even when not in auto mode so toggling it on is informed.
        let track_changed = match (self.last_track, track_id) {
            (Some(a), Some(b)) if a != b => true,
            _ => false,
        };
        if track_id.is_some() {
            self.last_track = track_id;
        }
        self.quiet_for = if f.quiet || f.silent { self.quiet_for + dt } else { 0.0 };
        // A drop: the energy trend was strongly rising and then collapses.
        self.trend_peak = if f.energy_trend > self.trend_peak { f.energy_trend } else { self.trend_peak * (1.0 - dt * 0.2) };
        let drop = self.trend_peak > 0.45 && f.energy_trend < -0.35;

        if !self.auto || self.fade.is_some() {
            return;
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
        if let Some(cue) = cue {
            if let Some(next) = self.pick_next() {
                self.last_cue = cue;
                self.trend_peak = 0.0;
                self.quiet_for = 0.0;
                self.switch_to(next);
            }
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        for s in &mut self.scenes {
            s.resize(device, width, height);
        }
    }

    pub fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        let mut errors = Vec::new();
        if let Err(e) = build(device, shaders, &self.layout).map(|p| self.pipeline = p) {
            errors.push(format!("mix: {e}"));
        }
        for s in &mut self.scenes {
            if let Err(e) = s.reload_shaders(device, shaders) {
                errors.push(format!("{}: {e}", s.name()));
            }
        }
        self.last_error = (!errors.is_empty()).then(|| errors.join("\n"));
    }

    /// Render the active scene(s) and composite into `target`.
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, target: &wgpu::TextureView) {
        let mut t = 0.0;
        let mut other = None;
        if let Some((to, progress)) = &mut self.fade {
            *progress += ctx.dt / self.fade_seconds.max(0.1);
            if *progress >= 1.0 {
                self.previous = Some(self.current);
                self.current = *to;
                self.fade = None;
            } else {
                t = smoothstep(*progress);
                other = Some(*to);
            }
        }

        self.scenes[self.current].render(queue, encoder, ctx);
        if let Some(o) = other {
            self.scenes[o].render(queue, encoder, ctx);
        }

        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&MixParams { t, _pad: [0.0; 3] }));
        let a = self.scenes[self.current].output();
        let b = other.map(|o| self.scenes[o].output()).unwrap_or(a);
        let group = scene::tex_group(device, &self.layout, &[a, b], &self.sampler, &self.params_buf);
        fullscreen_pass(encoder, "mix", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipeline, &group);
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "mix.wgsl");
        scene::pipeline(device, "mix", layout, &m, HDR_FORMAT, None)
    })
}
