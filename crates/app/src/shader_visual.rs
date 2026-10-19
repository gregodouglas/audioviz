//! Visuals loaded from a folder: a `visual.json` manifest (names, credit,
//! up to 24 parameters) and a `visual.wgsl` holding one function,
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
//!
//! Pipelines are compiled on a worker thread (see "building off the render
//! thread" below), so picking a heavy visual never stalls the picture.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Instant, SystemTime};

use bytemuck::{Pod, Zeroable};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::factory;
use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, fullscreen_pass, hdr_texture};
use crate::settings::Layer;
use crate::shaders::ShaderSet;

pub const MAX_PARAMS: usize = 24;
/// Most items a prepare step may compute.
pub const MAX_ITEMS: u32 = 262_144;
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
    /// A button: not saved; the shader sees how many times it was pressed.
    Button,
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
    /// For an integer: 0 is a setting of its own below `min`, shown as this
    /// ("By track"); values between 0 and `min` snap to `min`.
    #[serde(default)]
    pub zero: String,
    /// Decimals shown for a slider's value (default: by its range).
    #[serde(default)]
    pub decimals: Option<usize>,
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
    /// Pictures for `picture(uv)`: the album art, or the photos folder.
    #[serde(default)]
    pictures: Option<Pictures>,
    /// A second picture `shade` writes with `write_trail` and reads back
    /// next frame with `trail(uv)`.
    #[serde(default)]
    trail: bool,
    /// Shared libraries put in front of the code ("creatures").
    #[serde(default)]
    uses: Vec<String>,
    /// More code files in the visual's folder, after visual.wgsl, in order
    /// (read from the raw JSON by `with_parts`, before the manifest is parsed).
    #[serde(default)]
    #[allow(dead_code)]
    parts: Vec<String>,
    /// How the layer's levelling measures it: "lit" (the default: every lit
    /// part) or "bright" (only the bright part; faint fill does not count).
    #[serde(default)]
    levelling: String,
    /// Smooth the triangles' edges (4x multisampling of the triangle pass).
    #[serde(default)]
    antialias: bool,
    /// Can join a shared space (ADR-0002): it reads the space's camera, and
    /// the engine applies its opacity inside its draws.
    #[serde(default)]
    space: bool,
}

/// Where a visual's pictures come from: a choice control whose second
/// option means the photos folder (the first, the album art), and a number
/// control for the seconds each photo stays.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Pictures {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub dwell: String,
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
    /// The picture feed, when the visual asks for one.
    pub pictures: Option<Pictures>,
    pub paint: bool,
    pub band_waveform: bool,
    /// `shade` keeps a trail (`write_trail`, read back with `trail`).
    pub trail: bool,
    /// Shared libraries in front of the code.
    pub uses: Vec<String>,
    /// The levelling measures only the bright part ("levelling": "bright").
    pub level_bright: bool,
    /// The triangles are multisampled ("antialias": true).
    pub antialias: bool,
    /// Can join a shared space ("space": true).
    pub space: bool,
    /// Says where a shared space's camera is (`fn camera`), when it leads.
    pub camera: bool,
    /// Writes light that diffuses in a shared space (`write_glow`).
    pub glow: bool,
    pub wgsl: String,
    /// The folder it came from, watched for changes (none for embedded copies).
    pub dir: Option<PathBuf>,
}

/// Libraries a visual may put in front of its code with "uses": (name,
/// file, the libraries it needs, which come with it).
const LIBRARIES: &[(&str, &str, &[&str])] = &[
    ("creatures", "library/creatures.wgsl", &[]),
    ("luminous", "library/luminous.wgsl", &[]),
    ("jelly", "library/jelly.wgsl", &["creatures", "luminous"]),
];

/// The library files a visual's "uses" brings in, each once, what a library
/// needs before it.
fn library_files(uses: &[String]) -> Vec<&'static str> {
    fn add(name: &str, out: &mut Vec<&'static str>) {
        if let Some((_, file, needs)) = LIBRARIES.iter().find(|(n, _, _)| *n == name) {
            for n in needs.iter() {
                add(n, out);
            }
            if !out.contains(file) {
                out.push(file);
            }
        }
    }
    let mut out = Vec::new();
    for u in uses {
        add(u, &mut out);
    }
    out
}

/// How many code files a visual may add to visual.wgsl with "parts".
const MAX_PARTS: usize = 16;

/// The line in front of each part in a visual's code.
fn part_marker(name: &str) -> String {
    format!("// ==== part: {name} ====")
}

/// A visual's whole code: visual.wgsl, then each file its manifest lists in
/// "parts" (plain .wgsl names in the same folder), each behind a marker line
/// so errors can name the file. `read` gives a file's text by name.
fn with_parts(json: &str, main: &str, read: impl Fn(&str) -> Option<String>) -> Result<String, String> {
    let parts: Vec<String> = serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|v| v.get("parts").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()))
        .unwrap_or_default();
    if parts.len() > MAX_PARTS {
        return Err(format!("visual.json: at most {MAX_PARTS} parts"));
    }
    let mut code = main.to_string();
    for name in &parts {
        if !name.ends_with(".wgsl") || name.contains(['/', '\\']) || name.starts_with('.') || name == "visual.wgsl" {
            return Err(format!("visual.json: part \"{name}\" must be a .wgsl file in the visual's folder"));
        }
        let Some(text) = read(name) else { return Err(format!("visual.json: part \"{name}\" is not in the folder")) };
        code.push('\n');
        code.push_str(&part_marker(name));
        code.push('\n');
        code.push_str(&text);
    }
    Ok(code)
}

/// The code of the visual in folder `d` (visual.wgsl and its parts).
fn read_code(d: &Path, json: &str) -> Result<String, String> {
    let main = std::fs::read_to_string(d.join("visual.wgsl")).map_err(|e| format!("visual.wgsl: {e}"))?;
    with_parts(json, &main, |name| std::fs::read_to_string(d.join(name)).ok())
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

/// Whether the code calls `<name>(`, ignoring line comments and the
/// function's own definition.
fn calls(wgsl: &str, name: &str) -> bool {
    wgsl.lines().map(|l| l.split("//").next().unwrap_or("")).any(|l| {
        l.match_indices(name).any(|(at, _)| {
            let before = &l[..at];
            !before.ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_') && !before.trim_end().ends_with("fn") && l[at + name.len()..].trim_start().starts_with('(')
        })
    })
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
    if let Some(u) = m.uses.iter().find(|u| !LIBRARIES.iter().any(|(name, _, _)| name == u)) {
        let names: Vec<&str> = LIBRARIES.iter().map(|(name, _, _)| *name).collect();
        return Err(format!("visual.json: uses \"{u}\" is not a library (there are: {})", names.join(", ")));
    }
    if !matches!(m.levelling.as_str(), "" | "lit" | "bright") {
        return Err(format!("visual.json: levelling \"{}\" is not \"lit\" or \"bright\"", m.levelling));
    }
    if m.antialias && m.trail {
        return Err("visual.json: \"antialias\" and \"trail\" do not go together yet".into());
    }
    if m.space && (m.feedback || m.trail) {
        return Err("visual.json: \"space\" does not go with \"feedback\" or \"trail\" (in a shared space a layer has no picture of its own to keep)".into());
    }
    if defines(wgsl, "camera") && !m.space {
        return Err("visual.wgsl has fn camera, which leads a shared space's camera: set \"space\": true in visual.json".into());
    }
    if m.trail && !defines(wgsl, "shade") {
        return Err("visual.json sets \"trail\": fn shade writes it, and visual.wgsl has none".into());
    }
    if !defines(wgsl, "shade") && m.line == 0 && m.triangles == 0 {
        return Err("visual.wgsl needs fn shade(uv: vec2<f32>) -> vec3<f32> (only a visual with a line or triangles may leave it out)".into());
    }
    if let Some(pics) = &m.pictures {
        if !pics.from.is_empty() && !m.params.iter().any(|p| p.id == pics.from && p.kind == ParamKind::Choice) {
            return Err(format!("visual.json: pictures.from \"{}\" is not a choice param", pics.from));
        }
        if !pics.dwell.is_empty() && !m.params.iter().any(|p| p.id == pics.dwell && matches!(p.kind, ParamKind::Slider | ParamKind::Integer)) {
            return Err(format!("visual.json: pictures.dwell \"{}\" is not a number param", pics.dwell));
        }
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
    Ok(ShaderDef { id: m.id, name: m.name, short, summary: m.summary, credit: m.credit, hint: m.hint, params: m.params, feedback: m.feedback, items: m.items, line: m.line, line_count: m.line_count, shade: defines(wgsl, "shade"), beam: m.line_style == "beam", triangles: m.triangles, pictures: m.pictures.clone(), paint: defines(wgsl, "paint"), band_waveform: m.band_waveform, trail: m.trail, uses: m.uses, level_bright: m.levelling == "bright", antialias: m.antialias && m.triangles > 0, space: m.space, camera: defines(wgsl, "camera"), glow: calls(wgsl, "write_glow"), wgsl: wgsl.to_string(), dir })
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
            ParamKind::Button => Value::Null,
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
        // Buttons are not saved.
        Value::Object(self.params.iter().filter(|p| p.kind != ParamKind::Button).map(|p| (p.id.clone(), p.value(obj.and_then(|o| o.get(&p.id))))).collect())
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
        let libs: String = library_files(&self.uses).iter().map(|file| format!("{}\n", shaders.raw(file))).collect();
        format!("{}\n{libs}{}\n// ---- visual.wgsl ----\n", shaders.source_base("visual_api.wgsl"), self.generated())
    }

    fn source(&self, shaders: &ShaderSet, wgsl: &str) -> String {
        // The prepare step's entry point goes after the visual's code, so
        // error lines in visual.wgsl keep their offset.
        // Both write group 1, binding 0: one pipeline uses each.
        let mut prepare = String::new();
        if self.items > 0 || self.line > 0 || self.camera {
            prepare.push_str("\n@group(1) @binding(0) var<storage, read_write> items_out: array<vec4<f32>>;\n");
        }
        if self.items > 0 {
            prepare.push_str(&format!("\n@compute @workgroup_size({PREPARE_GROUP})\nfn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {{\n    if (id.x < ITEMS) {{\n        items_out[id.x] = prepare(id.x);\n    }}\n}}\n"));
        }
        if self.line > 0 {
            prepare.push_str(&format!("\n@compute @workgroup_size({PREPARE_GROUP})\nfn cs_line(@builtin(global_invocation_id) id: vec3<u32>) {{\n    if (id.x < LINE_POINTS) {{\n        let q = point(id.x);\n        items_out[2u * id.x] = vec4<f32>(q.pos, q.width, q.depth);\n        items_out[2u * id.x + 1u] = vec4<f32>(q.colour * V.space.y, 0.0);\n    }}\n}}\n"));
        }
        // The leading layer of a shared space hands its camera to the others.
        if self.camera {
            prepare.push_str("\n@compute @workgroup_size(1)\nfn cs_camera() {\n    let c = camera();\n    items_out[0] = vec4<f32>(c.yaw, c.pitch, c.dist, c.focal);\n    items_out[1] = vec4<f32>(c.look, 0.0);\n}\n");
        }
        if self.triangles > 0 {
            let paint = if self.paint { "paint(f)" } else { "f.colour" };
            prepare.push_str(&format!(
                "\nstruct MeshOut {{\n    @builtin(position) clip: vec4<f32>,\n    @location(0) colour: vec4<f32>,\n    @location(1) data: vec4<f32>,\n}};\n\nfn mesh_out(vi: u32) -> MeshOut {{\n    let v = vertex(vi);\n    let short = min(V.resolution.x, V.resolution.y);\n    var o: MeshOut;\n    o.clip = vec4<f32>(v.pos.x * short / V.resolution.x, v.pos.y * short / V.resolution.y, 0.0, v.pos.w);\n    o.colour = v.colour;\n    o.data = v.data;\n    return o;\n}}\n\n@vertex\nfn vs_mesh(@builtin(vertex_index) vi: u32) -> MeshOut {{\n    return mesh_out(vi);\n}}\n\n@fragment\nfn fs_mesh(m: MeshOut) -> @location(0) vec4<f32> {{\n    var f: MeshFragment;\n    f.colour = m.colour;\n    f.data = m.data;\n    f.pixel = m.clip.xy;\n    let c = {paint};\n    // A NaN must not reach the levelling.\n    if (!all(c == c)) {{\n        return vec4<f32>(0.0);\n    }}\n    return vec4<f32>(max(c.rgb, vec3<f32>(0.0)), clamp(c.a, 0.0, 1.0)) * V.space.y;\n}}\n"
            ));
        }
        // In a shared space every draw also writes the space's light target:
        // rgb the glow, alpha where the light is (the view depth, kept
        // nearest by the pipeline's Min blend; far where nothing is lit).
        if self.space {
            prepare.push_str("\nstruct SpaceOut {\n    @location(0) colour: vec4<f32>,\n    @location(1) light: vec4<f32>,\n};\n\nfn space_light(c: vec3<f32>, depth: f32) -> vec4<f32> {\n    let g = select(vec3<f32>(0.0), max(glow_out, vec3<f32>(0.0)), all(glow_out == glow_out)) * V.space.y;\n    let lit = dot(c + g, vec3<f32>(1.0)) > 0.0 && depth > 0.0;\n    return vec4<f32>(g, select(LIGHT_FAR, depth, lit));\n}\n\n@fragment\nfn fs_space(in: FsIn) -> SpaceOut {\n    let c = shade(in.uv);\n    var o: SpaceOut;\n    o.colour = vec4<f32>(select(vec3<f32>(0.0), max(c, vec3<f32>(0.0)), all(c == c)) * V.space.y, 1.0);\n    o.light = space_light(o.colour.rgb, depth_out);\n    return o;\n}\n");
            if self.triangles > 0 {
                let paint = if self.paint { "paint(f)" } else { "f.colour" };
                prepare.push_str(&format!(
                    "\nstruct MeshSpaceOut {{\n    @builtin(position) clip: vec4<f32>,\n    @location(0) colour: vec4<f32>,\n    @location(1) data: vec4<f32>,\n    @location(2) depth: f32,\n}};\n\n@vertex\nfn vs_mesh_space(@builtin(vertex_index) vi: u32) -> MeshSpaceOut {{\n    let m = mesh_out(vi);\n    return MeshSpaceOut(m.clip, m.colour, m.data, m.clip.w);\n}}\n\n@fragment\nfn fs_mesh_space(m: MeshSpaceOut) -> SpaceOut {{\n    var f: MeshFragment;\n    f.colour = m.colour;\n    f.data = m.data;\n    f.pixel = m.clip.xy;\n    let c = {paint};\n    var o: SpaceOut;\n    o.colour = select(vec4<f32>(0.0), vec4<f32>(max(c.rgb, vec3<f32>(0.0)), clamp(c.a, 0.0, 1.0)) * V.space.y, all(c == c));\n    o.light = space_light(o.colour.rgb, m.depth);\n    return o;\n}}\n"
                ));
            }
        }
        // A line visual may leave out shade(): the line pass clears instead.
        if !self.shade {
            prepare.push_str("\nfn shade(uv: vec2<f32>) -> vec3<f32> {\n    return vec3<f32>(0.0);\n}\n");
        }
        // One NaN pixel would black out the layer's levelling: drop it. The
        // wrappers scale by V.space.y: a layer's opacity in a shared space
        // (1 otherwise).
        let main = if self.trail {
            "@fragment\nfn fs_main(in: FsIn) -> TrailOut {\n    trail_out = vec3<f32>(0.0);\n    let c = shade(in.uv);\n    let t = trail_out;\n    var o: TrailOut;\n    o.colour = vec4<f32>(select(vec3<f32>(0.0), max(c, vec3<f32>(0.0)), all(c == c)), 1.0);\n    o.trail = vec4<f32>(select(vec3<f32>(0.0), max(t, vec3<f32>(0.0)), all(t == t)), 1.0);\n    return o;\n}\n\nstruct TrailOut {\n    @location(0) colour: vec4<f32>,\n    @location(1) trail: vec4<f32>,\n};\n"
        } else {
            "@fragment\nfn fs_main(in: FsIn) -> @location(0) vec4<f32> {\n    let c = shade(in.uv);\n    let ok = all(c == c);\n    return vec4<f32>(select(vec3<f32>(0.0), max(c, vec3<f32>(0.0)), ok) * V.space.y, 1.0);\n}\n"
        };
        prepare_after(&prepare, format!("{}{}\n\n{main}", self.prefix(shaders), wgsl))
    }
}

impl ShaderDef {
    /// What the worker needs to compile this visual's pipelines.
    fn job(&self, shaders: &ShaderSet) -> Job {
        let src = self.source(shaders, &self.wgsl);
        let (prepare, line, mesh, trail) = (self.items > 0, self.line > 0, self.triangles > 0, self.trail);
        let samples = if mesh && self.antialias { MESH_SAMPLES } else { 1 };
        let (space, camera) = (self.space, self.camera);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (&src, prepare, line, mesh, trail, samples, space, camera).hash(&mut h);
        let line_src = line.then(|| {
            let text = shaders.source("visual_lines.wgsl");
            let mut h = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut h);
            (h.finish(), text)
        });
        Job { key: h.finish(), id: self.id.clone(), src, prepare, line, mesh, trail, samples, space, camera, line_src }
    }
}

fn prepare_after(prepare: &str, mut s: String) -> String {
    s.push_str(prepare);
    s
}

/// Line `line` (1-based) of a visual's code, as (file, line in it): the
/// part markers `with_parts` puts in say where each file starts.
fn code_position(code: &str, line: usize) -> (String, usize) {
    let mut file = "visual.wgsl".to_string();
    let mut start = 0;
    for (i, l) in code.lines().enumerate() {
        if i + 1 >= line {
            break;
        }
        if let Some(name) = l.strip_prefix("// ==== part: ").and_then(|r| r.strip_suffix(" ====")) {
            file = name.to_string();
            start = i + 1;
        }
    }
    (file, line - start)
}

/// Rewrite the compiler's `wgsl:LINE:COL` positions, which count the whole
/// composed shader, as positions in visual.wgsl or one of its parts (`code`,
/// the visual's code), or the engine's part.
fn visual_lines(err: &str, offset: usize, code: &str) -> String {
    let mut out = String::with_capacity(err.len());
    let mut rest = err;
    while let Some(at) = rest.find("wgsl:") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 5..];
        let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
        match digits.parse::<usize>() {
            Ok(line) if line > offset => {
                let (file, at) = code_position(code, line - offset);
                out.push_str(&format!("{file} line {at}, col"));
            }
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
/// visual reloads live; AUDIOVIZ_REPO=<checkout> reads that checkout's in
/// any build (the lab sets it).
fn repo_folder() -> Option<PathBuf> {
    let d = match std::env::var_os("AUDIOVIZ_REPO") {
        Some(r) => PathBuf::from(r).join("assets").join("visuals"),
        None if cfg!(debug_assertions) => Path::new(env!("CARGO_MANIFEST_DIR")).parent()?.parent()?.join("assets").join("visuals"),
        None => return None,
    };
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
        let Ok(json) = std::fs::read_to_string(d.join("visual.json")) else { continue };
        if !d.join("visual.wgsl").is_file() {
            continue;
        }
        match read_code(&d, &json).and_then(|wgsl| parse(&json, &wgsl, watch.then(|| d.clone()))) {
            Ok(def) => out.push(def),
            Err(e) => tracing::warn!("visual {}: {e}", d.display()),
        }
    }
    out
}

/// Whether `id` is one of the visuals that ship with the app.
/// Whether visual `id` can join a shared space ("space": true).
pub fn can_join_space(id: &str) -> bool {
    matches!(crate::visuals::get(id).map(|v| &v.source), Some(crate::visuals::Source::Loaded(d)) if d.space)
}

pub fn is_factory(id: &str) -> bool {
    static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| embedded().into_iter().map(|d| d.id).collect()).iter().any(|f| f == id)
}

pub fn embedded() -> Vec<ShaderDef> {
    let mut by_folder: BTreeMap<&str, BTreeMap<&str, &str>> = BTreeMap::new();
    for (rel, text) in factory::embedded("visuals/") {
        let mut parts = rel.split('/');
        let (_, Some(f), Some(file), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else { continue };
        by_folder.entry(f).or_default().insert(file, text);
    }
    by_folder
        .into_iter()
        .filter_map(|(f, files)| {
            let (json, main) = (files.get("visual.json")?, files.get("visual.wgsl")?);
            with_parts(json, main, |name| files.get(name).map(|t| t.to_string())).and_then(|wgsl| parse(json, &wgsl, None)).map_err(|e| tracing::warn!("factory visual {f}: {e}")).ok()
        })
        .collect()
}

/// Whether layer `l`'s visual `id` takes pictures and its source control is
/// set to the photos folder (the panel then shows the folder field).
pub fn wants_photos(id: &str, l: &Layer) -> bool {
    let Some(crate::visuals::Source::Loaded(d)) = crate::visuals::get(id).map(|v| &v.source) else { return false };
    let Some(pics) = &d.pictures else { return false };
    let stored = l.params.get(id).and_then(Value::as_object);
    d.params.iter().find(|p| p.id == pics.from).is_some_and(|p| p.number(stored.and_then(|o| o.get(&p.id))) > 0.5)
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
    p: [[f32; 4]; 6],
    bands: [[f32; 4]; 8],
    raw: [[f32; 4]; 8],
    chroma: [[f32; 4]; 3],
    palette: crate::color::Table,
    wave_new: f32,
    auto_gain: f32,
    sample_rate: f32,
    track: f32,
    instance: f32,
    picture_fade: f32,
    picture_aspect: f32,
    picture_prev_aspect: f32,
    picture_count: f32,
    confidence: f32,
    silent: f32,
    _pad8: f32,
    hits: [f32; 4],
    strengths: [f32; 4],
    chroma_slow: [[f32; 4]; 3],
    /// (1 in a shared space, the layer's opacity there (1 otherwise), 0, 0).
    space: [f32; 4],
}

// ------------------------------------------------------------ building off the render thread
//
// Compiling a visual's pipelines takes 0.1 to 1.6 s on D3D12 (FXC, then the
// driver). One worker thread does it, so the app keeps drawing: a layer whose
// pipelines have not arrived draws nothing (`Scene::ready` is false, and the
// director keeps the visual it replaced on screen meanwhile). Results are
// kept by source, so each version of a visual is compiled once a run and
// every layer showing it shares the pipelines; `prewarm` compiles the
// scenes' visuals before they are needed. A layer waiting for its visual
// goes ahead of the prewarm queue.

/// The bind group layouts every loaded visual uses (shared, so one compiled
/// pipeline serves every layer that shows the visual), and the worker's queue.
struct Shared {
    device: wgpu::Device,
    layout: wgpu::BindGroupLayout,
    out_layout: wgpu::BindGroupLayout,
    line_layout: wgpu::BindGroupLayout,
    /// The shared space's camera, which every visual reads (binding 10).
    camera: wgpu::Buffer,
    work: Mutex<Work>,
    wake: Condvar,
}

#[derive(Default)]
struct Work {
    jobs: VecDeque<Job>,
    queued: HashSet<u64>,
    done: HashMap<u64, Result<Arc<Built>, String>>,
    /// The engine's line pipeline by the hash of `visual_lines.wgsl`, for a
    /// layer of its own or a shared space.
    lines: HashMap<(u64, bool), Option<wgpu::RenderPipeline>>,
}

struct Job {
    key: u64,
    id: String,
    src: String,
    prepare: bool,
    line: bool,
    mesh: bool,
    trail: bool,
    /// The triangle pass's samples (1, or MESH_SAMPLES when antialiased).
    samples: u32,
    /// Build the shared-space pipelines too, and the camera pass.
    space: bool,
    camera: bool,
    line_src: Option<(u64, String)>,
}

/// A visual's compiled pipelines.
struct Built {
    picture: wgpu::RenderPipeline,
    prepare: Option<wgpu::ComputePipeline>,
    line: Option<wgpu::ComputePipeline>,
    mesh: Option<wgpu::RenderPipeline>,
    line_draw: Option<wgpu::RenderPipeline>,
    /// Samples per pixel of the picture and triangles (1, or MESH_SAMPLES).
    samples: u32,
    /// With "space": how it draws in a shared space.
    space: Option<SpaceBuilt>,
}

/// A visual's pipelines in a shared space: everything draws into the
/// space's multisampled target, the picture added (the layers share it), and
/// a leading visual's camera pass.
struct SpaceBuilt {
    picture: wgpu::RenderPipeline,
    mesh: Option<wgpu::RenderPipeline>,
    line_draw: Option<wgpu::RenderPipeline>,
    camera: Option<wgpu::ComputePipeline>,
}

/// Bytes of a shared space's camera: (yaw, pitch, dist, focal), (look, 0).
const CAMERA_BYTES: u64 = 32;

fn shared(device: &wgpu::Device) -> Arc<Shared> {
    static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
    SHARED
        .get_or_init(|| {
            let entry = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::COMPUTE, ty, count: None };
            let texture = wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false };
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("loaded visual"),
                entries: &[
                    entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                    entry(1, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }),
                    entry(2, texture),
                    entry(3, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                    entry(4, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }),
                    entry(5, texture),
                    entry(6, texture),
                    entry(7, texture),
                    entry(8, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                    entry(9, texture),
                    entry(10, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }),
                ],
            });
            let out_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("loaded visual items out"),
                entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }, count: None }],
            });
            let line_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("loaded visual line draw"),
                entries: &[
                    wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                    wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                ],
            });
            // One space at a time, so one camera; it starts where the tank's would.
            let camera = device.create_buffer(&wgpu::BufferDescriptor { label: Some("shared space camera"), size: CAMERA_BYTES, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: true });
            if let Ok(mut v) = camera.slice(..).get_mapped_range_mut() {
                v.copy_from_slice(bytemuck::cast_slice(&[0.0f32, 0.0, 4.6, 2.3, 0.0, 0.0, 0.0, 0.0]));
            }
            camera.unmap();
            let sh = Arc::new(Shared { device: device.clone(), layout, out_layout, line_layout, camera, work: Mutex::new(Work::default()), wake: Condvar::new() });
            let worker = sh.clone();
            if let Err(e) = std::thread::Builder::new().name("visual builder".into()).spawn(move || worker.run()) {
                tracing::error!("could not start the visual builder: {e}");
            }
            sh
        })
        .clone()
}

impl Shared {
    /// Queue a job (unless it is done or queued already); `urgent` puts it,
    /// or moves it, to the front. Returns its key.
    fn request(&self, job: Job, urgent: bool) -> u64 {
        let key = job.key;
        let Ok(mut w) = self.work.lock() else { return key };
        if w.done.contains_key(&key) {
            return key;
        }
        if w.queued.contains(&key) {
            if urgent {
                if let Some(at) = w.jobs.iter().position(|j| j.key == key) {
                    if let Some(j) = w.jobs.remove(at) {
                        w.jobs.push_front(j);
                    }
                }
            }
            return key;
        }
        w.queued.insert(key);
        if urgent { w.jobs.push_front(job) } else { w.jobs.push_back(job) }
        self.wake.notify_one();
        key
    }

    fn result(&self, key: u64) -> Option<Result<Arc<Built>, String>> {
        self.work.lock().ok()?.done.get(&key).cloned()
    }

    fn run(&self) {
        loop {
            let job = {
                let Ok(mut w) = self.work.lock() else { return };
                while w.jobs.is_empty() {
                    w = match self.wake.wait(w) {
                        Ok(w) => w,
                        Err(_) => return,
                    };
                }
                w.jobs.pop_front().expect("a job")
            };
            let started = Instant::now();
            let line_draw = |space: bool| {
                let (k, src) = job.line_src.as_ref()?;
                if let Some(p) = self.work.lock().ok().and_then(|w| w.lines.get(&(*k, space)).cloned()) {
                    return p;
                }
                let p = scene::guarded(&self.device, || {
                    let m = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("visual_lines.wgsl"), source: wgpu::ShaderSource::Wgsl(src.as_str().into()) });
                    if space {
                        space_pipeline(&self.device, "loaded visual line in a shared space", &self.line_layout, &m, ("vs_main", "fs_space"), scene::ADDITIVE, LIGHT_NONE)
                    } else {
                        scene::pipeline(&self.device, "loaded visual line", &self.line_layout, &m, HDR_FORMAT, Some(scene::ADDITIVE))
                    }
                })
                .map_err(|e| tracing::warn!("visual_lines.wgsl: {e}"))
                .ok();
                if let Ok(mut w) = self.work.lock() {
                    w.lines.insert((*k, space), p.clone());
                }
                p
            };
            let built = self.compile(&job).map(|(picture, prepare, line, mesh, space)| {
                let space = space.map(|s| SpaceBuilt { line_draw: line_draw(true), ..s });
                Arc::new(Built { picture, prepare, line, mesh, line_draw: line_draw(false), samples: job.samples, space })
            });
            match &built {
                Ok(_) => tracing::debug!("visual {} built in {} ms", job.id, started.elapsed().as_millis()),
                Err(e) => tracing::debug!("visual {} failed to build: {e}", job.id),
            }
            if let Ok(mut w) = self.work.lock() {
                w.queued.remove(&job.key);
                w.done.insert(job.key, built);
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn compile(&self, job: &Job) -> Result<(wgpu::RenderPipeline, Option<wgpu::ComputePipeline>, Option<wgpu::ComputePipeline>, Option<wgpu::RenderPipeline>, Option<SpaceBuilt>), String> {
        let (device, layout, out_layout) = (&self.device, &self.layout, &self.out_layout);
        scene::guarded(device, || {
            let m = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(&job.id), source: wgpu::ShaderSource::Wgsl(job.src.as_str().into()) });
            let compute = |label: &str, entry: &str| {
                let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(layout), Some(out_layout)], immediate_size: 0 });
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(label), layout: Some(&pl), module: &m, entry_point: Some(entry), compilation_options: Default::default(), cache: None })
            };
            let prepare = job.prepare.then(|| compute("loaded visual prepare", "cs_main"));
            let line = job.line.then(|| compute("loaded visual line points", "cs_line"));
            let mesh = job.mesh.then(|| mesh_pipeline(device, layout, &m, job.samples));
            let picture = if job.trail { scene::pipeline2(device, &job.id, layout, &m, HDR_FORMAT) } else { scene::pipeline_samples(device, &job.id, layout, &m, HDR_FORMAT, None, job.samples) };
            // In a shared space everything is multisampled, the picture adds,
            // and every draw writes the light target too.
            let space = job.space.then(|| SpaceBuilt {
                picture: space_pipeline(device, &format!("{} in a shared space", job.id), layout, &m, ("vs_main", "fs_space"), scene::ADDITIVE, LIGHT_NONE),
                mesh: job.mesh.then(|| space_pipeline(device, &format!("{} triangles in a shared space", job.id), layout, &m, ("vs_mesh_space", "fs_mesh_space"), PREMULTIPLIED_OVER, LIGHT_GLOW)),
                line_draw: None,
                camera: job.camera.then(|| compute("loaded visual camera", "cs_camera")),
            });
            (picture, prepare, line, mesh, space)
        })
    }
}

/// Compile visuals `ids` in the background, in order, behind anything a layer
/// is waiting for, so picking them later finds them ready.
pub fn prewarm<'a>(device: &wgpu::Device, shaders: &ShaderSet, ids: impl IntoIterator<Item = &'a str>) {
    let sh = shared(device);
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id.to_string()) {
            continue;
        }
        if let Some(crate::visuals::Source::Loaded(d)) = crate::visuals::get(id).map(|v| &v.source) {
            sh.request(d.job(shaders), false);
        }
    }
}

/// The lab's hand on a shared space's camera (ADR-0003): after the leading
/// visual's camera pass, it turns the camera round what it looks at and
/// moves it nearer or further.
pub struct CameraNudge {
    pipeline: wgpu::ComputePipeline,
    group: wgpu::BindGroup,
    buf: wgpu::Buffer,
}

const NUDGE_WGSL: &str = "
@group(0) @binding(0) var<storage, read_write> cam: array<vec4<f32>, 2>;
@group(0) @binding(1) var<uniform> n: vec4<f32>;

@compute @workgroup_size(1)
fn main() {
    cam[0].x += n.x;
    cam[0].y = clamp(cam[0].y + n.y, -1.55, 1.55);
    cam[0].z *= n.z;
}
";

impl CameraNudge {
    pub fn new(device: &wgpu::Device) -> Self {
        let sh = shared(device);
        let m = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("camera nudge"), source: wgpu::ShaderSource::Wgsl(NUDGE_WGSL.into()) });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("camera nudge"), layout: None, module: &m, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
        let buf = scene::uniform_buffer(device, "camera nudge", 16);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera nudge"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: sh.camera.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: buf.as_entire_binding() }],
        });
        Self { pipeline, group, buf }
    }

    /// Turn the space's camera by `yaw` and `pitch` (degrees) and scale its
    /// distance by `zoom`.
    pub fn apply(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, [yaw, pitch, zoom]: [f32; 3]) {
        queue.write_buffer(&self.buf, 0, bytemuck::cast_slice(&[yaw.to_radians(), pitch.to_radians(), zoom, 0.0]));
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("camera nudge"), timestamp_writes: None });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
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
    /// The worker's job for the live code, until its pipelines arrive, and
    /// the line count of the code in front of visual.wgsl (for its errors).
    waiting: Option<u64>,
    prefix_lines: usize,
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
    /// Antialiased: the multisampled target the picture and the triangles
    /// draw in (made when the built code asks for antialiasing).
    msaa: Option<wgpu::TextureView>,
    uniform: wgpu::Buffer,
    wave_buf: wgpu::Buffer,
    sampler: wgpu::Sampler,
    out: [wgpu::TextureView; 2],
    /// The trail's two textures (with "trail"), paired with `out`.
    trail: Option<[wgpu::TextureView; 2]>,
    size: (u32, u32),
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
    /// Random per instance (`V.instance`), so two layers of one visual differ.
    instance: f32,
    /// Pictures: the device (pictures arrive at any size), the current and
    /// previous picture with their sizes, the crossfade, how many have come,
    /// the photos decoder and when to ask it for the next, the art shown.
    device: wgpu::Device,
    pics: [wgpu::TextureView; 2],
    pic_size: [(u32, u32); 2],
    pic_fade: f32,
    pic_count: u32,
    deck: Option<crate::pictures::PhotoDeck>,
    deck_next: f32,
    pic_art: u64,
    mirror: wgpu::Sampler,
    /// How many times each button control was pressed (what the shader sees).
    presses: BTreeMap<String, u32>,
    /// The build with its shared-space pipelines (with "space").
    space: Option<Arc<Built>>,
    /// Where a leading visual's camera pass writes, before it is copied to
    /// the space's camera, and the group that writes it.
    camera_out: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    /// How many line points this frame draws (0: no line).
    line_now: u32,
}

/// A picture texture filled with `rgba` (sRGB, one level); unfilled it is
/// zeroed, which is black.
fn picture_texture(device: &wgpu::Device, fill: Option<(&wgpu::Queue, &[u8])>, width: u32, height: u32) -> wgpu::TextureView {
    let size = wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("loaded visual picture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    if let Some((queue, rgba)) = fill {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * size.width), rows_per_image: Some(size.height) },
            size,
        );
    }
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// How long a new picture takes to replace the last.
const PICTURE_FADE_S: f32 = 1.5;

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

/// Samples per pixel of an antialiased triangle pass.
const MESH_SAMPLES: u32 = 4;

/// The multisampled target an antialiased visual draws its picture and
/// triangles in before they resolve into its output.
pub(crate) fn msaa_target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("loaded visual msaa"),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: MESH_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// How a shared space's light target blends: the glow adds, and the light
/// depth keeps the nearest.
const LIGHT_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Min },
};

/// What a draw writes of the light target. Only the glow for now: no pass
/// reads the light depth yet, and keeping it (the alpha's Min blend) cost
/// the bells' pass about 0.3 ms at 1440p; refraction and depth of field
/// bring it back, in a target of its own (ADR-0002's fallback). `shade` and
/// the line write nothing there, so glow comes from `paint` for now: `shade`
/// writing every pixel of the 4x target, once per member, cost 0.3 to 0.5 ms.
const LIGHT_GLOW: wgpu::ColorWrites = wgpu::ColorWrites::COLOR;
const LIGHT_NONE: wgpu::ColorWrites = wgpu::ColorWrites::empty();

/// A pipeline drawing into a shared space: its picture target (blended by
/// `blend`) and its light target (writing `light` of it), both multisampled.
pub(crate) fn space_pipeline(device: &wgpu::Device, label: &str, layout: &wgpu::BindGroupLayout, module: &wgpu::ShaderModule, (vs, fs): (&str, &str), blend: wgpu::BlendState, light: wgpu::ColorWrites) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
        vertex: wgpu::VertexState { module, entry_point: Some(vs), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL }), Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(LIGHT_BLEND), write_mask: light })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: MESH_SAMPLES, ..Default::default() },
        multiview_mask: None,
        cache: None,
    })
}

fn mesh_pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, module: &wgpu::ShaderModule, samples: u32) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("loaded visual triangles"), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("loaded visual triangles"),
        layout: Some(&pl),
        vertex: wgpu::VertexState { module, entry_point: Some("vs_mesh"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module, entry_point: Some("fs_mesh"), targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(PREMULTIPLIED_OVER), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        multiview_mask: None,
        cache: None,
    })
}

fn line_buffer(device: &wgpu::Device, n: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some("loaded visual line"), size: n.max(1) as u64 * 32, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false })
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

fn trail_textures(device: &wgpu::Device, id: &str, width: u32, height: u32) -> [wgpu::TextureView; 2] {
    [hdr_texture(device, &format!("{id} trail a"), width, height), hdr_texture(device, &format!("{id} trail b"), width, height)]
}

fn item_buffers(device: &wgpu::Device, n: u32) -> [wgpu::Buffer; 2] {
    let make = |label| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: n.max(1) as u64 * 16, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
    [make("loaded visual items a"), make("loaded visual items b")]
}

/// When visual.json last changed, and the newest of the folder's .wgsl files
/// (visual.wgsl and its parts).
fn stamp(dir: &Path) -> Option<(SystemTime, SystemTime)> {
    let m = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut code = m(&dir.join("visual.wgsl"))?;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "wgsl") {
            code = code.max(m(&p).unwrap_or(code));
        }
    }
    Some((m(&dir.join("visual.json"))?, code))
}

/// A new layer's `V.instance`: random, or with AUDIOVIZ_SEED=n (dev) the
/// same each run, so snapshots of two builds show the same layout.
fn instance_seed(def: &ShaderDef) -> f32 {
    static MADE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let made = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let seed = std::env::var("AUDIOVIZ_SEED").ok().and_then(|v| v.parse::<u64>().ok());
    let base = seed.unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64));
    crate::rng::Rng::seeded(&[base, made, def.id.len() as u64]).next()
}

impl ShaderVisual {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet, def: &'static ShaderDef) -> Result<Self, String> {
        let sh = shared(device);
        let (layout, out_layout, line_layout) = (sh.layout.clone(), sh.out_layout.clone(), sh.line_layout.clone());
        let art = art_texture(device);
        let line_uniform = scene::uniform_buffer(device, "loaded visual line", 16);
        let line_points = line_buffer(device, def.line);
        let (line_out_group, line_group) = line_groups(device, &out_layout, &line_layout, &line_points, &line_uniform);
        let camera_out = device.create_buffer(&wgpu::BufferDescriptor { label: Some("loaded visual camera"), size: CAMERA_BYTES, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("loaded visual camera out"), layout: &out_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_out.as_entire_binding() }] });
        let mut me = Self {
            line_cs: None,
            line_points,
            line_len: def.line,
            line_out_group,
            line_uniform,
            line_layout,
            line_group,
            line_draw: None,
            mesh: None,
            msaa: None,
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
            trail: def.trail.then(|| trail_textures(device, &def.id, width, height)),
            size: (width, height),
            black: hdr_texture(device, "loaded visual black", 1, 1),
            groups: Vec::new(),
            layout,
            pipeline: None,
            waiting: None,
            prefix_lines: 0,
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
            instance: instance_seed(def),
            device: device.clone(),
            pics: [picture_texture(device, None, 1, 1), picture_texture(device, None, 1, 1)],
            pic_size: [(1, 1); 2],
            pic_fade: 1.0,
            pic_count: 0,
            deck: None,
            deck_next: 0.0,
            pic_art: 0,
            mirror: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("loaded visual pictures"),
                address_mode_u: wgpu::AddressMode::MirrorRepeat,
                address_mode_v: wgpu::AddressMode::MirrorRepeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            presses: BTreeMap::new(),
            space: None,
            camera_out,
            camera_group,
            line_now: 0,
        };
        me.make_groups(device);
        me.reload(shaders);
        if me.waiting.is_none() && me.error.is_none() {
            me.build(shaders);
        }
        // A prewarmed visual is ready at once.
        me.collect();
        Ok(me)
    }

    fn make_groups(&mut self, device: &wgpu::Device) {
        let sh = shared(device);
        let group = |prev: &wgpu::TextureView, trail: &wgpu::TextureView, items: &wgpu::Buffer| {
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
                    wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&self.pics[0]) },
                    wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&self.pics[1]) },
                    wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::Sampler(&self.mirror) },
                    wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(trail) },
                    wgpu::BindGroupEntry { binding: 10, resource: sh.camera.as_entire_binding() },
                ],
            })
        };
        // Each output pairs with the trail written alongside it.
        let trail = |k: usize| self.trail.as_ref().map_or(&self.black, |t| &t[k]);
        self.groups = [(&self.black, &self.black), (&self.out[0], trail(0)), (&self.out[1], trail(1))].into_iter().flat_map(|(prev, t)| [group(prev, t, &self.items[0]), group(prev, t, &self.items[1])]).collect();
        self.item_groups = self
            .items
            .iter()
            .map(|b| device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("loaded visual items out"), layout: &self.out_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: b.as_entire_binding() }] }))
            .collect();
    }

    /// Ask the worker for the live code's pipelines (`collect` picks them up);
    /// meanwhile the old pipeline, if any, keeps drawing.
    fn build(&mut self, shaders: &ShaderSet) {
        self.prefix_lines = self.live.prefix(shaders).lines().count();
        self.waiting = Some(shared(&self.device).request(self.live.job(shaders), true));
    }

    /// Take the pipelines the worker built for the live code, or its error
    /// (keeping the old pipeline).
    fn collect(&mut self) {
        let Some(key) = self.waiting else { return };
        let Some(built) = shared(&self.device).result(key) else { return };
        self.waiting = None;
        let device = self.device.clone();
        match built {
            Ok(b) => {
                self.pipeline = Some(b.picture.clone());
                self.prepare = b.prepare.clone();
                self.line_cs = b.line.clone();
                self.mesh = b.mesh.clone();
                self.space = b.space.is_some().then(|| b.clone());
                if (b.samples > 1) != self.msaa.is_some() {
                    self.msaa = (b.samples > 1).then(|| msaa_target(&device, self.size.0, self.size.1));
                }
                if b.line_draw.is_some() {
                    self.line_draw = b.line_draw.clone();
                }
                self.error = None;
                if self.live.line != self.line_len {
                    self.line_points = line_buffer(&device, self.live.line);
                    self.line_len = self.live.line;
                    (self.line_out_group, self.line_group) = line_groups(&device, &self.out_layout, &self.line_layout, &self.line_points, &self.line_uniform);
                }
                // A reload may have changed how many items there are, or the trail.
                let trail = self.live.trail != self.trail.is_some();
                if trail {
                    self.trail = self.live.trail.then(|| trail_textures(&device, &self.def.id, self.size.0, self.size.1));
                }
                if self.live.items != self.items_len || trail {
                    self.items = item_buffers(&device, self.live.items);
                    self.items_len = self.live.items;
                    self.make_groups(&device);
                }
            }
            Err(e) => {
                let e = self::visual_lines(&e, self.prefix_lines, &self.live.wgsl);
                tracing::warn!("visual {}: {e}", self.live.id);
                self.error = Some(e);
            }
        }
    }

    /// Reload from the folder when either file changed.
    fn reload(&mut self, shaders: &ShaderSet) {
        let Some(dir) = self.def.dir.clone() else { return };
        let now = stamp(&dir);
        if now.is_none() || now == self.stamp {
            return;
        }
        self.stamp = now;
        let Ok(json) = std::fs::read_to_string(dir.join("visual.json")) else { return };
        match read_code(&dir, &json).and_then(|wgsl| parse(&json, &wgsl, Some(dir))) {
            Ok(mut d) => {
                // The id is what saved files point at; it stays the registered one.
                d.id = self.def.id.clone();
                self.live = d;
                self.build(shaders);
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
        if self.trail.is_some() {
            self.trail = Some(trail_textures(device, &self.def.id, width, height));
        }
        self.size = (width, height);
        if self.msaa.is_some() {
            self.msaa = Some(msaa_target(device, width, height));
        }
        self.make_groups(device);
    }

    fn reload_shaders(&mut self, _device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        // Errors arrive later, in the panel.
        self.build(shaders);
        Ok(())
    }

    fn level_bright(&self) -> bool {
        self.live.level_bright
    }

    fn ready(&self) -> bool {
        self.pipeline.is_some() || self.error.is_some()
    }

    fn poll(&mut self, _device: &wgpu::Device, shaders: &ShaderSet) {
        self.collect();
        if self.def.dir.is_some() && self.last_poll.elapsed().as_millis() > 400 {
            self.last_poll = Instant::now();
            self.reload(shaders);
        }
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        if self.update(queue, encoder, ctx, layer, None) {
            self.draw(encoder);
        }
    }

    fn joins_space(&self) -> bool {
        self.live.space && self.space.as_ref().is_some_and(|b| b.space.is_some())
    }

    fn leads_space(&self) -> bool {
        self.joins_space() && self.space.as_ref().and_then(|b| b.space.as_ref()).is_some_and(|s| s.camera.is_some())
    }

    fn space_glows(&self) -> bool {
        self.joins_space() && self.live.glow
    }

    fn space_update(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer, opacity: f32) {
        self.update(queue, encoder, ctx, layer, Some(opacity));
    }

    fn space_camera(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some(cs) = self.space.as_ref().and_then(|b| b.space.as_ref()).and_then(|s| s.camera.as_ref()) else { return };
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("loaded visual camera"), timestamp_writes: None });
            pass.set_pipeline(cs);
            pass.set_bind_group(0, &self.groups[self.icur], &[]);
            pass.set_bind_group(1, &self.camera_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.camera_out, 0, &shared(&self.device).camera, 0, CAMERA_BYTES);
    }

    fn space_draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(sp) = self.space.as_ref().and_then(|b| b.space.as_ref()) else { return };
        if self.pipeline.is_none() {
            return;
        }
        // No feedback in a space: `prev` reads black.
        pass.set_bind_group(0, &self.groups[self.icur], &[]);
        if self.live.shade {
            pass.set_pipeline(&sp.picture);
            pass.draw(0..3, 0..1);
        }
        if let Some(mesh) = sp.mesh.as_ref().filter(|_| self.live.triangles > 0) {
            pass.set_pipeline(mesh);
            pass.draw(0..self.live.triangles * 3, 0..1);
        }
        if let Some(line) = sp.line_draw.as_ref().filter(|_| self.line_now >= 2) {
            pass.set_pipeline(line);
            pass.set_bind_group(0, &self.line_group, &[]);
            pass.draw(0..6, 0..self.line_now - 1);
        }
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
                        ParamKind::Button => {
                            let r = ghost_button(ui, &p.label, false);
                            let r = if p.help.is_empty() { r } else { r.on_hover_text(&p.help) };
                            if r.clicked() {
                                *self.presses.entry(p.id.clone()).or_default() += 1;
                            }
                        }
                        ParamKind::Integer | ParamKind::Slider => {
                            let int = p.kind == ParamKind::Integer;
                            let mut x = v.as_f64().unwrap_or(0.0) as f32;
                            let d = def.as_f64().unwrap_or(0.0) as f32;
                            let span = (p.max - p.min).abs();
                            let unit = if p.unit.is_empty() { String::new() } else { format!(" {}", p.unit) };
                            // An integer with a zero setting: 0 reads as its name, and below min snaps to min.
                            let zero = (int && !p.zero.is_empty()).then(|| p.zero.clone());
                            let (lo, low, places) = (if zero.is_some() { 0.0 } else { p.min }, p.min as f64, p.decimals);
                            let r = slider_def(ui, &p.label, &mut x, Some(d), lo..=p.max, move |v| match &zero {
                                Some(z) if v < 0.5 => z.clone(),
                                Some(_) => format!("{:.0}{unit}", v.max(low)),
                                None if places.is_some() => format!("{v:.*}{unit}", places.unwrap_or(2)),
                                None if int || span > 20.0 => format!("{v:.0}{unit}"),
                                None if span > 2.0 => format!("{v:.1}{unit}"),
                                None => format!("{v:.2}{unit}"),
                            }, p.log);
                            if !p.help.is_empty() {
                                r.on_hover_text(&p.help);
                            }
                            if int && !p.zero.is_empty() && x >= 0.5 {
                                x = x.max(p.min);
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
    /// A frame before anything draws: the uniforms, the album art, the
    /// waveform, the prepare step and the line's points. `space` is the
    /// layer's opacity when it is in a shared space. False when there is no
    /// pipeline yet (on its own, the output is cleared).
    fn update(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer, space: Option<f32>) -> bool {
        self.collect();
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
        let mut p = [[0.0f32; 4]; MAX_PARAMS / 4];
        for (i, d) in self.live.params.iter().enumerate().take(MAX_PARAMS) {
            p[i / 4][i % 4] = if d.kind == ParamKind::Button { self.presses.get(&d.id).copied().unwrap_or(0) as f32 } else { d.number(stored.and_then(|o| o.get(&d.id))) };
        }
        let mut chroma_slow = [[0.0f32; 4]; 3];
        for (i, c) in f.chroma_slow.iter().enumerate() {
            chroma_slow[i / 4][i % 4] = *c;
        }
        self.feed_pictures(queue, ctx, stored, dt);
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
            instance: self.instance,
            picture_fade: { let t = self.pic_fade.clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) },
            picture_aspect: self.pic_size[0].0 as f32 / self.pic_size[0].1.max(1) as f32,
            picture_prev_aspect: self.pic_size[1].0 as f32 / self.pic_size[1].1.max(1) as f32,
            picture_count: self.pic_count as f32,
            confidence: f.beat_confidence,
            silent: f.silent as u32 as f32,
            _pad8: 0.0,
            hits: f.onset_hit.map(|h| h as u32 as f32),
            strengths: f.onset_strength,
            chroma_slow,
            space: [space.map_or(0.0, |_| 1.0), space.unwrap_or(1.0), 0.0, 0.0],
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
        if self.pipeline.is_none() {
            if space.is_none() {
                scene::clear_pass(encoder, "loaded visual (no shader)", &self.out[self.cur]);
            }
            self.line_now = 0;
            return false;
        }
        // With feedback (or a trail) the two outputs ping-pong and `prev`
        // reads the other; in a shared space there is none.
        let prev = if space.is_none() && (self.live.feedback || self.trail.is_some()) { 1 + self.cur } else { 0 };
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
            (Some(cs), Some(_)) if count >= 2 => {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("loaded visual line points"), timestamp_writes: None });
                pass.set_pipeline(cs);
                pass.set_bind_group(0, group, &[]);
                pass.set_bind_group(1, &self.line_out_group, &[]);
                pass.dispatch_workgroups(count.div_ceil(PREPARE_GROUP), 1, 1);
                Some(())
            }
            _ => None,
        };
        self.line_now = if line.is_some() { count } else { 0 };
        if self.line_now > 0 {
            let u: [u32; 4] = [(ctx.width as f32).to_bits(), (ctx.height as f32).to_bits(), count, self.live.beam as u32];
            queue.write_buffer(&self.line_uniform, 0, bytemuck::cast_slice(&u));
        }
        true
    }

    /// Draw this frame's picture, triangles and line into its own output
    /// (after `update`).
    fn draw(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(pipeline) = &self.pipeline else { return };
        let (write, prev) = if self.live.feedback || self.trail.is_some() { (1 - self.cur, 1 + self.cur) } else { (self.cur, 0) };
        let group = &self.groups[prev * 2 + self.icur];
        let line = self.line_draw.as_ref().filter(|_| self.line_now >= 2);
        let count = self.line_now;
        let mesh = self.mesh.as_ref().filter(|_| self.live.triangles > 0);
        // The passes in order (picture, triangles, line); the first clears.
        let picture = self.live.shade || (line.is_none() && mesh.is_none());
        if let (Some(ms), Some(mesh)) = (&self.msaa, mesh) {
            // Antialiased: the picture and the triangles in one multisampled
            // pass, resolved into the output.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loaded visual antialiased"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: ms, depth_slice: None, resolve_target: Some(&self.out[write]), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Discard } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, group, &[]);
            if picture {
                pass.set_pipeline(pipeline);
                pass.draw(0..3, 0..1);
            }
            pass.set_pipeline(mesh);
            pass.draw(0..self.live.triangles * 3, 0..1);
        } else if picture {
            match &self.trail {
                Some(t) if self.live.trail => scene::fullscreen_pass2(encoder, &self.def.id, &self.out[write], &t[write], pipeline, group),
                _ => fullscreen_pass(encoder, &self.def.id, &self.out[write], wgpu::LoadOp::Clear(wgpu::Color::BLACK), pipeline, group),
            }
        }
        if let (Some(mesh), None) = (mesh, &self.msaa) {
            let load = if picture { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) };
            let mut pass = scene::begin_pass(encoder, "loaded visual triangles", &self.out[write], load);
            pass.set_pipeline(mesh);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..self.live.triangles * 3, 0..1);
        }
        let picture = picture || mesh.is_some();
        if let Some(draw) = line {
            let load = if picture { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) };
            let mut pass = scene::begin_pass(encoder, "loaded visual line", &self.out[write], load);
            pass.set_pipeline(draw);
            pass.set_bind_group(0, &self.line_group, &[]);
            pass.draw(0..6, 0..count - 1);
        }
        self.cur = write;
    }
}

impl ShaderVisual {
    /// A new picture replaces the current one, which becomes the previous.
    fn incoming(&mut self, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) {
        let view = picture_texture(&self.device, Some((queue, rgba)), width, height);
        self.pics.swap(0, 1);
        self.pic_size.swap(0, 1);
        self.pics[0] = view;
        self.pic_size[0] = (width.max(1), height.max(1));
        self.pic_fade = 0.0;
        self.pic_count += 1;
        let device = self.device.clone();
        self.make_groups(&device);
    }

    /// The picture feed: photos from the folder when the visual's source
    /// control says so and the folder is set, else the album art.
    fn feed_pictures(&mut self, queue: &wgpu::Queue, ctx: &FrameCtx, stored: Option<&Map<String, Value>>, dt: f32) {
        let Some(pics) = self.live.pictures.clone() else { return };
        let number = |id: &str| self.live.params.iter().find(|d| d.id == id).map(|d| d.number(stored.and_then(|o| o.get(id))));
        let want_photos = number(&pics.from).is_some_and(|v| v > 0.5) && !ctx.photos_folder.trim().is_empty();
        if want_photos {
            if self.deck.as_ref().is_none_or(|d| d.folder != ctx.photos_folder) {
                let mut d = crate::pictures::PhotoDeck::spawn(ctx.photos_folder);
                d.request();
                self.deck = Some(d);
            }
        } else {
            self.deck = None;
        }
        let live = self.deck.as_ref().is_some_and(|d| d.files > 0);
        if live {
            let dwell = number(&pics.dwell).unwrap_or(15.0).max(2.0);
            let deck = self.deck.as_mut().expect("live");
            if let Some(p) = deck.poll() {
                self.incoming(queue, p.width, p.height, &p.rgba);
                self.deck_next = ctx.time + dwell;
            } else if ctx.time >= self.deck_next {
                deck.request();
            }
        } else if let Some(art) = ctx.art {
            if ctx.art_id != self.pic_art {
                self.pic_art = ctx.art_id;
                self.incoming(queue, art.width, art.height, &art.rgba);
            }
        }
        self.pic_fade = (self.pic_fade + dt / PICTURE_FADE_S).min(1.0);
    }

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
        assert_eq!(visual_lines("x wgsl:593:5 y", 590, "a\nb\nc\n"), "x visual.wgsl line 3, col 5 y");
        assert_eq!(visual_lines("wgsl:12:1", 590, ""), "the engine's part, line 1");
        // A visual in parts: errors name the part and its own line.
        let code = with_parts(r#"{"parts": ["b.wgsl"]}"#, "one\ntwo", |n| (n == "b.wgsl").then(|| "three\nfour".to_string())).unwrap();
        assert_eq!(visual_lines("wgsl:592:1", 590, &code), "visual.wgsl line 2, col 1");
        assert_eq!(visual_lines("wgsl:595:1", 590, &code), "b.wgsl line 2, col 1");
        assert!(with_parts(r#"{"parts": ["../x.wgsl"]}"#, "", |_| Some(String::new())).is_err());
        assert!(with_parts(r#"{"parts": ["gone.wgsl"]}"#, "", |_| None).is_err());
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
        assert_eq!(std::mem::offset_of!(Uniform, bands), 208);
        assert_eq!(std::mem::offset_of!(Uniform, chroma), 464);
        assert_eq!(std::mem::offset_of!(Uniform, palette), 512);
        assert_eq!(std::mem::offset_of!(Uniform, wave_new), 512 + 512);
        assert_eq!(std::mem::offset_of!(Uniform, hits), 512 + 512 + 48);
        assert_eq!(std::mem::offset_of!(Uniform, chroma_slow), 512 + 512 + 80);
        assert_eq!(std::mem::offset_of!(Uniform, space), 512 + 512 + 128);
        assert_eq!(std::mem::size_of::<Uniform>(), 512 + 512 + 144);
    }

    #[test]
    fn factory_visuals_parse() {
        // Every shipped folder parses (a broken one would only be skipped with a warning).
        let folders: std::collections::BTreeSet<&str> = factory::embedded("visuals/").into_iter().filter(|(rel, _)| rel.ends_with("/visual.json")).filter_map(|(rel, _)| rel.split('/').nth(1)).collect();
        let all = embedded();
        assert_eq!(all.len(), folders.len(), "a factory visual did not parse (see the warning above)");
        for d in &all {
            assert!(!d.wgsl.is_empty(), "{}", d.id);
        }
        // A visual in parts carries them all.
        let jelly = all.iter().find(|d| d.id == "jellyfish").expect("jellyfish");
        assert!(jelly.wgsl.contains(&part_marker("tank.wgsl")));
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
        assert!(parse(r#"{"id": "x", "name": "x", "items": 999999}"#, prep, None).is_err());
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
    fn calls_finds_a_call_not_a_definition_or_a_comment() {
        assert!(calls("    c += x;\n    write_glow(c);", "write_glow"));
        assert!(calls("let y = write_glow (c);", "write_glow"));
        assert!(!calls("fn write_glow(c: vec3<f32>) {", "write_glow"));
        assert!(!calls("// write_glow(c)", "write_glow"));
        assert!(!calls("my_write_glow(c);", "write_glow"));
        let jelly = embedded().into_iter().find(|d| d.id == "jellyfish").expect("jellyfish");
        assert!(jelly.glow && jelly.space);
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
            let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}: {}", d.id, visual_lines(&e.emit_to_string(&src), d.prefix(&shaders).lines().count(), &d.wgsl)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{}: {e:?}", d.id));
        }
    }
}
