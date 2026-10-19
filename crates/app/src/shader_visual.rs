//! Visuals loaded from a folder: a `visual.json` manifest (names, credit,
//! up to sixteen parameters) and a `visual.wgsl` holding one function,
//! `fn shade(uv: vec2<f32>) -> vec3<f32>`, written against the API in
//! `shaders/visual_api.wgsl` (time, a music clock, bands, pitch classes,
//! beats, onsets, the waveform, the layer's palette and tint, the
//! parameters by name, optionally last frame). The panel is built from the
//! manifest. Both files reload when saved; a broken shader keeps the last
//! good one drawing and shows its error in the panel.
//!
//! Folders live in `<config>/audioviz/visuals/<name>/`. The factory ones
//! (`assets/visuals/`) are written there on first run like presets and
//! scenes; a dev run reads the repo's `assets/visuals/` directly (debug
//! builds) or the embedded copies, and never writes.
//!
//! A shader runs inside wgpu's validation: it can draw, nothing else (no
//! files, no network), so a visual folder is safe to share.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use bytemuck::{Pod, Zeroable};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::factory;
use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, fullscreen_pass, hdr_texture};
use crate::settings::Layer;
use crate::shaders::ShaderSet;

pub const MAX_PARAMS: usize = 16;
/// Waveform samples a loaded visual sees.
const WAVE: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    #[default]
    Slider,
    Integer,
    Toggle,
    Choice,
}

/// One control in `visual.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct ParamDef {
    /// Name in the shader (`params().radius`) and in saved files.
    pub id: String,
    pub label: String,
    #[serde(rename = "type", default)]
    pub kind: ParamKind,
    /// Panel group heading; params with the same group sit together.
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub min: f32,
    #[serde(default = "one")]
    pub max: f32,
    /// A number, a bool for a toggle, or an option's name for a choice.
    #[serde(default)]
    pub default: Value,
    /// Choice options, shown as written; the shader sees the index.
    #[serde(default)]
    pub options: Vec<String>,
    /// Logarithmic slider.
    #[serde(default)]
    pub log: bool,
    /// Shown after the value ("px", "°/s").
    #[serde(default)]
    pub unit: String,
    /// Tooltip.
    #[serde(default)]
    pub help: String,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
struct Manifest {
    id: String,
    name: String,
    #[serde(default)]
    short: String,
    #[serde(default)]
    summary: String,
    /// Who the idea or code comes from, and its licence.
    #[serde(default)]
    credit: String,
    /// The panel's closing hint.
    #[serde(default)]
    hint: String,
    #[serde(default)]
    params: Vec<ParamDef>,
    /// Keep last frame's picture for `prev(uv)`.
    #[serde(default)]
    feedback: bool,
}

/// A loaded visual, as the registry holds it.
#[derive(Debug, Clone)]
pub struct ShaderDef {
    pub id: String,
    pub name: String,
    pub short: String,
    pub summary: String,
    pub credit: String,
    pub hint: String,
    pub params: Vec<ParamDef>,
    pub feedback: bool,
    pub wgsl: String,
    /// The folder it came from, watched for changes (none for embedded copies).
    pub dir: Option<PathBuf>,
}

const RESERVED: &[&str] = &["array", "bool", "break", "case", "const", "continue", "default", "discard", "else", "enable", "f32", "fn", "for", "i32", "if", "let", "loop", "mat2x2", "override", "ptr", "return", "struct", "switch", "true", "false", "type", "u32", "var", "vec2", "vec3", "vec4", "while", "target", "patch", "self", "super", "static", "move", "mod", "macro", "texture", "sampler", "uniform", "storage", "private", "function", "workgroup"];

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some('a'..='z' | '_')) && c.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') && !RESERVED.contains(&s)
}

fn parse(json: &str, wgsl: &str, dir: Option<PathBuf>) -> Result<ShaderDef, String> {
    let m: Manifest = serde_json::from_str(json).map_err(|e| format!("visual.json: {e}"))?;
    if m.id.is_empty() || !m.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
        return Err(format!("visual.json: id \"{}\" must be lowercase letters, digits, - or _", m.id));
    }
    if m.params.len() > MAX_PARAMS {
        return Err(format!("visual.json: at most {MAX_PARAMS} params"));
    }
    let mut seen = std::collections::HashSet::new();
    for p in &m.params {
        if !is_ident(&p.id) {
            return Err(format!("visual.json: param id \"{}\" must be a lowercase identifier (and not a WGSL word)", p.id));
        }
        if !seen.insert(p.id.as_str()) {
            return Err(format!("visual.json: param \"{}\" twice", p.id));
        }
        if p.kind == ParamKind::Choice && p.options.is_empty() {
            return Err(format!("visual.json: choice \"{}\" has no options", p.id));
        }
    }
    let short = if m.short.is_empty() { m.name.clone() } else { m.short };
    Ok(ShaderDef { id: m.id, name: m.name, short, summary: m.summary, credit: m.credit, hint: m.hint, params: m.params, feedback: m.feedback, wgsl: wgsl.to_string(), dir })
}

// ------------------------------------------------------------ values

impl ParamDef {
    /// The stored value or the default, as the panel and files hold it.
    fn value(&self, stored: Option<&Value>) -> Value {
        let v = stored.cloned().unwrap_or(Value::Null);
        match self.kind {
            ParamKind::Toggle => Value::Bool(v.as_bool().or(self.default.as_bool()).unwrap_or(false)),
            ParamKind::Choice => {
                let name = v.as_str().or(self.default.as_str()).map(str::to_lowercase);
                let i = name.and_then(|n| self.options.iter().position(|o| o.to_lowercase() == n)).unwrap_or(0);
                Value::String(self.options[i].to_lowercase())
            }
            ParamKind::Integer => Value::from(v.as_f64().or(self.default.as_f64()).unwrap_or(self.min as f64).round()),
            ParamKind::Slider => Value::from(v.as_f64().or(self.default.as_f64()).unwrap_or(self.min as f64)),
        }
    }

    /// What the shader sees.
    fn number(&self, stored: Option<&Value>) -> f32 {
        match self.value(stored) {
            Value::Bool(b) => b as u32 as f32,
            Value::String(s) => self.options.iter().position(|o| o.to_lowercase() == s).unwrap_or(0) as f32,
            v => v.as_f64().unwrap_or(0.0) as f32,
        }
    }
}

impl ShaderDef {
    /// Full parameters from what a layer stores, defaults filled in.
    pub fn params_json(&self, stored: Option<&Value>) -> Value {
        let obj = stored.and_then(Value::as_object);
        Value::Object(self.params.iter().map(|p| (p.id.clone(), p.value(obj.and_then(|o| o.get(&p.id))))).collect())
    }

    /// The WGSL the engine adds between the API and the visual's code.
    fn generated(&self) -> String {
        let mut s = String::from("// Generated from visual.json.\nstruct Params {\n");
        for p in &self.params {
            s.push_str(&format!("    {}: f32,\n", p.id));
        }
        if self.params.is_empty() {
            s.push_str("    none: f32,\n");
        }
        s.push_str("};\n\nfn params() -> Params {\n    return Params(");
        let args: Vec<String> = if self.params.is_empty() { vec!["0.0".into()] } else { (0..self.params.len()).map(|i| format!("param({i}u)")).collect() };
        s.push_str(&args.join(", "));
        s.push_str(");\n}\n");
        s
    }

    /// The text in front of the visual's own code (its first line is the
    /// next one), so errors can point into visual.wgsl.
    fn prefix(&self, shaders: &ShaderSet) -> String {
        format!("{}\n{}\n// ---- visual.wgsl ----\n", shaders.source("visual_api.wgsl"), self.generated())
    }

    fn source(&self, shaders: &ShaderSet, wgsl: &str) -> String {
        format!(
            "{}{}\n\n@fragment\nfn fs_main(in: FsIn) -> @location(0) vec4<f32> {{\n    let c = shade(in.uv);\n    // One NaN pixel would black out the layer's levelling: drop it.\n    let ok = all(c == c);\n    return vec4<f32>(select(vec3<f32>(0.0), max(c, vec3<f32>(0.0)), ok), 1.0);\n}}\n",
            self.prefix(shaders),
            wgsl
        )
    }
}

/// Rewrite the compiler's `wgsl:LINE:COL` positions, which count the whole
/// composed shader, as positions in visual.wgsl (or the engine's part).
fn visual_lines(err: &str, offset: usize) -> String {
    let mut out = String::with_capacity(err.len());
    let mut rest = err;
    while let Some(at) = rest.find("wgsl:") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 5..];
        let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
        match digits.parse::<usize>() {
            Ok(line) if line > offset => out.push_str(&format!("visual.wgsl line {}, col", line - offset)),
            Ok(_) => out.push_str("the engine's part, line"),
            Err(_) => {
                out.push_str("wgsl:");
                rest = tail;
                continue;
            }
        }
        rest = &tail[digits.len()..];
        if let Some(r) = rest.strip_prefix(':') {
            out.push(' ');
            rest = r;
        }
    }
    out.push_str(rest);
    out
}

// ------------------------------------------------------------ discovery

fn folder() -> Option<PathBuf> {
    factory::dir("visuals")
}

/// Debug builds on a dev run read the repo's folder, so editing a factory
/// visual reloads live.
fn repo_folder() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/visuals");
    d.is_dir().then_some(d)
}

fn read_folder(root: &Path, watch: bool) -> Vec<ShaderDef> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else { return out };
    let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for d in dirs {
        let (Ok(json), Ok(wgsl)) = (std::fs::read_to_string(d.join("visual.json")), std::fs::read_to_string(d.join("visual.wgsl"))) else { continue };
        match parse(&json, &wgsl, watch.then(|| d.clone())) {
            Ok(def) => out.push(def),
            Err(e) => tracing::warn!("visual {}: {e}", d.display()),
        }
    }
    out
}

fn embedded() -> Vec<ShaderDef> {
    let mut by_folder: BTreeMap<&str, (Option<&str>, Option<&str>)> = BTreeMap::new();
    for (rel, text) in factory::embedded("visuals/") {
        let mut parts = rel.split('/');
        let (_, Some(f), Some(file)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let e = by_folder.entry(f).or_default();
        match file {
            "visual.json" => e.0 = Some(text),
            "visual.wgsl" => e.1 = Some(text),
            _ => {}
        }
    }
    by_folder.into_iter().filter_map(|(f, (j, w))| parse(j?, w?, None).map_err(|e| tracing::warn!("factory visual {f}: {e}")).ok()).collect()
}

/// Every loaded visual: a dev run reads the repo (debug) or the embedded
/// copies; otherwise the factory folders are written into the user's
/// folder if missing and the folder is read.
pub fn discover() -> Vec<ShaderDef> {
    if factory::embedded_only() {
        return match repo_folder() {
            Some(d) => read_folder(&d, true),
            None => embedded(),
        };
    }
    if let Err(e) = factory::install("visuals/", false) {
        tracing::warn!("could not install factory visuals: {e}");
    }
    folder().map(|d| read_folder(&d, true)).unwrap_or_default()
}

// ------------------------------------------------------------ the scene

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    resolution: [f32; 2],
    time: f32,
    dt: f32,
    flow: f32,
    energy: f32,
    bass: f32,
    mid: f32,
    treble: f32,
    beat: f32,
    beat_phase: f32,
    beat_count: f32,
    bpm: f32,
    onset: f32,
    swell: f32,
    key: f32,
    onsets: [f32; 4],
    tint: [f32; 4],
    palette_mix: f32,
    palette_shift: f32,
    quiet: f32,
    _pad: f32,
    p: [[f32; 4]; 4],
    bands: [[f32; 4]; 8],
    raw: [[f32; 4]; 8],
    chroma: [[f32; 4]; 3],
    palette: crate::color::Table,
}

pub struct ShaderVisual {
    def: &'static ShaderDef,
    /// The manifest and code in use (reloaded from the folder when it changes).
    live: ShaderDef,
    error: Option<String>,
    stamp: Option<(SystemTime, SystemTime)>,
    last_poll: Instant,
    layout: wgpu::BindGroupLayout,
    pipeline: Option<wgpu::RenderPipeline>,
    uniform: wgpu::Buffer,
    wave_buf: wgpu::Buffer,
    sampler: wgpu::Sampler,
    out: [wgpu::TextureView; 2],
    black: wgpu::TextureView,
    /// Bind groups by what `prev` reads: black, `out[0]`, `out[1]`.
    groups: Vec<wgpu::BindGroup>,
    cur: usize,
    bands: [f32; 32],
    flow: f32,
    swell: f32,
    ring: VecDeque<[f32; 2]>,
}

fn stamp(dir: &Path) -> Option<(SystemTime, SystemTime)> {
    let m = |f: &str| std::fs::metadata(dir.join(f)).and_then(|m| m.modified()).ok();
    Some((m("visual.json")?, m("visual.wgsl")?))
}

impl ShaderVisual {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet, def: &'static ShaderDef) -> Result<Self, String> {
        let entry = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loaded visual"),
            entries: &[
                entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                entry(1, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }),
                entry(2, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
                entry(3, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let mut me = Self {
            def,
            live: def.clone(),
            error: None,
            stamp: def.dir.as_deref().and_then(stamp),
            last_poll: Instant::now(),
            uniform: scene::uniform_buffer(device, &format!("{} params", def.id), std::mem::size_of::<Uniform>() as u64),
            wave_buf: device.create_buffer(&wgpu::BufferDescriptor { label: Some("loaded visual wave"), size: (WAVE * 8) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
            sampler: scene::linear_sampler(device),
            out: [hdr_texture(device, &format!("{} a", def.id), width, height), hdr_texture(device, &format!("{} b", def.id), width, height)],
            black: hdr_texture(device, "loaded visual black", 1, 1),
            groups: Vec::new(),
            layout,
            pipeline: None,
            cur: 0,
            bands: [0.0; 32],
            flow: 0.0,
            swell: 0.0,
            ring: VecDeque::with_capacity(WAVE * 2),
        };
        me.make_groups(device);
        me.build(device, shaders);
        Ok(me)
    }

    fn make_groups(&mut self, device: &wgpu::Device) {
        let group = |prev: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("loaded visual"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: self.wave_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(prev) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            })
        };
        self.groups = vec![group(&self.black), group(&self.out[0]), group(&self.out[1])];
    }

    /// Compile the live code; on failure keep the old pipeline and the error.
    fn build(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        let src = self.live.source(shaders, &self.live.wgsl);
        let layout = &self.layout;
        let id = self.live.id.clone();
        let built = scene::guarded(device, || {
            let m = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(&id), source: wgpu::ShaderSource::Wgsl(src.into()) });
            scene::pipeline(device, &id, layout, &m, HDR_FORMAT, None)
        });
        match built {
            Ok(p) => {
                self.pipeline = Some(p);
                self.error = None;
            }
            Err(e) => {
                let e = self::visual_lines(&e, self.live.prefix(shaders).lines().count());
                tracing::warn!("visual {}: {e}", self.live.id);
                self.error = Some(e);
            }
        }
    }

    /// Reload from the folder when either file changed.
    fn reload(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        let Some(dir) = self.def.dir.clone() else { return };
        let now = stamp(&dir);
        if now.is_none() || now == self.stamp {
            return;
        }
        self.stamp = now;
        let (Ok(json), Ok(wgsl)) = (std::fs::read_to_string(dir.join("visual.json")), std::fs::read_to_string(dir.join("visual.wgsl"))) else { return };
        match parse(&json, &wgsl, Some(dir)) {
            Ok(mut d) => {
                // The id is what saved files point at; it stays the registered one.
                d.id = self.def.id.clone();
                self.live = d;
                self.build(device, shaders);
            }
            Err(e) => self.error = Some(e),
        }
    }
}

impl Scene for ShaderVisual {
    fn id(&self) -> &'static str {
        &self.def.id
    }

    fn name(&self) -> &'static str {
        &self.def.name
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.out = [hdr_texture(device, &format!("{} a", self.def.id), width, height), hdr_texture(device, &format!("{} b", self.def.id), width, height)];
        self.make_groups(device);
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.build(device, shaders);
        self.error.clone().map_or(Ok(()), Err)
    }

    fn poll(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        if self.def.dir.is_some() && self.last_poll.elapsed().as_millis() > 400 {
            self.last_poll = Instant::now();
            self.reload(device, shaders);
        }
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let f = ctx.features;
        let dt = ctx.dt;
        let energy = ((f.bass_att + f.mid_att + f.treb_att) / 3.0).clamp(0.0, 2.5);
        self.flow += dt * (0.5 + 0.5 * energy);
        let want = (f.bass_att - 1.0).max(0.0) * 0.8 + f.onset_pulse[0].min(1.0) * 0.6;
        self.swell += (want - self.swell) * (1.0 - (-dt / if want > self.swell { 0.04 } else { 0.35 }).exp());
        let (attack, release) = (1.0 - (-dt / 0.03).exp(), 1.0 - (-dt / 0.2).exp());
        let mut raw = [[0.0f32; 4]; 8];
        let mut smooth = [[0.0f32; 4]; 8];
        for i in 0..32 {
            let v = f.bands.get(i).copied().unwrap_or(0.0).max(0.0);
            let c = self.bands[i];
            self.bands[i] = c + (v - c) * if v > c { attack } else { release };
            raw[i / 4][i % 4] = v;
            smooth[i / 4][i % 4] = self.bands[i];
        }
        let mut chroma = [[0.0f32; 4]; 3];
        for (i, c) in f.chroma.iter().enumerate() {
            chroma[i / 4][i % 4] = *c;
        }
        let stored = layer.params.get(&self.def.id).and_then(Value::as_object);
        let mut p = [[0.0f32; 4]; 4];
        for (i, d) in self.live.params.iter().enumerate().take(MAX_PARAMS) {
            p[i / 4][i % 4] = d.number(stored.and_then(|o| o.get(&d.id)));
        }
        let u = Uniform {
            resolution: [ctx.width as f32, ctx.height as f32],
            time: ctx.time,
            dt,
            flow: self.flow,
            energy,
            bass: f.bass_att,
            mid: f.mid_att,
            treble: f.treb_att,
            beat: f.beat_pulse * f.beat_confidence,
            beat_phase: f.beat_phase,
            beat_count: f.beat_count as f32,
            bpm: f.bpm,
            onset: f.onset_pulse[3].min(1.0),
            swell: self.swell,
            key: f.key_class as f32,
            onsets: f.onset_pulse.map(|o| o.min(1.0)),
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], 1.0],
            palette_mix: ctx.palette_amount,
            palette_shift: ctx.palette_shift,
            quiet: f.quiet as u32 as f32,
            _pad: 0.0,
            p,
            bands: smooth,
            raw,
            chroma,
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        let gain = ctx.auto_gain;
        for s in ctx.points {
            self.ring.push_back([s[0] * gain, s[1] * gain]);
        }
        while self.ring.len() > WAVE {
            self.ring.pop_front();
        }
        let mut wave = vec![[0.0f32; 2]; WAVE];
        let off = WAVE - self.ring.len();
        for (i, s) in self.ring.iter().enumerate() {
            wave[off + i] = *s;
        }
        queue.write_buffer(&self.wave_buf, 0, bytemuck::cast_slice(&wave));
        let Some(pipeline) = &self.pipeline else {
            scene::clear_pass(encoder, "loaded visual (no shader)", &self.out[self.cur]);
            return;
        };
        // With feedback the two outputs ping-pong and `prev` reads the other.
        let (write, group) = if self.live.feedback { (1 - self.cur, &self.groups[1 + self.cur]) } else { (self.cur, &self.groups[0]) };
        fullscreen_pass(encoder, &self.def.id, &self.out[write], wgpu::LoadOp::Clear(wgpu::Color::BLACK), pipeline, group);
        self.cur = write;
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.out[self.cur]
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        use crate::widgets::{dropdown, ghost_button, group, hint, segmented, slider_def, small_caps, toggle_row};
        if let Some(e) = &self.error {
            ui.label(egui::RichText::new(format!("Shader error, showing the last good version:\n{e}")).color(crate::theme::WARN).size(11.5));
            ui.add_space(6.0);
        }
        let id = self.def.id.clone();
        let mut values: Map<String, Value> = match self.def_params(layer) {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        let mut groups: Vec<&str> = Vec::new();
        for p in &self.live.params {
            if !groups.contains(&p.group.as_str()) {
                groups.push(p.group.as_str());
            }
        }
        for g in groups {
            let title = if g.is_empty() { "Controls" } else { g };
            let members: Vec<&ParamDef> = self.live.params.iter().filter(|p| p.group == g).collect();
            group(ui, &format!("{id}.{title}"), title, true, members.len(), |ui| {
                for p in members {
                    let v = values.get(&p.id).cloned().unwrap_or(Value::Null);
                    let def = p.value(Some(&p.default));
                    match p.kind {
                        ParamKind::Toggle => {
                            let mut on = v.as_bool().unwrap_or(false);
                            let r = toggle_row(ui, &p.label, &mut on);
                            if !p.help.is_empty() {
                                r.on_hover_text(&p.help);
                            }
                            values.insert(p.id.clone(), Value::Bool(on));
                        }
                        ParamKind::Choice => {
                            let cur = v.as_str().and_then(|s| p.options.iter().position(|o| o.to_lowercase() == s)).unwrap_or(0);
                            let labels: Vec<&str> = p.options.iter().map(String::as_str).collect();
                            let pick = if labels.len() <= 3 {
                                small_caps(ui, &p.label);
                                segmented(ui, &labels, cur)
                            } else {
                                dropdown(ui, &p.label, &labels, Some(cur))
                            };
                            if let Some(i) = pick {
                                values.insert(p.id.clone(), Value::String(p.options[i].to_lowercase()));
                            }
                        }
                        ParamKind::Integer | ParamKind::Slider => {
                            let int = p.kind == ParamKind::Integer;
                            let mut x = v.as_f64().unwrap_or(0.0) as f32;
                            let d = def.as_f64().unwrap_or(0.0) as f32;
                            let span = (p.max - p.min).abs();
                            let unit = if p.unit.is_empty() { String::new() } else { format!(" {}", p.unit) };
                            let r = slider_def(ui, &p.label, &mut x, Some(d), p.min..=p.max, move |v| if int || span > 20.0 { format!("{v:.0}{unit}") } else if span > 2.0 { format!("{v:.1}{unit}") } else { format!("{v:.2}{unit}") }, p.log);
                            if !p.help.is_empty() {
                                r.on_hover_text(&p.help);
                            }
                            let x = if int { x.round() } else { x };
                            values.insert(p.id.clone(), Value::from(x as f64));
                        }
                    }
                }
            });
        }
        if !self.live.hint.is_empty() {
            hint(ui, &self.live.hint);
        }
        if !self.live.credit.is_empty() {
            hint(ui, &self.live.credit);
        }
        if let Some(d) = &self.def.dir {
            hint(ui, &format!("Loaded from {}; save either file and it reloads.", d.display()));
        }
        ui.add_space(6.0);
        if ghost_button(ui, &format!("Reset {}", self.live.short.to_lowercase()), false).clicked() {
            values = Map::new();
        }
        layer.params.insert(id, Value::Object(values));
    }
}

impl ShaderVisual {
    fn def_params(&self, layer: &Layer) -> Value {
        self.live.params_json(layer.params.get(&self.def.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{"id": "test", "name": "Test", "params": [
        {"id": "radius", "label": "Radius", "min": 0.1, "max": 2.0, "default": 0.5},
        {"id": "count", "label": "Count", "type": "integer", "min": 1, "max": 9, "default": 4},
        {"id": "mirror", "label": "Mirror", "type": "toggle", "default": true},
        {"id": "print", "label": "Print", "type": "choice", "options": ["Contours", "Dots"], "default": "dots"}]}"#;

    #[test]
    fn a_manifest_parses_and_fills_defaults() {
        let d = parse(JSON, "fn shade(uv: vec2<f32>) -> vec3<f32> { return vec3<f32>(0.0); }", None).unwrap();
        let full = d.params_json(None);
        assert_eq!(full["radius"], 0.5);
        assert_eq!(full["count"], 4.0);
        assert_eq!(full["mirror"], true);
        assert_eq!(full["print"], "dots");
        assert_eq!(d.params[3].number(Some(&Value::String("dots".into()))), 1.0);
        let stored = serde_json::json!({"radius": 1.5});
        assert_eq!(d.params_json(Some(&stored))["radius"], 1.5);
        assert!(d.generated().contains("fn params() -> Params"));
    }

    #[test]
    fn error_positions_point_into_the_visual() {
        assert_eq!(visual_lines("x wgsl:593:5 y", 590), "x visual.wgsl line 3, col 5 y");
        assert_eq!(visual_lines("wgsl:12:1", 590), "the engine's part, line 1");
    }

    #[test]
    fn bad_manifests_are_refused() {
        assert!(parse(r#"{"id": "Bad Id", "name": "x"}"#, "", None).is_err());
        assert!(parse(r#"{"id": "x", "name": "x", "params": [{"id": "let", "label": "x"}]}"#, "", None).is_err());
        assert!(parse(r#"{"id": "x", "name": "x", "params": [{"id": "a", "label": "x"}, {"id": "a", "label": "y"}]}"#, "", None).is_err());
    }

    #[test]
    fn uniform_layout_matches_wgsl() {
        assert_eq!(std::mem::offset_of!(Uniform, onsets), 64);
        assert_eq!(std::mem::offset_of!(Uniform, p), 112);
        assert_eq!(std::mem::offset_of!(Uniform, bands), 176);
        assert_eq!(std::mem::offset_of!(Uniform, chroma), 432);
        assert_eq!(std::mem::offset_of!(Uniform, palette), 480);
    }

    #[test]
    fn factory_visuals_parse() {
        for d in embedded() {
            assert!(!d.wgsl.is_empty(), "{}", d.id);
        }
    }
}
