//! audioviz-lab (ADR-0003): one visual on its own, held still, rendered by
//! the engine off-screen into PNGs and a contact sheet, for design work.
//! It reads the visuals and engine shaders from this checkout, so a shader
//! edit needs no rebuild; time is its own and the music is silence, so the
//! same arguments give the same pixels.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use audioviz::director::{Director, SpacePass};
use audioviz::gpu_timer::GpuTimer;
use audioviz::post::{Post, PostParams};
use audioviz::settings::{Layer, Look, Settings, Space};
use audioviz::shaders::ShaderSet;
use audioviz_analysis::FeatureFrame;

const HELP: &str = "\
audioviz-lab - one visual, still, rendered by the engine (ADR-0003)

usage: audioviz-lab <visual> [options]

  --scene <name>      start from a factory scene's picture, tint and the
                      visual's layer in it (e.g. --scene Jellyfish)
  --layer <n>         which of the scene's layers (default: the first
                      showing <visual>)
  --set id=value      a param of the visual (repeatable; e.g. count=1)
  --at <seconds>      run this long from the start, then hold (default 4)
  --shots a,b,...     views: front side back above below close, or
                      yaw:pitch:zoom (degrees, degrees, distance factor);
                      default front,side,below
  --passes a,b,...    all (the picture), sharp (no diffusion), glow (only
                      the diffusion); default all
  --exposure <x>      a fixed exposure, levelling and auto exposure off
                      (default 1); auto levels it as the app does (a lone
                      object fills the frame, so auto lifts it further)
  --size WxH          default 1600x1000
  --out <dir>         default tmp/lab
  --name <prefix>     file names start with it (default the visual's id)
  --dx12              D3D12 instead of Vulkan (slower to build shaders)
  --seed <n>          the layer's instance seed (default 1)

writes <out>/<name>_<shot>_<pass>.png for each, and <out>/<name>_sheet.png
(shots across, passes down).
";

struct Args {
    visual: String,
    scene: Option<String>,
    layer: Option<usize>,
    sets: Vec<(String, serde_json::Value)>,
    at: f32,
    shots: Vec<(String, [f32; 3])>,
    passes: Vec<SpacePass>,
    size: (u32, u32),
    out: PathBuf,
    name: Option<String>,
    dx12: bool,
    seed: u64,
    exposure: Option<f32>,
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
    let mut a = Args { visual: String::new(), scene: None, layer: None, sets: Vec::new(), at: 4.0, shots: Vec::new(), passes: vec![SpacePass::All], size: (1600, 1000), out: PathBuf::from("tmp/lab"), name: None, dx12: false, seed: 1, exposure: Some(1.0) };
    let mut shots = "front,side,below".to_string();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().with_context(|| format!("{arg} needs {what}"));
        match arg.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                return Ok(None);
            }
            "--scene" => a.scene = Some(value("a scene name")?),
            "--layer" => a.layer = Some(value("a layer number")?.parse()?),
            "--set" => {
                let kv = value("id=value")?;
                let (k, v) = kv.split_once('=').context("--set needs id=value")?;
                a.sets.push((k.to_string(), serde_json::from_str(v).unwrap_or_else(|_| serde_json::Value::String(v.to_string()))));
            }
            "--at" => a.at = value("seconds")?.parse()?,
            "--shots" => shots = value("a list of shots")?,
            "--passes" => a.passes = value("a list of passes")?.split(',').map(pass).collect::<anyhow::Result<_>>()?,
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
    // Still pictures: no grain, the jellies' neon look.
    s.post.grain = 0.0;
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
        s.post.grain = 0.0;
        s.tint = scene.tint;
    }
    let params = layer.params.entry(a.visual.clone()).or_insert_with(|| serde_json::Value::Object(Default::default()));
    if let Some(obj) = params.as_object_mut() {
        for (k, v) in &a.sets {
            obj.insert(k.clone(), v.clone());
        }
    }
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

struct Lab {
    device: wgpu::Device,
    queue: wgpu::Queue,
    shaders: ShaderSet,
    director: Director,
    post: Post,
    timer: GpuTimer,
    settings: Settings,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
    features: FeatureFrame,
}

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

impl Lab {
    /// One frame at `time`, `dt` after the last; with `read`, its pixels.
    fn frame(&mut self, time: f32, dt: f32, read: bool) -> Option<image::RgbaImage> {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("lab") });
        self.timer.begin(&self.device);
        let s = &self.settings;
        self.director.render(&self.device, &self.queue, &mut encoder, s, &self.features, &[], 48000, s.tint, None, None, 0, dt, time, 1.0, self.post.input(), &mut self.timer);
        let p = s.post;
        self.post.render(&self.queue, &mut encoder, &self.view, PostParams { exposure: p.exposure, bloom: p.bloom, grain: p.grain, time, dt, flags: p.auto_exposure as u32, flash_rise: 2.0, saturation: p.saturation, look: (p.look == Look::Neon) as u32, ..Default::default() });
        self.timer.finish(&mut encoder);
        let (w, h) = self.size;
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buf = read.then(|| self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("lab read"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }));
        if let Some(buf) = &buf {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &self.target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
        self.queue.submit([encoder.finish()]);
        self.timer.after_submit();
        let buf = buf?;
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let data = buf.slice(..).get_mapped_range().ok()?;
        let mut img = image::RgbaImage::new(w, h);
        for y in 0..h {
            let line = &data[(y * row) as usize..(y * row + w * 4) as usize];
            img.as_mut()[(y * w * 4) as usize..((y + 1) * w * 4) as usize].copy_from_slice(line);
        }
        Some(img)
    }
}

fn main() -> anyhow::Result<()> {
    let Some(a) = parse_args()? else { return Ok(()) };
    // Read this checkout's visuals and shaders, never the user's folder,
    // with a fixed instance: before any other thread starts.
    unsafe {
        std::env::set_var("AUDIOVIZ_REPO", repo());
        std::env::set_var("AUDIOVIZ_DEFAULTS", "1");
        std::env::set_var("AUDIOVIZ_SEED", a.seed.to_string());
    }
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"))).init();
    let started = Instant::now();
    let settings = settings(&a)?;
    if audioviz::visuals::get(&a.visual).is_none() {
        bail!("no visual \"{}\" in {}", a.visual, repo().join("assets").join("visuals").display());
    }

    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    desc.backends = if a.dx12 { wgpu::Backends::DX12 } else { wgpu::Backends::VULKAN };
    let instance = wgpu::Instance::new(desc);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: None, ..Default::default() })).context("no GPU adapter")?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("audioviz lab"),
        required_features: GpuTimer::features(&adapter),
        required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .context("request device")?;
    let (w, h) = a.size;
    let shaders = ShaderSet::load();
    let director = Director::new(&device, &shaders, w, h, &settings).map_err(|e| anyhow::anyhow!("director: {e}"))?;
    let post = Post::new(&device, FORMAT, w, h, &shaders).map_err(|e| anyhow::anyhow!("post: {e}"))?;
    let timer = GpuTimer::new(&device, &queue);
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
    let mut lab = Lab { device, queue, shaders, director, post, timer, settings, target, view, size: a.size, features: FeatureFrame::default() };

    // Wait for the visual's build (on the engine's worker).
    let build = Instant::now();
    while !lab.director.layers[0].ready() {
        if build.elapsed() > Duration::from_secs(120) {
            bail!("{} did not build in 120 s", a.visual);
        }
        std::thread::sleep(Duration::from_millis(10));
        lab.director.poll(&lab.device, &lab.shaders);
    }
    if let Some(e) = &lab.director.last_error {
        bail!("{e}");
    }
    let built = build.elapsed();

    // Run to `at` on a 60 Hz clock, then hold there.
    let steps = (a.at * 60.0).round().max(1.0) as u32;
    for k in 0..steps {
        lab.frame(k as f32 / 60.0, 1.0 / 60.0, false);
    }
    let held = steps as f32 / 60.0;
    let mut gpu = Vec::new();
    std::fs::create_dir_all(&a.out)?;
    let name = a.name.clone().unwrap_or_else(|| a.visual.clone());
    let mut tiles = Vec::new();
    for &p in &a.passes {
        for (sname, nudge) in &a.shots {
            lab.director.camera_nudge = *nudge;
            lab.director.pass = p;
            // A few frames, so whatever reads last frame's camera has it.
            for _ in 0..3 {
                lab.frame(held, 0.0, false);
            }
            let img = lab.frame(held, 0.0, true).context("read back")?;
            gpu.push(lab.timer.frame_ms);
            let file = a.out.join(format!("{name}_{sname}_{}.png", pass_name(p)));
            img.save(&file)?;
            tiles.push((format!("{sname} {}", pass_name(p)), img));
        }
    }
    let sheet = a.out.join(format!("{name}_sheet.png"));
    contact_sheet(&tiles, a.shots.len()).save(&sheet)?;
    gpu.sort_by(f32::total_cmp);
    println!("{}: built in {:.1} s, held at {held:.2} s, {} shots, frame {:.2} ms on the GPU, {:.1} s in all", a.visual, built.as_secs_f32(), tiles.len(), gpu.get(gpu.len() / 2).copied().unwrap_or(0.0), started.elapsed().as_secs_f32());
    println!("{}", sheet.display());
    Ok(())
}

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
