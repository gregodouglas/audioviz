// The API a loaded visual is written against (see docs/visuals.md). The
// engine prepends this file (after the shared prelude: FsIn, the hashes, ign)
// to the visual's own `visual.wgsl`, adds a `Params` struct and `params()`
// built from its `visual.json`, and calls its
//     fn shade(uv: vec2<f32>) -> vec3<f32>
// once per pixel. uv is 0..1 with y down; `centred(uv)` gives coordinates
// with the short side's half-extent 1 and y up. The colour is linear HDR
// light; the layer's effects, levelling and the Picture handle the rest.

struct VisualIn {
    resolution: vec2<f32>,
    time: f32,          // seconds since start
    dt: f32,            // seconds since the last frame
    flow: f32,          // a music clock: advances 0.5 to ~1.7 s per second with the energy
    energy: f32,        // overall level against its own average, ~1 typical, 0..2.5
    bass: f32,          // the bands against their averages, ~1 typical
    mid: f32,
    treble: f32,
    beat: f32,          // beat pulse 0..1 (times confidence), jumps on the beat and decays
    beat_phase: f32,    // 0..1 through the current beat
    beat_count: f32,    // beats since start
    bpm: f32,
    onset: f32,         // any-band onset pulse 0..1
    swell: f32,         // bass above its average plus low onsets, fast attack, slow release, 0..~2
    key: f32,           // dominant pitch class 0..11 (C = 0)
    onsets: vec4<f32>,  // onset pulses: low, mid, high, all
    tint: vec4<f32>,    // the layer's tint (rgb)
    palette_mix: f32,   // how much the layer's palette replaces the tint (0 = no palette)
    palette_shift: f32, // the palette's moving offset, 0..1
    quiet: f32,         // 1 in a quiet passage
    has_art: f32,       // 1 when the playing track has album art (art() is black otherwise)
    p: array<vec4<f32>, 4>,        // parameters 0..15 (use params())
    bands: array<vec4<f32>, 8>,    // 32 bands, low to high, smoothed (30 ms attack, 200 ms release)
    raw: array<vec4<f32>, 8>,      // the same, unsmoothed
    chroma_v: array<vec4<f32>, 3>, // 12 pitch classes, C first
    palette: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var<uniform> V: VisualIn;
@group(0) @binding(1) var<storage, read> wave_buf: array<vec2<f32>>;
@group(0) @binding(2) var prev_tex: texture_2d<f32>;
@group(0) @binding(3) var prev_samp: sampler;
// Per-item values from prepare() (see item()); one placeholder without it.
@group(0) @binding(4) var<storage, read> items_buf: array<vec4<f32>>;
@group(0) @binding(5) var art_tex: texture_2d<f32>;

const TAU: f32 = 6.2831853;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

// Arrays of vec4 are indexed directly (component by a runtime index),
// which is much cheaper in a loop than picking through a switch.
fn param(i: u32) -> f32 {
    let k = min(i, 15u);
    return V.p[k / 4u][k % 4u];
}

// Band i (0..31, low to high), smoothed.
fn band(i: u32) -> f32 {
    let k = min(i, 31u);
    return V.bands[k / 4u][k % 4u];
}

fn band_raw(i: u32) -> f32 {
    let k = min(i, 31u);
    return V.raw[k / 4u][k % 4u];
}

// The smoothed spectrum at t (0 = lowest band, 1 = highest), interpolated.
fn band_at(t: f32) -> f32 {
    let x = clamp(t, 0.0, 1.0) * 31.0;
    let i = u32(floor(x));
    return mix(band(i), band(min(i + 1u, 31u)), fract(x));
}

// Pitch class i (0..11, C first).
fn chroma(i: u32) -> f32 {
    let k = i % 12u;
    return V.chroma_v[k / 4u][k % 4u];
}

// The layer's colour at t (0..1, wraps): its palette (moving with the
// palette's cycle) mixed over its tint by the palette amount.
fn colour(t: f32) -> vec3<f32> {
    let x = fract(t + V.palette_shift) * 32.0;
    let i = u32(floor(x)) % 32u;
    let pal = mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
    return mix(V.tint.rgb, pal, V.palette_mix);
}

// The waveform: t 0 = oldest .. 1 = newest of the last 1024 samples, (left, right), auto-gained.
fn wave(t: f32) -> vec2<f32> {
    let n = arrayLength(&wave_buf);
    let x = clamp(t, 0.0, 1.0) * f32(n - 1u);
    let i = u32(floor(x));
    return mix(wave_buf[i], wave_buf[min(i + 1u, n - 1u)], fract(x));
}

fn wave_mono(t: f32) -> f32 {
    let w = wave(t);
    return 0.5 * (w.x + w.y);
}

// Item i as prepare() computed it (zero without a prepare step). In shade()
// it is this frame's value; inside prepare() it is last frame's, so a
// prepare step can keep state: smooth, integrate, remember.
fn item(i: u32) -> vec4<f32> {
    return items_buf[min(i, arrayLength(&items_buf) - 1u)];
}

// Last frame's output at uv (black unless the manifest sets "feedback": true).
fn prev(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(prev_tex, prev_samp, uv, 0.0).rgb;
}

// The playing track's album art at uv (0..1, y down, square), as linear
// colour; black when there is none (V.has_art is 0). It changes with the track.
fn art(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(art_tex, prev_samp, uv, 0.0).rgb;
}

// Centred coordinates: the short side's half-extent is 1, y up.
fn centred(uv: vec2<f32>) -> vec2<f32> {
    let short = min(V.resolution.x, V.resolution.y);
    return (uv * 2.0 - 1.0) * vec2<f32>(V.resolution.x / short, -V.resolution.y / short);
}

// The inverse of centred(): back to uv, for reading prev() at a moved place.
fn uncentred(p: vec2<f32>) -> vec2<f32> {
    let short = min(V.resolution.x, V.resolution.y);
    return (p / vec2<f32>(V.resolution.x / short, -V.resolution.y / short)) * 0.5 + 0.5;
}

// One pixel in centred units.
fn pixel() -> f32 {
    return 2.0 / min(V.resolution.x, V.resolution.y);
}

// p turned by angle a (radians, counter-clockwise).
fn rotate(p: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
}

// Smooth value noise, 0..1, one feature per unit of p.
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
