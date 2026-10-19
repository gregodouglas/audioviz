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
/// Most items a prepare step may compute.
pub const MAX_ITEMS: u32 = 16384;
/// Most triangles a loaded visual may draw.
pub const MAX_TRIANGLES: u32 = 1 << 20;
/// Most points a loaded visual's line may have.
pub const MAX_LINE: u32 = 65536;
const PREPARE_GROUP: u32 = 64;
/// Waveform samples a loaded visual sees (`wave(t)` spans the newest 1024).
const WAVE: usize = 4096;
/// The album art's size on the GPU (square; the art is resampled to it).
const ART: u32 = 512;

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
    /// What a choice saves for each option (default: the option, lowercase).
    #[serde(default)]
    pub values: Vec<String>,
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
    /// How many items `prepare(i)` computes each frame (0 = no prepare step).
    #[serde(default)]
    items: u32,
    /// How many points `point(i)` gives a line drawn over the picture (0 = no line).
    #[serde(default)]
    line: u32,
    /// A param whose value is how many of the line's points are drawn.
    #[serde(default)]
    line_count: String,
    /// "line" (the default) or "beam".
    #[serde(default)]
    line_style: String,
    /// The layer's band filters the waveform itself (as the scope does).
    #[serde(default)]
    band_waveform: bool,
    /// Triangles `vertex(i)` gives, drawn in order over the picture (0 = none).
    #[serde(default)]
    triangles: u32,
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
    /// Items computed by `prepare` once a frame (0 = none).
    pub items: u32,
    /// Points of the line `point` draws (0 = none).
    pub line: u32,
    /// The param that sets how many of them are drawn (empty = all).
    pub line_count: String,
    /// The code has `fn shade` (a line visual may leave it out).
    pub shade: bool,
    /// The line is a phosphor beam (Gaussian, dimmer when fast).
    pub beam: bool,
    /// Triangles drawn by `vertex(i)` (0 = none), and whether `paint` colours them.
    pub triangles: u32,
    pub paint: bool,
    pub band_waveform: bool,
    pub wgsl: String,
    /// The folder it came from, watched for changes (none for embedded copies).
    pub dir: Option<PathBuf>,
}

/// WGSL keywords and type names: never a control id.
const RESERVED: &[&str] = &["array", "bool", "break", "case", "const", "continue", "default", "discard", "else", "enable", "f32", "fn", "for", "i32", "if", "let", "loop", "mat2x2", "override", "ptr", "return", "struct", "switch", "true", "false", "type", "u32", "var", "vec2", "vec3", "vec4", "while", "texture", "sampler", "uniform", "storage", "private", "function", "workgroup"];

/// WGSL's reserved words. A control may carry one as its id, since saved
/// files use it (Loop's "smooth"); in the shader it is `id_`.
const WGSL_RESERVED: &[&str] = &["alias", "attribute", "active", "asm", "auto", "await", "become", "binding_array", "cast", "catch", "class", "co_await", "co_return", "co_yield", "coherent", "column_major", "common", "compile", "compile_fragment", "concept", "const_cast", "consteval", "constexpr", "constinit", "crate", "debugger", "decltype", "delete", "demote", "demote_to_helper", "do", "dynamic_cast", "enum", "explicit", "export", "extends", "extern", "external", "fallthrough", "filter", "final", "finally", "friend", "from", "fxgroup", "get", "goto", "groupshared", "highp", "impl", "implements", "import", "inline", "instanceof", "interface", "layout", "lowp", "macro", "macro_rules", "match", "mediump", "meta", "mod", "module", "move", "mut", "mutable", "namespace", "new", "nil", "noexcept", "noinline", "nointerpolation", "noperspective", "null", "nullptr", "of", "operator", "package", "packoffset", "partition", "pass", "patch", "pixelfragment", "precise", "precision", "premerge", "priv", "protected", "pub", "public", "readonly", "ref", "regardless", "register", "reinterpret_cast", "require", "resource", "restrict", "self", "set", "shared", "sizeof", "smooth", "snorm", "static", "static_assert", "static_cast", "std", "subroutine", "super", "target", "template", "this", "thread_local", "throw", "trait", "try", "typedef", "typeid", "typename", "typeof", "union", "unless", "unorm", "unsafe", "unsized", "use", "using", "varying", "virtual", "volatile", "wgsl", "where", "with", "writeonly", "yield", "flat", "linear", "perspective", "centroid"];

/// A control's name in the shader: its id, or `id_` for a WGSL reserved word.
fn field(id: &str) -> String {
    if WGSL_RESERVED.contains(&id) { format!("{id}_") } else { id.to_string() }
}

/// Whether the code defines `fn <name>(`, ignoring line comments.
fn defines(wgsl: &str, name: &str) -> bool {
    wgsl.lines().map(|l| l.split("//").next().unwrap_or("")).any(|l| {
        let t = l.trim_start();
        t.strip_prefix("fn").is_some_and(|r| r.starts_with(char::is_whitespace) && r.trim_start().strip_prefix(name).is_some_and(|r| r.trim_start().starts_with('(')))
    })
}

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
        if !p.values.is_empty() && p.values.len() != p.options.len() {
            return Err(format!("visual.json: choice \"{}\" needs one value per option", p.id));
        }
    }
    if m.items > MAX_ITEMS {
        return Err(format!("visual.json: at most {MAX_ITEMS} items"));
    }
    match (defines(wgsl, "prepare"), m.items > 0) {
        (true, false) => return Err("visual.wgsl has fn prepare: set \"items\" in visual.json to how many it computes".into()),
        (false, true) => return Err("visual.json sets \"items\" but visual.wgsl has no fn prepare(i: u32) -> vec4<f32>".into()),
        _ => {}
    }
    if m.line > MAX_LINE {
        return Err(format!("visual.json: a line of at most {MAX_LINE} points"));
    }
    match (defines(wgsl, "point"), m.line > 0) {
        (true, false) => return Err("visual.wgsl has fn point: set \"line\" in visual.json to how many points the line has".into()),
        (false, true) => return Err("visual.json sets \"line\" but visual.wgsl has no fn point(i: u32) -> LinePoint".into()),
        (true, true) if m.line < 2 => return Err("visual.json: a line needs at least 2 points".into()),
        _ => {}
    }
    if !matches!(m.line_style.as_str(), "" | "line" | "beam") {
        return Err(format!("visual.json: line_style \"{}\" is not \"line\" or \"beam\"", m.line_style));
    }
    if !defines(wgsl, "shade") && m.line == 0 && m.triangles == 0 {
        return Err("visual.wgsl needs fn shade(uv: vec2<f32>) -> vec3<f32> (only a visual with a line or triangles may leave it out)".into());
    }
    if m.triangles > MAX_TRIANGLES {
        return Err(format!("visual.json: at most {MAX_TRIANGLES} triangles"));
    }
    match (defines(wgsl, "vertex"), m.triangles > 0) {
        (true, false) => return Err("visual.wgsl has fn vertex: set \"triangles\" in visual.json to how many it draws".into()),
        (false, true) => return Err("visual.json sets \"triangles\" but visual.wgsl has no fn vertex(i: u32) -> MeshVertex".into()),
        _ => {}
    }
    if !m.line_count.is_empty() && !m.params.iter().any(|p| p.id == m.line_count && matches!(p.kind, ParamKind::Integer | ParamKind::Slider)) {
        return Err(format!("visual.json: line_count \"{}\" is not a number param", m.line_count));
    }
    let short = if m.short.is_empty() { m.name.clone() } else { m.short };
    Ok(ShaderDef { id: m.id, name: m.name, short, summary: m.summary, credit: m.credit, hint: m.hint, params: m.params, feedback: m.feedback, items: m.items, line: m.line, line_count: m.line_count, shade: defines(wgsl, "shade"), beam: m.line_style == "beam", triangles: m.triangles, paint: defines(wgsl, "paint"), band_waveform: m.band_waveform, wgsl: wgsl.to_string(), dir })
}

// ------------------------------------------------------------ values

impl ParamDef {
    /// What option `i` of a choice saves.
    fn choice_value(&self, i: usize) -> String {
        self.values.get(i).cloned().unwrap_or_else(|| self.options[i].to_lowercase())
    }

    /// The option a saved value (or an option's name) stands for.
    fn choice_index(&self, s: &str) -> Option<usize> {
        let s = s.to_lowercase();
        (0..self.options.len()).find(|&i| self.choice_value(i).to_lowercase() == s || self.options[i].to_lowercase() == s)
    }

    /// The stored value or the default, as the panel and files hold it.
    fn value(&self, stored: Option<&Value>) -> Value {
        let v = stored.cloned().unwrap_or(Value::Null);
        match self.kind {
            ParamKind::Toggle => Value::Bool(v.as_bool().or(self.default.as_bool()).unwrap_or(false)),
            ParamKind::Choice => {
                let name = v.as_str().or(self.default.as_str()).map(str::to_lowercase);
                let i = name.and_then(|n| self.choice_index(&n)).unwrap_or(0);
                Value::String(self.choice_value(i))
            }
            // Whole numbers stay whole in saved files ("count": 64, not 64.0).
            ParamKind::Integer => Value::from(v.as_f64().or(self.default.as_f64()).unwrap_or(self.min as f64).round() as i64),
            ParamKind::Slider => Value::from(v.as_f64().or(self.default.as_f64()).unwrap_or(self.min as f64)),
        }
    }

    /// What the shader sees.
    fn number(&self, stored: Option<&Value>) -> f32 {
        match self.value(stored) {
            Value::Bool(b) => b as u32 as f32,
            Value::String(s) => self.choice_index(&s).unwrap_or(0) as f32,
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
        let mut s = format!("// Generated from visual.json.\nconst ITEMS: u32 = {}u;\nconst LINE_POINTS: u32 = {}u;\n\nstruct Params {{\n", self.items, self.line);
        for p in &self.params {
            s.push_str(&format!("    {}: f32,\n", field(&p.id)));
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
        format!("{}\n{}\n// ---- visual.wgsl ----\n", shaders.source_base("visual_api.wgsl"), self.generated())
    }

    fn source(&self, shaders: &ShaderSet, wgsl: &str) -> String {
        // The prepare step's entry point goes after the visual's code, so
        // error lines in visual.wgsl keep their offset.
        // Both write group 1, binding 0: one pipeline uses each.
        let mut prepare = String::new();
        if self.items > 0 || self.line > 0 {
            prepare.push_str("\n@group(1) @binding(0) var<storage, read_write> items_out: array<vec4<f32>>;\n");
        }
        if self.items > 0 {
            prepare.push_str(&format!("\n@compute @workgroup_size({PREPARE_GROUP})\nfn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {{\n    if (id.x < ITEMS) {{\n        items_out[id.x] = prepare(id.x);\n    }}\n}}\n"));
        }
        if self.line > 0 {
            prepare.push_str(&format!("\n@compute @workgroup_size({PREPARE_GROUP})\nfn cs_line(@builtin(global_invocation_id) id: vec3<u32>) {{\n    if (id.x < LINE_POINTS) {{\n        let q = point(id.x);\n        items_out[2u * id.x] = vec4<f32>(q.pos, q.width, 0.0);\n        items_out[2u * id.x + 1u] = vec4<f32>(q.colour, 0.0);\n    }}\n}}\n"));
        }
        if self.triangles > 0 {
            let paint = if self.paint { "paint(f)" } else { "f.colour" };
            prepare.push_str(&format!(
                "\nstruct MeshOut {{\n    @builtin(position) clip: vec4<f32>,\n    @location(0) colour: vec4<f32>,\n    @location(1) data: vec4<f32>,\n}};\n\n@vertex\nfn vs_mesh(@builtin(vertex_index) vi: u32) -> MeshOut {{\n    let v = vertex(vi);\n    let short = min(V.resolution.x, V.resolution.y);\n    var o: MeshOut;\n    o.clip = vec4<f32>(v.pos.x * short / V.resolution.x, v.pos.y * short / V.resolution.y, 0.0, v.pos.w);\n    o.colour = v.colour;\n    o.data = v.data;\n    return o;\n}}\n\n@fragment\nfn fs_mesh(m: MeshOut) -> @location(0) vec4<f32> {{\n    var f: MeshFragment;\n    f.colour = m.colour;\n    f.data = m.data;\n    f.pixel = m.clip.xy;\n    let c = {paint};\n    // A NaN must not reach the levelling.\n    if (!all(c == c)) {{\n        return vec4<f32>(0.0);\n    }}\n    return vec4<f32>(max(c.rgb, vec3<f32>(0.0)), clamp(c.a, 0.0, 1.0));\n}}\n"
            ));
        }
        // A line visual may leave out shade(): the line pass clears instead.
        if !self.shade {
            prepare.push_str("\nfn shade(uv: vec2<f32>) -> vec3<f32> {\n    return vec3<f32>(0.0);\n}\n");
        }
        prepare_after(&prepare, format!(
            "{}{}\n\n@fragment\nfn fs_main(in: FsIn) -> @location(0) vec4<f32> {{\n    let c = shade(in.uv);\n    // One NaN pixel would black out the layer's levelling: drop it.\n    let ok = all(c == c);\n    return vec4<f32>(select(vec3<f32>(0.0), max(c, vec3<f32>(0.0)), ok), 1.0);\n}}\n",
            self.prefix(shaders),
            wgsl
        ))
    }
}

fn prepare_after(prepare: &str, mut s: String) -> String {
    s.push_str(prepare);
    s
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
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?.parent()?.join("assets").join("visuals");
    d.is_dir().then_some(d)
}

fn read_folder(root: &Path, watch: bool) -> Vec<ShaderDef> {
    read_folder_except(root, watch, &[])
}

/// `read_folder`, leaving out the folders in `skip` without reading them.
fn read_folder_except(root: &Path, watch: bool, skip: &[PathBuf]) -> Vec<ShaderDef> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else { return out };
    let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && !skip.contains(p)).collect();
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

pub fn embedded() -> Vec<ShaderDef> {
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

/// The folder a person adds visuals to: the user's, or on a debug dev run
/// the repo's (none on a release dev run, which reads embedded copies).
pub fn authoring_folder() -> Option<PathBuf> {
    if factory::embedded_only() { repo_folder() } else { folder() }
}

/// Visuals in folders that are not in `known` yet (added while running).
pub fn rescan(known: &[PathBuf]) -> Vec<ShaderDef> {
    let Some(root) = authoring_folder() else { return Vec::new() };
    read_folder_except(&root, true, known)
}

/// Write the shipped visuals back over the user's copies (and recreate any
/// that were deleted). Running ones reload from their files.
pub fn restore_factory() -> anyhow::Result<usize> {
    if factory::embedded_only() {
        anyhow::bail!("a dev run never writes into the audioviz folder");
    }
    factory::install("visuals/", true)
}

const TEMPLATE_JSON: &str = include_str!("../../../assets/visual-template/visual.json");
const TEMPLATE_WGSL: &str = include_str!("../../../assets/visual-template/visual.wgsl");

/// A folder name and id from a name: lowercase letters and digits joined by '-'.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "my-visual".into() } else { out }
}

/// Make a new visual from the starter template in the user's folder:
/// `<folder>/<slug>/visual.json` and `visual.wgsl`, with an id no other
/// visual or folder has. `taken` says whether an id is in use.
pub fn create(name: &str, taken: impl Fn(&str) -> bool) -> anyhow::Result<ShaderDef> {
    if factory::embedded_only() {
        anyhow::bail!("a dev run never writes into the audioviz folder");
    }
    let root = folder().ok_or_else(|| anyhow::anyhow!("no config folder"))?;
    create_in(&root, name, taken)
}

fn create_in(root: &Path, name: &str, taken: impl Fn(&str) -> bool) -> anyhow::Result<ShaderDef> {
    let base = slug(name);
    let (id, dir) = (1..1000)
        .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
        .map(|id| (id.clone(), root.join(&id)))
        .find(|(id, dir)| !taken(id) && !dir.exists())
        .ok_or_else(|| anyhow::anyhow!("no free folder name"))?;
    let title = name.trim();
    let title = if title.is_empty() { "My visual" } else { title };
    let mut manifest: Map<String, Value> = serde_json::from_str(TEMPLATE_JSON)?;
    manifest.insert("id".into(), Value::String(id.clone()));
    manifest.insert("name".into(), Value::String(title.to_string()));
    manifest.insert("short".into(), Value::String(title.to_string()));
    let json = serde_json::to_string_pretty(&manifest)? + "
";
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("visual.json"), &json)?;
    std::fs::write(dir.join("visual.wgsl"), TEMPLATE_WGSL)?;
    parse(&json, TEMPLATE_WGSL, Some(dir)).map_err(|e| anyhow::anyhow!(e))
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
    has_art: f32,
    p: [[f32; 4]; 4],
    bands: [[f32; 4]; 8],
    raw: [[f32; 4]; 8],
    chroma: [[f32; 4]; 3],
    palette: crate::color::Table,
    wave_new: f32,
    auto_gain: f32,
    sample_rate: f32,
    track: f32,
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
    /// The prepare step: its output layout, pipeline, the two item buffers
    /// (this frame's and last frame's) and a bind group writing each.
    out_layout: wgpu::BindGroupLayout,
    prepare: Option<wgpu::ComputePipeline>,
    items: [wgpu::Buffer; 2],
    items_len: u32,
    item_groups: Vec<wgpu::BindGroup>,
    /// Which item buffer holds the latest values.
    icur: usize,
    /// The line: its point pass, the points buffer (written by `point`,
    /// group 1 of that pass), and the engine's draw over the picture.
    line_cs: Option<wgpu::ComputePipeline>,
    line_points: wgpu::Buffer,
    line_len: u32,
    line_out_group: wgpu::BindGroup,
    line_uniform: wgpu::Buffer,
    line_layout: wgpu::BindGroupLayout,
    line_group: wgpu::BindGroup,
    line_draw: Option<wgpu::RenderPipeline>,
    /// The triangles' pipeline (premultiplied over the picture).
    mesh: Option<wgpu::RenderPipeline>,
    uniform: wgpu::Buffer,
    wave_buf: wgpu::Buffer,
    sampler: wgpu::Sampler,
    out: [wgpu::TextureView; 2],
    black: wgpu::TextureView,
    /// The album art, and the track id it was last filled for.
    art: wgpu::Texture,
    art_view: wgpu::TextureView,
    art_id: u64,
    has_art: bool,
    /// Bind groups by what `prev` reads (black, `out[0]`, `out[1]`) times
    /// which item buffer `item()` reads: index `prev * 2 + items`.
    groups: Vec<wgpu::BindGroup>,
    cur: usize,
    bands: [f32; 32],
    flow: f32,
    swell: f32,
    ring: VecDeque<[f32; 2]>,
}

fn art_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("loaded visual art"),
        size: wgpu::Extent3d { width: ART, height: ART, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// The art resampled to ART x ART, RGBA8.
fn art_pixels(a: &audioviz_metadata::ArtImage) -> Option<Vec<u8>> {
    if a.width == ART && a.height == ART {
        return Some(a.rgba.clone());
    }
    let img = image::RgbaImage::from_raw(a.width, a.height, a.rgba.clone())?;
    Some(image::imageops::resize(&img, ART, ART, image::imageops::FilterType::Triangle).into_raw())
}

/// Blending for triangles: premultiplied over, so alpha 0 adds light and
/// alpha 1 covers.
const PREMULTIPLIED_OVER: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
};

fn mesh_pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, module: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("loaded visual triangles"), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("loaded visual triangles"),
        layout: Some(&pl),
        vertex: wgpu::VertexState { module, entry_point: Some("vs_mesh"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module, entry_point: Some("fs_mesh"), targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(PREMULTIPLIED_OVER), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn line_buffer(device: &wgpu::Device, n: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some("loaded visual line"), size: n.max(1) as u64 * 32, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false })
}

/// The engine's line pipeline (`visual_lines.wgsl`), additive over the picture.
fn line_pipeline(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Option<wgpu::RenderPipeline> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "visual_lines.wgsl");
        scene::pipeline(device, "loaded visual line", layout, &m, HDR_FORMAT, Some(scene::ADDITIVE))
    })
    .map_err(|e| tracing::warn!("visual_lines.wgsl: {e}"))
    .ok()
}

/// The points buffer as the point pass writes it and as the draw reads it.
fn line_groups(device: &wgpu::Device, out_layout: &wgpu::BindGroupLayout, layout: &wgpu::BindGroupLayout, points: &wgpu::Buffer, uniform: &wgpu::Buffer) -> (wgpu::BindGroup, wgpu::BindGroup) {
    let out = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("loaded visual line out"), layout: out_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: points.as_entire_binding() }] });
    let draw = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("loaded visual line draw"),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: points.as_entire_binding() }],
    });
    (out, draw)
}

fn item_buffers(device: &wgpu::Device, n: u32) -> [wgpu::Buffer; 2] {
    let make = |label| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: n.max(1) as u64 * 16, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
    [make("loaded visual items a"), make("loaded visual items b")]
}

fn stamp(dir: &Path) -> Option<(SystemTime, SystemTime)> {
    let m = |f: &str| std::fs::metadata(dir.join(f)).and_then(|m| m.modified()).ok();
    Some((m("visual.json")?, m("visual.wgsl")?))
}

impl ShaderVisual {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet, def: &'static ShaderDef) -> Result<Self, String> {
        let entry = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::COMPUTE, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loaded visual"),
            entries: &[
                entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                entry(1, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }),
                entry(2, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
                entry(3, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                entry(4, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }),
                entry(5, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
            ],
        });
        let out_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loaded visual items out"),
            entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }, count: None }],
        });
        let art = art_texture(device);
        let line_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loaded visual line draw"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
            ],
        });
        let line_uniform = scene::uniform_buffer(device, "loaded visual line", 16);
        let line_points = line_buffer(device, def.line);
        let (line_out_group, line_group) = line_groups(device, &out_layout, &line_layout, &line_points, &line_uniform);
        let line_draw = (def.line > 0).then(|| line_pipeline(device, shaders, &line_layout)).flatten();
        let mut me = Self {
            line_cs: None,
            line_points,
            line_len: def.line,
            line_out_group,
            line_uniform,
            line_layout,
            line_group,
            line_draw,
            mesh: None,
            def,
            art_view: art.create_view(&wgpu::TextureViewDescriptor::default()),
            art,
            art_id: 0,
            has_art: false,
            live: def.clone(),
            error: None,
            // None, so the first reload reads the files: they may have
            // changed since the registry read them at start.
            stamp: None,
            last_poll: Instant::now(),
            uniform: scene::uniform_buffer(device, &format!("{} params", def.id), std::mem::size_of::<Uniform>() as u64),
            wave_buf: device.create_buffer(&wgpu::BufferDescriptor { label: Some("loaded visual wave"), size: (WAVE * 8) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
            sampler: scene::linear_sampler(device),
            out: [hdr_texture(device, &format!("{} a", def.id), width, height), hdr_texture(device, &format!("{} b", def.id), width, height)],
            black: hdr_texture(device, "loaded visual black", 1, 1),
            groups: Vec::new(),
            layout,
            pipeline: None,
            out_layout,
            prepare: None,
            items: item_buffers(device, def.items),
            items_len: def.items,
            item_groups: Vec::new(),
            icur: 0,
            cur: 0,
            bands: [0.0; 32],
            flow: 0.0,
            swell: 0.0,
            ring: VecDeque::with_capacity(WAVE * 2),
        };
        me.make_groups(device);
        me.reload(device, shaders);
        if me.pipeline.is_none() && me.error.is_none() {
            me.build(device, shaders);
        }
        Ok(me)
    }

    fn make_groups(&mut self, device: &wgpu::Device) {
        let group = |prev: &wgpu::TextureView, items: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("loaded visual"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: self.wave_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(prev) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 4, resource: items.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&self.art_view) },
                ],
            })
        };
        self.groups = [&self.black, &self.out[0], &self.out[1]].into_iter().flat_map(|prev| [group(prev, &self.items[0]), group(prev, &self.items[1])]).collect();
        self.item_groups = self
            .items
            .iter()
            .map(|b| device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("loaded visual items out"), layout: &self.out_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: b.as_entire_binding() }] }))
            .collect();
    }

    /// Compile the live code; on failure keep the old pipeline and the error.
    fn build(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        let src = self.live.source(shaders, &self.live.wgsl);
        let (layout, out_layout) = (&self.layout, &self.out_layout);
        let id = self.live.id.clone();
        let with_prepare = self.live.items > 0;
        let with_line = self.live.line > 0;
        let with_mesh = self.live.triangles > 0;
        let built = scene::guarded(device, || {
            let m = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(&id), source: wgpu::ShaderSource::Wgsl(src.into()) });
            let prepare = with_prepare.then(|| {
                let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("loaded visual prepare"), bind_group_layouts: &[Some(layout), Some(out_layout)], immediate_size: 0 });
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("loaded visual prepare"), layout: Some(&pl), module: &m, entry_point: Some("cs_main"), compilation_options: Default::default(), cache: None })
            });
            let line = with_line.then(|| {
                let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("loaded visual line points"), bind_group_layouts: &[Some(layout), Some(out_layout)], immediate_size: 0 });
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("loaded visual line points"), layout: Some(&pl), module: &m, entry_point: Some("cs_line"), compilation_options: Default::default(), cache: None })
            });
            let mesh = with_mesh.then(|| mesh_pipeline(device, layout, &m));
            (scene::pipeline(device, &id, layout, &m, HDR_FORMAT, None), prepare, line, mesh)
        });
        match built {
            Ok((p, prepare, line, mesh)) => {
                self.pipeline = Some(p);
                self.prepare = prepare;
                self.line_cs = line;
                self.mesh = mesh;
                self.error = None;
                if self.live.line != self.line_len {
                    self.line_points = line_buffer(device, self.live.line);
                    self.line_len = self.live.line;
                    (self.line_out_group, self.line_group) = line_groups(device, &self.out_layout, &self.line_layout, &self.line_points, &self.line_uniform);
                }
                if self.live.line > 0 && self.line_draw.is_none() {
                    self.line_draw = line_pipeline(device, shaders, &self.line_layout);
                }
                // A reload may have changed how many items there are.
                if self.live.items != self.items_len {
                    self.items = item_buffers(device, self.live.items);
                    self.items_len = self.live.items;
                    self.make_groups(device);
                }
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
        if self.line_draw.is_some() {
            self.line_draw = line_pipeline(device, shaders, &self.line_layout);
        }
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
            has_art: self.has_art as u32 as f32,
            p,
            bands: smooth,
            raw,
            chroma,
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
            wave_new: ctx.points.len().min(WAVE - 1) as f32,
            auto_gain: ctx.auto_gain,
            sample_rate: ctx.sample_rate as f32,
            // The track id hashed to 0..1.
            track: crate::rng::Rng::seeded(&[ctx.art_id]).next(),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        // The album art, once per track.
        if ctx.art_id != self.art_id {
            self.art_id = ctx.art_id;
            let px = ctx.art.and_then(art_pixels);
            if let Some(px) = &px {
                let size = wgpu::Extent3d { width: ART, height: ART, depth_or_array_layers: 1 };
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo { texture: &self.art, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    px,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * ART), rows_per_image: Some(ART) },
                    size,
                );
            }
            self.has_art = px.is_some();
        }
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
        let (write, prev) = if self.live.feedback { (1 - self.cur, 1 + self.cur) } else { (self.cur, 0) };
        // The prepare step reads last frame's items and writes this frame's.
        if let Some(prepare) = &self.prepare {
            let next = 1 - self.icur;
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("loaded visual prepare"), timestamp_writes: None });
            pass.set_pipeline(prepare);
            pass.set_bind_group(0, &self.groups[prev * 2 + self.icur], &[]);
            pass.set_bind_group(1, &self.item_groups[next], &[]);
            pass.dispatch_workgroups(self.items_len.div_ceil(PREPARE_GROUP), 1, 1);
            drop(pass);
            self.icur = next;
        }
        let group = &self.groups[prev * 2 + self.icur];
        // The line's points, after prepare so point() can read this frame's items.
        // How many points to draw: all, or what the line_count param says.
        let count = match self.live.params.iter().position(|d| d.id == self.live.line_count) {
            Some(k) => (p[k / 4][k % 4].round().max(2.0) as u32).min(self.line_len),
            None => self.line_len,
        };
        let line = match (&self.line_cs, &self.line_draw) {
            (Some(cs), Some(draw)) if count >= 2 => {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("loaded visual line points"), timestamp_writes: None });
                pass.set_pipeline(cs);
                pass.set_bind_group(0, group, &[]);
                pass.set_bind_group(1, &self.line_out_group, &[]);
                pass.dispatch_workgroups(count.div_ceil(PREPARE_GROUP), 1, 1);
                Some(draw)
            }
            _ => None,
        };
        let mesh = self.mesh.as_ref().filter(|_| self.live.triangles > 0);
        // The passes in order (picture, triangles, line); the first clears.
        let picture = self.live.shade || (line.is_none() && mesh.is_none());
        if picture {
            fullscreen_pass(encoder, &self.def.id, &self.out[write], wgpu::LoadOp::Clear(wgpu::Color::BLACK), pipeline, group);
        }
        if let Some(mesh) = mesh {
            let load = if picture { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) };
            let mut pass = scene::begin_pass(encoder, "loaded visual triangles", &self.out[write], load);
            pass.set_pipeline(mesh);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..self.live.triangles * 3, 0..1);
        }
        let picture = picture || mesh.is_some();
        if let Some(draw) = line {
            let u: [u32; 4] = [(ctx.width as f32).to_bits(), (ctx.height as f32).to_bits(), count, self.live.beam as u32];
            queue.write_buffer(&self.line_uniform, 0, bytemuck::cast_slice(&u));
            let load = if picture { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) };
            let mut pass = scene::begin_pass(encoder, "loaded visual line", &self.out[write], load);
            pass.set_pipeline(draw);
            pass.set_bind_group(0, &self.line_group, &[]);
            pass.draw(0..6, 0..count - 1);
        }
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
                            let cur = v.as_str().and_then(|s| p.choice_index(s)).unwrap_or(0);
                            let labels: Vec<&str> = p.options.iter().map(String::as_str).collect();
                            let pick = if labels.len() <= 3 {
                                small_caps(ui, &p.label);
                                segmented(ui, &labels, cur)
                            } else {
                                dropdown(ui, &p.label, &labels, Some(cur))
                            };
                            if let Some(i) = pick {
                                values.insert(p.id.clone(), Value::String(p.choice_value(i)));
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
                            values.insert(p.id.clone(), if int { Value::from(x.round() as i64) } else { Value::from(x as f64) });
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
        ui.horizontal(|ui| {
            if ghost_button(ui, &format!("Reset {}", self.live.short.to_lowercase()), false).clicked() {
                values = Map::new();
            }
            if let Some(d) = &self.def.dir {
                if ghost_button(ui, "Open its folder", false).on_hover_text("visual.json and visual.wgsl: save either and it reloads").clicked() {
                    if let Err(e) = crate::platform::open_folder(d) {
                        tracing::warn!("{e}");
                    }
                }
            }
        });
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
        assert_eq!(full["count"], 4);
        assert_eq!(full["mirror"], true);
        assert_eq!(full["print"], "dots");
        assert_eq!(d.params[3].number(Some(&Value::String("dots".into()))), 1.0);
        let stored = serde_json::json!({"radius": 1.5});
        assert_eq!(d.params_json(Some(&stored))["radius"], 1.5);
        assert!(d.generated().contains("fn params() -> Params"));
        let r = parse(r#"{"id": "x", "name": "x", "params": [{"id": "smooth", "label": "Smoothing"}]}"#, "fn shade(uv: vec2<f32>) -> vec3<f32> { return vec3<f32>(0.0); }", None).unwrap();
        assert!(r.generated().contains("smooth_: f32"), "a reserved word gets an underscore in the shader");
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
        assert_eq!(std::mem::offset_of!(Uniform, wave_new), 480 + 512);
    }

    #[test]
    fn factory_visuals_parse() {
        for d in embedded() {
            assert!(!d.wgsl.is_empty(), "{}", d.id);
        }
    }

    #[test]
    fn a_prepare_step_needs_items_and_items_need_a_prepare_step() {
        let prep = "fn prepare(i: u32) -> vec4<f32> { return vec4<f32>(0.0); }
fn shade(uv: vec2<f32>) -> vec3<f32> { return item(0u).rgb; }";
        assert!(defines(prep, "prepare"));
        assert!(defines("fn  prepare (i: u32)", "prepare"));
        assert!(!defines("// fn prepare(i: u32)
fn prepared(x: f32)", "prepare"));
        assert!(parse(r#"{"id": "x", "name": "x"}"#, prep, None).is_err());
        assert!(parse(r#"{"id": "x", "name": "x", "items": 8}"#, "fn shade(uv: vec2<f32>) -> vec3<f32> { return vec3<f32>(0.0); }", None).is_err());
        assert!(parse(r#"{"id": "x", "name": "x", "items": 99999}"#, prep, None).is_err());
        let d = parse(r#"{"id": "x", "name": "x", "items": 8}"#, prep, None).unwrap();
        assert!(d.generated().contains("const ITEMS: u32 = 8u;"));
    }

    #[test]
    fn a_line_needs_points_and_points_need_a_line() {
        let shade = "fn shade(uv: vec2<f32>) -> vec3<f32> { return vec3<f32>(0.0); }";
        let pts = "fn point(i: u32) -> LinePoint { var q: LinePoint; return q; }";
        let both = format!("{pts}
{shade}");
        assert!(parse(r#"{"id": "x", "name": "x"}"#, &both, None).is_err());
        assert!(parse(r#"{"id": "x", "name": "x", "line": 100}"#, shade, None).is_err());
        assert!(parse(r#"{"id": "x", "name": "x", "line": 1}"#, &both, None).is_err());
        let d = parse(r#"{"id": "x", "name": "x", "line": 100}"#, &both, None).unwrap();
        assert!(d.generated().contains("const LINE_POINTS: u32 = 100u;"));
        assert!(d.source(&ShaderSet::load(), &d.wgsl).contains("fn cs_line"));
    }

    #[test]
    fn new_visual_names_become_ids() {
        assert_eq!(slug("My Tunnel #2"), "my-tunnel-2");
        assert_eq!(slug("  "), "my-visual");
        assert_eq!(slug("Été"), "t");
        assert!(parse(&TEMPLATE_JSON.replace("my-visual", &slug("Neon Rings")), TEMPLATE_WGSL, None).is_ok());
    }

    #[test]
    fn a_new_visual_gets_a_free_folder_and_id() {
        let root = std::env::temp_dir().join(format!("audioviz-new-visual-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("neon-rings")).unwrap();
        let d = create_in(&root, "Neon Rings", |id| id == "neon-rings-2").unwrap();
        assert_eq!(d.id, "neon-rings-3");
        assert_eq!(d.name, "Neon Rings");
        assert_eq!(d.dir.as_deref(), Some(root.join("neon-rings-3").as_path()));
        assert_eq!(read_folder(&root, false).len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Every factory visual (and the starter template) compiles against the API as the engine composes it.
    #[test]
    fn factory_visuals_compile() {
        use wgpu::naga;
        let shaders = ShaderSet::load();
        let mut all = embedded();
        assert!(all.len() >= 4);
        all.push(parse(TEMPLATE_JSON, TEMPLATE_WGSL, None).unwrap());
        for d in all {
            let src = d.source(&shaders, &d.wgsl);
            let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}: {}", d.id, visual_lines(&e.emit_to_string(&src), d.prefix(&shaders).lines().count())));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{}: {e:?}", d.id));
        }
    }
}
