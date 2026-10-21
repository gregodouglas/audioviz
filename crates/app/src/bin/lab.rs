//! audioviz-lab (ADR-0003): one visual on its own, held still, rendered by
//! the engine, in a window to turn it round or off-screen into PNGs and a
//! contact sheet, for design work. It reads the visuals and engine shaders
//! from this checkout, so a shader edit needs no rebuild (the window reloads
//! on save); time is its own and the music is silence (or a made-up groove,
//! --groove), so the same arguments
//! give the same pixels.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use audioviz::director::{Director, SpacePass};
use audioviz::gpu_timer::GpuTimer;
use audioviz::post::{Post, PostParams};
use audioviz::settings::{Layer, Look, Settings, Space};
use audioviz::shaders::ShaderSet;
use audioviz_analysis::FeatureFrame;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const HELP: &str = "\
audioviz-lab - one visual, still, rendered by the engine (ADR-0003)

usage: audioviz-lab <visual> [options]

  --window            open a window to turn it round (default: write PNGs)
  --scene <name>      start from a factory scene's picture, tint and the
                      visual's layer in it (e.g. --scene Jellyfish)
  --layer <n>         which of the scene's layers (default: the first
                      showing <visual>)
  --set id=value      a param of the visual (repeatable; e.g. count=1)
  --at <seconds>      run this long from the start, then hold (default 4)
  --run <seconds>     after --at, keep time running this long at 60 Hz in
                      the first shot and set, and measure how much each
                      frame changes from the last: prints the median and
                      the worst steps (a snap or a strobe is a spike) and
                      how far the light swings, and
                      writes <name>_run.csv and a strip of eight frames,
                      <name>_run.png
  --palette <name>    the layer's palette: a preset (phosphor, techno, neon,
                      rave, 80s pop, galaxy, fire, ice, sunset, mono) or none
  --groove <bpm>      play a made-up groove instead of silence: a kick on
                      every beat, a snare on two and four, hi-hats on the
                      eighths, the mids swelling over four bars, a drop
                      every sixteen bars (the same every run)
  --shots a,b,...     views: front side back above below close, or
                      yaw:pitch:zoom (degrees, degrees, distance factor);
                      default front,side,below
  --pan x,y           slide what the camera looks at right and up, in units
                      of its distance (every shot; default 0,0)
  --passes a,b,...    all (the picture), sharp (no diffusion), glow (only
                      the diffusion); default all
  --layers a,b,...    the visual's design layers to show, by name (its
                      visual.json layers; a prefix will do), each entry
                      a row: all, organs, glass+edges, ...; default all
  --exposure <x>      a fixed exposure, levelling and auto exposure off
                      (default 1); auto levels it as the app does (a lone
                      object fills the frame, so auto lifts it further)
  --bloom <x>         the Picture's bloom (default 0: off for design work)
  --size WxH          default 1600x1000
  --out <dir>         default tmp/lab
  --name <prefix>     file names start with it (default the visual's id)
  --dx12              D3D12 instead of Vulkan (slower to build shaders)
  --seed <n>          the layer's instance seed (default 1)

PNGs: <out>/<name>_<shot>_<pass>[_<layers>].png for each, and
<out>/<name>_sheet.png (shots across; passes, then layer sets, down).

window: left-drag orbits, right- or middle-drag pans, the wheel zooms;
1 2 3 the pass (all, sharp, glow); Tab shows the next design layer alone,
X everything but it, A all of them; Space runs or holds time; . steps one
frame; R resets the view; S saves a PNG into --out; Esc quits. Saving a
visual's files or an engine shader reloads it.
";

struct Args {
    visual: String,
    window: bool,
    scene: Option<String>,
    layer: Option<usize>,
    sets: Vec<(String, serde_json::Value)>,
    at: f32,
    shots: Vec<(String, [f32; 3])>,
    pan: [f32; 2],
    passes: Vec<SpacePass>,
    layers: Vec<String>,
    size: (u32, u32),
    out: PathBuf,
    name: Option<String>,
    dx12: bool,
    seed: u64,
    exposure: Option<f32>,
    bloom: f32,
    groove: Option<f32>,
    run: f32,
    palette: Option<String>,
}

fn shot(name: &str) -> anyhow::Result<[f32; 3]> {
    Ok(match name {
        "front" => [0.0, 0.0, 1.0],
        "side" => [90.0, 0.0, 1.0],
        "back" => [180.0, 0.0, 1.0],
        "above" => [0.0, -40.0, 1.0],
        "below" => [0.0, 40.0, 1.0],
        "close" => [0.0, 0.0, 0.5],
        other => {
            let v: Vec<f32> = other.split(':').map(str::parse).collect::<Result<_, _>>().with_context(|| format!("shot \"{other}\" is not a name or yaw:pitch:zoom"))?;
            match v[..] {
                [y, p, z] => [y, p, z],
                [y, p] => [y, p, 1.0],
                _ => bail!("shot \"{other}\": yaw:pitch:zoom"),
            }
        }
    })
}

fn pass(name: &str) -> anyhow::Result<SpacePass> {
    Ok(match name {
        "all" => SpacePass::All,
        "sharp" => SpacePass::Sharp,
        "glow" => SpacePass::Glow,
        other => bail!("pass \"{other}\" is not all, sharp or glow"),
    })
}

fn pass_name(p: SpacePass) -> &'static str {
    match p {
        SpacePass::All => "all",
        SpacePass::Sharp => "sharp",
        SpacePass::Glow => "glow",
    }
}

fn parse_args() -> anyhow::Result<Option<Args>> {
    let mut args = std::env::args().skip(1);
    let mut a = Args { visual: String::new(), window: false, scene: None, layer: None, sets: Vec::new(), at: 4.0, shots: Vec::new(), pan: [0.0, 0.0], passes: vec![SpacePass::All], layers: vec!["all".into()], size: (1600, 1000), out: PathBuf::from("tmp/lab"), name: None, dx12: false, seed: 1, exposure: Some(1.0), bloom: 0.0, groove: None, run: 0.0, palette: None };
    let mut shots = "front,side,below".to_string();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().with_context(|| format!("{arg} needs {what}"));
        match arg.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                return Ok(None);
            }
            "--window" => a.window = true,
            "--scene" => a.scene = Some(value("a scene name")?),
            "--layer" => a.layer = Some(value("a layer number")?.parse()?),
            "--set" => {
                let kv = value("id=value")?;
                let (k, v) = kv.split_once('=').context("--set needs id=value")?;
                a.sets.push((k.to_string(), serde_json::from_str(v).unwrap_or_else(|_| serde_json::Value::String(v.to_string()))));
            }
            "--at" => a.at = value("seconds")?.parse()?,
            "--shots" => shots = value("a list of shots")?,
            "--pan" => {
                let v = value("x,y")?;
                let (x, y) = v.split_once(',').context("--pan x,y")?;
                a.pan = [x.parse()?, y.parse()?];
            }
            "--passes" => a.passes = value("a list of passes")?.split(',').map(pass).collect::<anyhow::Result<_>>()?,
            "--layers" => a.layers = value("a list of layer sets")?.split(',').map(str::to_string).collect(),
            "--size" => {
                let v = value("WxH")?;
                let (w, h) = v.split_once('x').context("--size WxH")?;
                a.size = (w.parse()?, h.parse()?);
            }
            "--out" => a.out = PathBuf::from(value("a folder")?),
            "--name" => a.name = Some(value("a prefix")?),
            "--dx12" => a.dx12 = true,
            "--seed" => a.seed = value("a number")?.parse()?,
            "--exposure" => {
                let v = value("auto or a number")?;
                a.exposure = if v == "auto" { None } else { Some(v.parse()?) };
            }
            "--bloom" => a.bloom = value("a number")?.parse()?,
            "--groove" => a.groove = Some(value("beats a minute")?.parse()?),
            "--run" => a.run = value("seconds")?.parse()?,
            "--palette" => a.palette = Some(value("a palette's name")?),
            other if !other.starts_with('-') && a.visual.is_empty() => a.visual = other.to_string(),
            other => bail!("unknown option {other} (see --help)"),
        }
    }
    if a.visual.is_empty() {
        print!("{HELP}");
        return Ok(None);
    }
    a.shots = shots.split(',').map(|s| shot(s).map(|v| (s.replace(':', "_"), v))).collect::<anyhow::Result<_>>()?;
    Ok(Some(a))
}

/// A made-up groove at `bpm`, `t` seconds in (`dt` since the last frame,
/// so each hit lands on one frame): a kick on every beat, a snare on two
/// and four, hi-hats on the eighths, the mids swelling over four bars, and
/// every sixteen bars a drop (a harder kick on a building swell). Shaped
/// like the analysis's output: levels about 1 at average, onsets as
/// strength, hit and decaying pulse, a sure beat clock.
fn groove(bpm: f32, t: f32, dt: f32) -> FeatureFrame {
    let beats = t * bpm / 60.0;
    let before = (t - dt).max(0.0) * bpm / 60.0;
    let phase = beats.fract();
    let count = beats.floor() as u32;
    let on_beat = beats.floor() > before.floor();
    let on_eighth = (beats * 2.0).floor() > (before * 2.0).floor();
    let drop_beat = count % 64 == 0 && count > 0;
    let kick = (-phase * 7.0).exp() * if drop_beat { 1.0 } else { 0.8 };
    let snare = if count % 2 == 1 { (-phase * 9.0).exp() } else { 0.0 };
    let hat = (-(beats * 2.0).fract() * 12.0).exp() * 0.8;
    let swell = 0.5 - 0.5 * (beats / 16.0 * std::f32::consts::TAU).cos();
    let mut f = FeatureFrame::default();
    f.time_s = t as f64;
    f.rms = 0.15 + 0.1 * kick;
    f.rms_slow = 0.18;
    f.peak = 0.6;
    f.lufs_m = -14.0;
    f.lufs_s = -14.0;
    f.bass = 0.8 + kick * if drop_beat { 1.2 } else { 0.6 };
    f.bass_att = 0.85 + kick * if drop_beat { 0.9 } else { 0.35 };
    f.mid = 0.8 + 0.4 * swell + 0.5 * snare;
    f.mid_att = 0.8 + 0.4 * swell + 0.25 * snare;
    f.treb = 0.85 + 0.6 * hat;
    f.treb_att = 0.85 + 0.35 * hat;
    f.bands = (0..32).map(|i| {
        let x = i as f32 / 31.0;
        (0.35 + 0.5 * kick * (1.0 - x).powi(3) + 0.3 * swell * (1.0 - (x - 0.45).abs() * 2.0).max(0.0) + 0.4 * hat * x * x).min(1.0)
    }).collect();
    f.levels = f.bands.iter().map(|b| b * 0.8).collect();
    f.attacks = (0..32).map(|i| {
        let x = i as f32 / 31.0;
        (kick * (1.0 - x).powi(3) + snare * (1.0 - (x - 0.4).abs() * 3.0).max(0.0) + hat * x * x).min(1.0)
    }).collect();
    f.broad_level = [0.5 + 0.4 * kick, 0.55 + 0.35 * kick, 0.5 + 0.2 * swell, 0.5 + 0.3 * swell + 0.2 * snare, 0.45 + 0.3 * hat, 0.4 + 0.4 * hat];
    f.broad_relative = [0.8 + kick, 0.8 + 0.8 * kick, 0.8 + 0.4 * swell, 0.8 + 0.4 * swell + 0.4 * snare, 0.8 + 0.6 * hat, 0.8 + 0.6 * hat];
    f.broad_attack = [kick, kick * 0.8, snare * 0.5, snare, hat * 0.7, hat];
    f.centroid_hz = 1200.0 + 2500.0 * hat;
    f.stereo_width = 0.4;
    f.onset_strength = [kick, snare, hat, kick.max(snare).max(hat)];
    f.onset_hit = [on_beat, on_beat && count % 2 == 1, on_eighth, on_beat || on_eighth];
    f.onset_pulse = [kick, snare, hat, kick.max(snare).max(hat)];
    f.bpm = bpm;
    f.beat_confidence = 0.9;
    f.beat_phase = phase;
    f.beat_count = count;
    f.beat_pulse = (-phase * 7.0).exp();
    f.chroma[((count / 8) * 7 % 12) as usize] = 1.0;
    f.chroma_slow = f.chroma;
    f.key_class = ((count / 8) * 7 % 12) as u8;
    f.energy_trend = swell * 2.0 - 1.0;
    f
}

/// The checkout this lab was built from (the visuals and shaders it reads).
fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The settings for one layer of the visual in a shared space: from a
/// factory scene when asked (its picture, tint and the visual's layer),
/// with the params from the command line on top.
fn settings(a: &Args) -> anyhow::Result<Settings> {
    let mut s = Settings::default();
    let mut layer = Layer::new(&a.visual, true, 1.0);
    s.post.look = Look::Neon;
    if let Some(name) = &a.scene {
        let path = repo().join("assets").join("scenes").join(format!("{name}.json"));
        let text = std::fs::read_to_string(&path).with_context(|| format!("no scene {}", path.display()))?;
        let scene = audioviz::scene_store::from_text(&text)?;
        let i = match a.layer {
            Some(i) => i,
            None => scene.layers.iter().position(|l| l.visual == a.visual).with_context(|| format!("scene {name} has no {} layer", a.visual))?,
        };
        layer = scene.layer(i);
        layer.enabled = true;
        layer.opacity = 1.0;
        s.post = scene.post;
        s.tint = scene.tint;
    }
    if let Some(name) = &a.palette {
        if name != "none" && audioviz::color::preset(name).is_none() {
            bail!("no palette {name} (see --help)");
        }
        layer.palette.name = name.to_lowercase();
    }
    let params = layer.params.entry(a.visual.clone()).or_insert_with(|| serde_json::Value::Object(Default::default()));
    if let Some(obj) = params.as_object_mut() {
        for (k, v) in &a.sets {
            obj.insert(k.clone(), v.clone());
        }
    }
    // Still pictures for design work: no grain, bloom only when asked, a
    // fixed exposure unless asked to level.
    s.post.grain = 0.0;
    s.post.bloom = a.bloom;
    if let Some(x) = a.exposure {
        s.post.auto_exposure = false;
        s.post.exposure = x;
    }
    s.layers = vec![layer];
    s.space = Space::Shared;
    s.onboarded = true;
    s.normalize();
    Ok(s)
}

/// The visual's design layers' names (visual.json "layers"; with variants,
/// those of the variant the layer's values pick).
fn layer_names(visual: &str, s: &Settings) -> Vec<String> {
    match audioviz::visuals::get(visual).map(|v| &v.source) {
        Some(audioviz::visuals::Source::Loaded(d)) => d.variant_for(s.layer(0).params.get(visual)).layers.clone(),
        None => Vec::new(),
    }
}

/// A layer set as bits: "all", or names (or their starts) joined by "+".
fn layer_mask(names: &[String], set: &str) -> anyhow::Result<u32> {
    if set == "all" {
        return Ok(u32::MAX);
    }
    let key = |s: &str| s.to_lowercase().chars().filter(char::is_ascii_alphanumeric).collect::<String>();
    let mut mask = 0;
    for want in set.split('+') {
        let w = key(want);
        let i = names.iter().position(|n| key(n) == w).or_else(|| names.iter().position(|n| key(n).starts_with(&w))).with_context(|| format!("no design layer \"{want}\" (there are: {})", names.join(", ")))?;
        mask |= 1 << i;
    }
    Ok(mask)
}

/// Read this checkout's visuals and shaders, never the user's folder, with
/// a fixed instance. Called before any other thread starts.
fn set_env(a: &Args) {
    unsafe {
        std::env::set_var("AUDIOVIZ_REPO", repo());
        std::env::set_var("AUDIOVIZ_DEFAULTS", "1");
        std::env::set_var("AUDIOVIZ_SEED", a.seed.to_string());
    }
}

fn instance(a: &Args) -> wgpu::Instance {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    desc.backends = if a.dx12 { wgpu::Backends::DX12 } else { wgpu::Backends::VULKAN };
    wgpu::Instance::new(desc)
}

fn device(adapter: &wgpu::Adapter) -> anyhow::Result<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("audioviz lab"),
        required_features: GpuTimer::features(adapter),
        required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .context("request device")
}

/// The engine with one visual, its picture going to a texture of `format`.
struct Lab {
    device: wgpu::Device,
    queue: wgpu::Queue,
    shaders: ShaderSet,
    director: Director,
    post: Post,
    timer: GpuTimer,
    settings: Settings,
    features: FeatureFrame,
    time: f32,
    groove: Option<f32>,
}

impl Lab {
    fn new(device: wgpu::Device, queue: wgpu::Queue, settings: Settings, size: (u32, u32), format: wgpu::TextureFormat) -> anyhow::Result<Self> {
        let shaders = ShaderSet::load();
        let director = Director::new(&device, &shaders, size.0, size.1, &settings).map_err(|e| anyhow::anyhow!("director: {e}"))?;
        let post = Post::new(&device, format, size.0, size.1, &shaders).map_err(|e| anyhow::anyhow!("post: {e}"))?;
        let timer = GpuTimer::new(&device, &queue);
        Ok(Self { device, queue, shaders, director, post, timer, settings, features: FeatureFrame::default(), time: 0.0, groove: None })
    }

    /// Wait for the visual's build (on the engine's worker); how long it took.
    fn built(&mut self, visual: &str) -> anyhow::Result<Duration> {
        let start = Instant::now();
        while !self.director.layers[0].ready() {
            if start.elapsed() > Duration::from_secs(120) {
                bail!("{visual} did not build in 120 s");
            }
            std::thread::sleep(Duration::from_millis(10));
            self.director.poll(&self.device, &self.shaders, &self.settings);
        }
        if let Some(e) = &self.director.last_error {
            bail!("{e}");
        }
        if let Some(e) = self.director.layers[0].error() {
            bail!("{visual}: {e}");
        }
        Ok(start.elapsed())
    }

    /// Run to `at` on a 60 Hz clock.
    fn settle(&mut self, view: &wgpu::TextureView, at: f32) {
        let steps = (at * 60.0).round().max(1.0) as u32;
        for _ in 0..steps {
            self.frame(view, 1.0 / 60.0, None);
        }
    }

    /// One frame into `view`, `dt` on from the last; `then` adds commands
    /// after it (a read back) to the same submission.
    fn frame(&mut self, view: &wgpu::TextureView, dt: f32, then: Option<&dyn Fn(&mut wgpu::CommandEncoder)>) {
        self.time += dt;
        if let Some(bpm) = self.groove {
            self.features = groove(bpm, self.time, dt);
        }
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("lab") });
        self.timer.begin(&self.device);
        let s = &self.settings;
        self.director.render(&self.device, &self.queue, &mut encoder, s, &self.features, &[], 48000, s.tint, None, None, 0, dt, self.time, 1.0, self.post.input(), self.post.exposure(), &mut self.timer);
        let p = s.post;
        self.post.render(&self.queue, &mut encoder, view, PostParams { exposure: p.exposure, bloom: p.bloom, grain: p.grain, time: self.time, dt, flags: p.auto_exposure as u32, flash_rise: 2.0, saturation: p.saturation, look: (p.look == Look::Neon) as u32, ..Default::default() });
        self.timer.finish(&mut encoder);
        if let Some(then) = then {
            then(&mut encoder);
        }
        self.queue.submit([encoder.finish()]);
        self.timer.after_submit();
    }

    /// Pick up saved files: the visual's (the director polls its folder) and
    /// the engine's shaders.
    fn reload(&mut self) {
        self.director.poll(&self.device, &self.shaders, &self.settings);
        if self.shaders.poll() {
            self.director.reload_shaders(&self.device, &self.shaders);
            if let Err(e) = self.post.reload_shaders(&self.device, &self.shaders) {
                eprintln!("post: {e}");
            }
            if let Some(e) = &self.director.last_error {
                eprintln!("{e}");
            }
        }
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.director.resize(&self.device, w, h);
        self.post.resize(&self.device, w, h);
    }
}

/// Render a frame into `texture` (RGBA or BGRA, 8 bits) and copy it out.
fn read_back(lab: &mut Lab, view: &wgpu::TextureView, texture: &wgpu::Texture, dt: f32) -> anyhow::Result<image::RgbaImage> {
    let (w, h) = (texture.width(), texture.height());
    let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buf = lab.device.create_buffer(&wgpu::BufferDescriptor { label: Some("lab read"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let copy = |encoder: &mut wgpu::CommandEncoder| {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    };
    lab.frame(view, dt, Some(&copy));
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = lab.device.poll(wgpu::PollType::wait_indefinitely());
    let data = buf.slice(..).get_mapped_range().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let bgra = matches!(texture.format(), wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    let mut img = image::RgbaImage::new(w, h);
    for y in 0..h {
        let line = &data[(y * row) as usize..(y * row + w * 4) as usize];
        let out = &mut img.as_mut()[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
        out.copy_from_slice(line);
        if bgra {
            for px in out.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
    }
    Ok(img)
}

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn main() -> anyhow::Result<()> {
    let Some(a) = parse_args()? else { return Ok(()) };
    set_env(&a);
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"))).init();
    let settings = settings(&a)?;
    if audioviz::visuals::get(&a.visual).is_none() {
        bail!("no visual \"{}\" in {}", a.visual, repo().join("assets").join("visuals").display());
    }
    if a.window {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);
        let mut viewer = Viewer { args: a, settings: Some(settings), view: None, error: None };
        event_loop.run_app(&mut viewer)?;
        return viewer.error.map_or(Ok(()), Err);
    }
    shots(&a, settings)
}

/// The batch lab: every shot in every pass, as PNGs and a contact sheet.
fn shots(a: &Args, settings: Settings) -> anyhow::Result<()> {
    let started = Instant::now();
    let instance = instance(a);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: None, ..Default::default() })).context("no GPU adapter")?;
    let (device, queue) = device(&adapter)?;
    let (w, h) = a.size;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lab picture"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut lab = Lab::new(device, queue, settings, a.size, FORMAT)?;
    lab.groove = a.groove;
    let built = lab.built(&a.visual)?;
    lab.settle(&view, a.at);
    std::fs::create_dir_all(&a.out)?;
    let name = a.name.clone().unwrap_or_else(|| a.visual.clone());
    let names = layer_names(&a.visual, &lab.settings);
    let sets: Vec<(String, u32)> = a.layers.iter().map(|set| layer_mask(&names, set).map(|m| (set.clone(), m))).collect::<anyhow::Result<_>>()?;
    if a.run > 0.0 {
        return run(a, &mut lab, &view, &target, &name, sets[0].1);
    }
    let mut tiles = Vec::new();
    let mut gpu = Vec::new();
    for (set, mask) in &sets {
        audioviz::shader_visual::show_layers(*mask);
        for &p in &a.passes {
            for (sname, [yaw, pitch, zoom]) in &a.shots {
                lab.director.camera_nudge = [*yaw, *pitch, *zoom, a.pan[0], a.pan[1]];
                lab.director.pass = p;
                // A few frames, so whatever reads last frame's camera has it.
                for _ in 0..3 {
                    lab.frame(&view, 0.0, None);
                }
                let img = read_back(&mut lab, &view, &target, 0.0)?;
                gpu.push(lab.timer.frame_ms);
                let tag = if *mask == u32::MAX { String::new() } else { format!("_{}", set.replace('+', "-")) };
                img.save(a.out.join(format!("{name}_{sname}_{}{tag}.png", pass_name(p))))?;
                tiles.push((format!("{sname} {} {}", pass_name(p), if *mask == u32::MAX { "" } else { set }), img));
            }
        }
    }
    let sheet = a.out.join(format!("{name}_sheet.png"));
    // One view: the tiles four across; else the views across.
    let cols = if a.shots.len() == 1 { tiles.len().min(4) } else { a.shots.len() };
    contact_sheet(&tiles, cols).save(&sheet)?;
    gpu.sort_by(f32::total_cmp);
    println!("{}: built in {:.1} s, held at {:.2} s, {} shots, frame {:.2} ms on the GPU, {:.1} s in all", a.visual, built.as_secs_f32(), lab.time, tiles.len(), gpu.get(gpu.len() / 2).copied().unwrap_or(0.0), started.elapsed().as_secs_f32());
    println!("{}", sheet.display());
    Ok(())
}

/// The lab running (--run): time goes on from `at` at 60 Hz in the first
/// shot, and each frame is compared with the one before it (the mean
/// absolute change of its pixels, 0..255). Motion that eases changes the
/// picture a little every frame; a snap, a strobe or a jump in phase is a
/// spike well above the median.
fn run(a: &Args, lab: &mut Lab, view: &wgpu::TextureView, target: &wgpu::Texture, name: &str, mask: u32) -> anyhow::Result<()> {
    audioviz::shader_visual::show_layers(mask);
    let (sname, [yaw, pitch, zoom]) = &a.shots[0];
    lab.director.camera_nudge = [*yaw, *pitch, *zoom, a.pan[0], a.pan[1]];
    lab.director.pass = a.passes[0];
    // A few frames, so whatever reads last frame's camera has it.
    for _ in 0..3 {
        lab.frame(view, 0.0, None);
    }
    let frames = (a.run * 60.0).round().max(2.0) as usize;
    let mut last = read_back(lab, view, target, 1.0 / 60.0)?;
    let mut changes = Vec::with_capacity(frames);
    let mut light = Vec::with_capacity(frames);
    let mut strip = Vec::new();
    for f in 1..=frames {
        let img = read_back(lab, view, target, 1.0 / 60.0)?;
        let sum: u64 = img.as_raw().chunks_exact(4).zip(last.as_raw().chunks_exact(4)).map(|(p, q)| (0..3).map(|c| (p[c] as i32 - q[c] as i32).unsigned_abs() as u64).sum::<u64>()).sum();
        changes.push((lab.time, sum as f32 / (img.width() * img.height() * 3) as f32));
        light.push(img.as_raw().chunks_exact(4).map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum::<u64>() as f32 / (img.width() * img.height() * 3) as f32);
        if (f - 1) % (frames / 8).max(1) == 0 && strip.len() < 8 {
            strip.push((format!("{:.2} s", lab.time), img.clone()));
        }
        last = img;
    }
    let csv: String = changes.iter().zip(&light).map(|((t, c), l)| format!("{t:.4},{c:.4},{l:.4}\n")).collect();
    std::fs::write(a.out.join(format!("{name}_run.csv")), format!("time,change,light\n{csv}"))?;
    let png = a.out.join(format!("{name}_run.png"));
    contact_sheet(&strip, 4).save(&png)?;
    let mut sorted: Vec<f32> = changes.iter().map(|c| c.1).collect();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[sorted.len() / 2].max(1e-4);
    let mut worst = changes.clone();
    worst.sort_by(|x, y| y.1.total_cmp(&x.1));
    let spikes = changes.iter().filter(|c| c.1 > median * 4.0).count();
    println!("{}: ran {:.2} s in {sname} from {:.2} s: change per frame median {:.3}, p95 {:.3}, max {:.3} ({:.1}x the median); {} frames over 4x the median", a.visual, a.run, a.at, median, sorted[sorted.len() * 95 / 100], sorted[sorted.len() - 1], sorted[sorted.len() - 1] / median, spikes);
    let (lo, hi) = light.iter().fold((f32::MAX, 0.0f32), |(lo, hi), &l| (lo.min(l), hi.max(l)));
    println!("light (mean of the frame, 0..255): {:.2} to {:.2}, swinging {:.0}%", lo, hi, (hi - lo) / lo.max(1e-3) * 100.0);
    println!("worst: {}", worst.iter().take(5).map(|(t, c)| format!("{t:.2} s {c:.3}")).collect::<Vec<_>>().join(", "));
    println!("{}", png.display());
    Ok(())
}

// ---------------------------------------------------------------- the window

struct Viewer {
    args: Args,
    settings: Option<Settings>,
    view: Option<View>,
    error: Option<anyhow::Error>,
}

struct View {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    lab: Lab,
    /// The camera nudge: yaw, pitch (degrees), zoom, pan right, pan up.
    cam: [f32; 5],
    drag: Option<MouseButton>,
    cursor: (f64, f64),
    running: bool,
    step: bool,
    save: bool,
    shown: Instant,
    /// The visual's design layers, the one picked (None: all) and whether
    /// it shows alone or everything but it.
    names: Vec<String>,
    solo: Option<usize>,
    without: bool,
}

impl View {
    fn open(event_loop: &ActiveEventLoop, a: &Args, settings: Settings) -> anyhow::Result<Self> {
        let window = Arc::new(event_loop.create_window(Window::default_attributes().with_title(format!("audioviz-lab: {}", a.visual)).with_inner_size(winit::dpi::PhysicalSize::new(a.size.0, a.size.1)))?);
        let instance = instance(a);
        let surface = instance.create_surface(window.clone())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), ..Default::default() })).context("no GPU adapter")?;
        let (device, queue) = device(&adapter)?;
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let mut config = surface.get_default_config(&adapter, w, h).context("surface not supported")?;
        config.format = format;
        config.present_mode = wgpu::PresentMode::Fifo;
        // Saving a PNG copies the picture out of the window.
        if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        surface.configure(&device, &config);
        let mut lab = Lab::new(device, queue, settings, (w, h), format)?;
        lab.groove = a.groove;
        let built = lab.built(&a.visual)?;
        // Settle off-screen, then show it held.
        let scratch = lab.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("lab settle"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        lab.settle(&scratch.create_view(&Default::default()), a.at);
        println!("{}: built in {:.1} s, held at {:.2} s", a.visual, built.as_secs_f32(), lab.time);
        lab.director.pass = a.passes.first().copied().unwrap_or_default();
        let names = layer_names(&a.visual, &lab.settings);
        audioviz::shader_visual::show_layers(layer_mask(&names, a.layers.first().map_or("all", String::as_str))?);
        Ok(Self { window, surface, config, lab, cam: [0.0, 0.0, 1.0, a.pan[0], a.pan[1]], drag: None, cursor: (0.0, 0.0), running: false, step: false, save: false, shown: Instant::now(), names, solo: None, without: false })
    }

    /// Show the picked design layer alone, everything but it, or all.
    fn pick_layers(&self) {
        let mask = match self.solo {
            None => u32::MAX,
            Some(i) if self.without => !(1 << i),
            Some(i) => 1 << i,
        };
        audioviz::shader_visual::show_layers(mask);
    }

    /// What the design layers show, for the title.
    fn layers_label(&self) -> String {
        match self.solo {
            None => "all layers".into(),
            Some(i) => format!("{}{}", if self.without { "without " } else { "only " }, self.names.get(i).map_or("?", String::as_str)),
        }
    }

    fn redraw(&mut self, a: &Args) {
        self.lab.reload();
        self.lab.director.camera_nudge = self.cam;
        let dt = if self.running || std::mem::take(&mut self.step) { 1.0 / 60.0 } else { 0.0 };
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            _ => {
                self.surface.configure(&self.lab.device, &self.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        if std::mem::take(&mut self.save) && self.config.usage.contains(wgpu::TextureUsages::COPY_SRC) {
            match read_back(&mut self.lab, &view, &frame.texture, dt) {
                Ok(img) => {
                    let name = a.name.clone().unwrap_or_else(|| a.visual.clone());
                    let [y, p, z, ..] = self.cam;
                    let layers = self.solo.map_or(String::new(), |i| format!("_{}{}", if self.without { "without-" } else { "" }, self.names.get(i).map_or("layer", String::as_str).to_lowercase().replace(' ', "-")));
                    let file = a.out.join(format!("{name}_{y:.0}_{p:.0}_{z:.2}_{}{layers}.png", pass_name(self.lab.director.pass)));
                    let saved = std::fs::create_dir_all(&a.out).map_err(anyhow::Error::from).and_then(|_| img.save(&file).map_err(anyhow::Error::from));
                    match saved {
                        Ok(()) => println!("saved {}", file.display()),
                        Err(e) => eprintln!("could not save {}: {e}", file.display()),
                    }
                }
                Err(e) => eprintln!("could not read the picture back: {e}"),
            }
        } else {
            self.lab.frame(&view, dt, None);
        }
        self.window.pre_present_notify();
        self.lab.queue.present(frame);
        if self.shown.elapsed() > Duration::from_millis(250) {
            self.shown = Instant::now();
            let [y, p, z, ..] = self.cam;
            let state = if self.running { "running" } else { "held" };
            let error = if self.lab.director.last_error.is_some() || self.lab.director.layers[0].error().is_some() { " | shader error (see the console; showing the last good one)" } else { "" };
            self.window.set_title(&format!("audioviz-lab: {} | {} | {} | {state} at {:.2} s | yaw {y:.0} pitch {p:.0} zoom {z:.2} | {:.2} ms{error} | drag orbit, right-drag pan, wheel zoom, 1 2 3 pass, Tab layer, X without, A all, Space run, . step, R reset, S save", a.visual, pass_name(self.lab.director.pass), self.layers_label(), self.lab.time, self.lab.timer.frame_ms));
        }
    }
}

impl ApplicationHandler for Viewer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(settings) = self.settings.take() else { return };
        match View::open(event_loop, &self.args, settings) {
            Ok(v) => self.view = Some(v),
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(v) = self.view.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                let (w, h) = (size.width.max(1), size.height.max(1));
                v.config.width = w;
                v.config.height = h;
                v.surface.configure(&v.lab.device, &v.config);
                v.lab.resize(w, h);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                v.drag = (state == ElementState::Pressed).then_some(button);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (dx, dy) = ((position.x - v.cursor.0) as f32, (position.y - v.cursor.1) as f32);
                v.cursor = (position.x, position.y);
                match v.drag {
                    Some(MouseButton::Left) => {
                        v.cam[0] += dx * 0.3;
                        v.cam[1] = (v.cam[1] - dy * 0.3).clamp(-89.0, 89.0);
                    }
                    Some(MouseButton::Right | MouseButton::Middle) => {
                        // Pan in units of the camera's distance: about one
                        // distance across the picture's height.
                        let k = 0.9 / v.config.height as f32 * v.cam[2];
                        v.cam[3] -= dx * k;
                        v.cam[4] += dy * k;
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                v.cam[2] = (v.cam[2] * 0.9f32.powf(lines)).clamp(0.05, 20.0);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => match event.logical_key.as_ref() {
                Key::Named(NamedKey::Escape) => event_loop.exit(),
                Key::Named(NamedKey::Space) => v.running = !v.running,
                Key::Character("1") => v.lab.director.pass = SpacePass::All,
                Key::Character("2") => v.lab.director.pass = SpacePass::Sharp,
                Key::Character("3") => v.lab.director.pass = SpacePass::Glow,
                Key::Character(".") => v.step = true,
                Key::Named(NamedKey::Tab) if !v.names.is_empty() => {
                    v.solo = match v.solo {
                        None => Some(0),
                        Some(i) if i + 1 < v.names.len() => Some(i + 1),
                        Some(_) => None,
                    };
                    v.pick_layers();
                }
                Key::Character("x" | "X") if v.solo.is_some() => {
                    v.without = !v.without;
                    v.pick_layers();
                }
                Key::Character("a" | "A") => {
                    v.solo = None;
                    v.pick_layers();
                }
                Key::Character("r" | "R") => v.cam = [0.0, 0.0, 1.0, self.args.pan[0], self.args.pan[1]],
                Key::Character("s" | "S") => v.save = true,
                _ => {}
            },
            WindowEvent::RedrawRequested => v.redraw(&self.args),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(v) = &self.view {
            v.window.request_redraw();
        }
    }
}

// ---------------------------------------------------------------- the sheet

/// Tiles in rows of `cols`, each scaled to fit 800 px across, labelled.
fn contact_sheet(tiles: &[(String, image::RgbaImage)], cols: usize) -> image::RgbaImage {
    let (w0, h0) = tiles[0].1.dimensions();
    let scale = (800.0 / w0 as f32).min(1.0);
    let (tw, th) = ((w0 as f32 * scale) as u32, (h0 as f32 * scale) as u32);
    let cols = cols.max(1) as u32;
    let rows = (tiles.len() as u32).div_ceil(cols);
    let mut sheet = image::RgbaImage::from_pixel(tw * cols, th * rows, image::Rgba([0, 0, 0, 255]));
    for (k, (label, img)) in tiles.iter().enumerate() {
        let small = image::imageops::resize(img, tw, th, image::imageops::FilterType::Triangle);
        let (x, y) = ((k as u32 % cols) * tw, (k as u32 / cols) * th);
        image::imageops::overlay(&mut sheet, &small, x as i64, y as i64);
        text(&mut sheet, x + 8, y + 8, label);
    }
    sheet
}

/// A label in a 5 x 7 pixel font, drawn at twice its size.
fn text(img: &mut image::RgbaImage, x: u32, y: u32, s: &str) {
    let mut cx = x;
    for ch in s.chars() {
        let rows = glyph(ch.to_ascii_uppercase());
        for (r, bits) in rows.iter().enumerate() {
            for c in 0..5 {
                if bits >> (4 - c) & 1 == 1 {
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (px, py) = (cx + c * 2 + dx, y + r as u32 * 2 + dy);
                        if px < img.width() && py < img.height() {
                            img.put_pixel(px, py, image::Rgba([235, 235, 235, 255]));
                        }
                    }
                }
            }
        }
        cx += 12;
    }
}

fn glyph(c: char) -> [u8; 7] {
    match c {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 17, 25, 21, 19, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [31, 2, 4, 2, 1, 17, 14],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 30, 1, 1, 17, 14],
        '6' => [6, 8, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 2, 12],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        _ => [0; 7],
    }
}
