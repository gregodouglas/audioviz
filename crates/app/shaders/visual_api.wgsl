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
    p: array<vec4<f32>, 8>,        // parameters 0..31 (use params())
    bands: array<vec4<f32>, 8>,    // 32 bands, low to high, smoothed (30 ms attack, 200 ms release)
    raw: array<vec4<f32>, 8>,      // the same, unsmoothed
    chroma_v: array<vec4<f32>, 3>, // 12 pitch classes, C first
    palette: array<vec4<f32>, 32>,
    wave_new: f32,      // how many of sample()'s newest samples arrived since last frame
    auto_gain: f32,     // the gain already applied to the waveform (divide to undo it)
    sample_rate: f32,   // samples per second
    track: f32,         // 0..1, a hash of the playing track (changes with the track; seeds)
    instance: f32,      // 0..1, random per layer: two layers of one visual differ
    picture_fade: f32,  // 0..1, eased: how far picture() has replaced picture_prev()
    picture_aspect: f32,        // width / height of picture()
    picture_prev_aspect: f32,   // and of picture_prev()
    picture_count: f32, // pictures shown so far (goes up by one with each new one)
    confidence: f32,    // 0..1, how sure the beat clock is of the beat
    silent: f32,        // 1 when nothing is playing
    width: f32,         // stereo width 0..1 (use stereo_width())
    hits: vec4<f32>,    // 1 on the frame an onset is detected (low, mid, high, all), else 0
    strengths: vec4<f32>, // onset strength now, continuous (low, mid, high, all)
    chroma_slow_v: array<vec4<f32>, 3>, // 12 pitch classes, slow profile (the chord over seconds)
    space: vec4<f32>,   // x: 1 in a shared space (in_space() reads it); y: the layer's opacity there (the engine applies it); z: the design layers shown, as bits (layer_on())
    levels_v: array<vec4<f32>, 8>,       // the 32 bands on one scale (use level())
    attacks_v: array<vec4<f32>, 8>,      // their attacks (use attack())
    broad_v: array<vec4<f32>, 2>,        // six broad bands' level (use broad())
    broad_rel_v: array<vec4<f32>, 2>,    // their value against their own average (broad_rel())
    broad_attack_v: array<vec4<f32>, 2>, // their attacks (broad_attack())
    noisiness: f32,     // 0 tonal (notes, chords) .. 1 noise-like (hats, cymbals, distortion)
    balance: f32,       // -1 all left .. 1 all right
    activity: f32,      // 0..1, how busy: onsets a second over the last couple of seconds (1 = eight or more)
    brightness: f32,    // 0..1, where the sound's weight sits: 200 Hz .. 8 kHz on a log scale
    // The music's shape over bars and seconds (ADR-0006). Read them by name
    // with the functions below (bar_phase(), build(), drop_pulse() ...).
    bar: vec4<f32>,     // bar phase, beat in the bar, how sure of the bar, pulse steadiness
    context: vec4<f32>, // build, bass presence, contrast, novelty
    trends: vec4<f32>,  // brightness, density and energy trends, bars into the section
    moments: vec4<f32>, // drop pulse, how sure of the drop, change pulse, phrase phase
    section: vec4<f32>, // section label, seen before, bars to the expected change, how sure
    extra: vec4<f32>,   // loudness, drops so far, changes so far, 0
};

@group(0) @binding(0) var<uniform> V: VisualIn;
@group(0) @binding(1) var<storage, read> wave_buf: array<vec2<f32>>;
@group(0) @binding(2) var prev_tex: texture_2d<f32>;
@group(0) @binding(3) var prev_samp: sampler;
// Per-item values from prepare() (see item()); one placeholder without it.
@group(0) @binding(4) var<storage, read> items_buf: array<vec4<f32>>;
@group(0) @binding(5) var art_tex: texture_2d<f32>;
@group(0) @binding(6) var picture_tex: texture_2d<f32>;
@group(0) @binding(7) var picture_prev_tex: texture_2d<f32>;
@group(0) @binding(8) var mirror_samp: sampler;
// Last frame's trail (with "trail": true; black otherwise).
@group(0) @binding(9) var trail_tex: texture_2d<f32>;
// The shared space's camera (see scene_camera()): (yaw, pitch, dist, focal), (look, 0).
@group(0) @binding(10) var<storage, read> scene_cam: array<vec4<f32>, 2>;
// What the layer's light goes through on its way to the screen (see steady()).
@group(0) @binding(11) var seen_tex: texture_2d<f32>;

const TAU: f32 = 6.2831853;

// A camera looking at `look` from `dist` away, turned `yaw` about the
// vertical and tipped `pitch` (radians); `focal` is centred units per unit
// of depth. The "creatures" library works with it (to_camera, project,
// clip_pos); a visual that can lead a shared space says where its camera
// is with fn camera() -> Camera.
struct Camera {
    yaw: f32,
    pitch: f32,
    dist: f32,
    focal: f32,
    look: vec3<f32>,
};

// Whether design layer i (the i-th name in visual.json's "layers") is
// shown. Always true in the app; the lab shows them alone or together.
fn layer_on(i: u32) -> bool {
    return ((u32(V.space.z) >> min(i, 15u)) & 1u) == 1u;
}

// Multiply light that should hold still on screen by it (a backdrop, say,
// that the bright things' levelling would otherwise pump as they flash): it
// undoes the swings of the layer's levelling and the Picture's auto
// exposure over the last ~8 s, leaving their slow drift. 1 when neither
// applies, and in the first frame.
fn steady() -> f32 {
    let s = textureLoad(seen_tex, vec2<i32>(0, 0), 0);
    return select(1.0, s.r, s.a > 0.5);
}

// Covering (ADR-0005). A visual with "covers": true draws its triangles
// twice in a shared space: first the cover pass, with covering() true,
// where what paint returns with alpha from a half is solid (it hides what
// is behind it, and what comes later behind it is left out) and the rest
// is dropped; then with everything else, covering() false, as light.
// cover_pass() says whether the cover pass runs (a shared space and
// "covers"); without it a visual draws its solid parts with the rest.
var<private> covering_now: bool;

fn covering() -> bool {
    return covering_now;
}

fn cover_pass() -> bool {
    return COVERS && in_space();
}

// The near plane of a shared space's depth (reversed Z: a point at view
// depth d is at SPACE_NEAR / d, the far plane at 0).
const SPACE_NEAR: f32 = 0.05;

// 1 when the layer is in a shared space (with "space": true in visual.json).
fn in_space() -> bool {
    return V.space.x > 0.5;
}

// The shared space's camera this frame: the leading layer's camera(). Only
// meaningful in a shared space (see in_space()).
fn scene_camera() -> Camera {
    let a = scene_cam[0];
    let b = scene_cam[1];
    return Camera(a.x, a.y, a.z, a.w, b.xyz);
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

// Arrays of vec4 are indexed directly (component by a runtime index),
// which is much cheaper in a loop than picking through a switch.
fn param(i: u32) -> f32 {
    let k = min(i, 23u);
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

// Band i on one scale for all bands (0..1 over the 42 dB under the
// track's loud level): a quiet band reads low however long it is quiet,
// so the bands keep their balance. band() is each band against its own
// recent best instead.
fn level(i: u32) -> f32 {
    let k = min(i, 31u);
    return V.levels_v[k / 4u][k % 4u];
}

// How far band i rose in the last ~30 ms, 0..1, fading over ~80 ms: the
// band's punch. A held note gives none.
fn attack(i: u32) -> f32 {
    let k = min(i, 31u);
    return V.attacks_v[k / 4u][k % 4u];
}

// The six broad bands, for broad(), broad_rel() and broad_attack().
const SUB: u32 = 0u;       // 30-60 Hz
const BASS: u32 = 1u;      // 60-250 Hz
const LOW_MID: u32 = 2u;   // 250-500 Hz
const MID: u32 = 3u;       // 500 Hz-2 kHz
const PRESENCE: u32 = 4u; // 2-6 kHz
const AIR: u32 = 5u;       // 6-16 kHz

// A broad band's level on the bands' one scale, 0..1.
fn broad(k: u32) -> f32 {
    let i = min(k, 5u);
    return V.broad_v[i / 4u][i % 4u];
}

// A broad band against its own average (1 = typical, up to 3).
fn broad_rel(k: u32) -> f32 {
    let i = min(k, 5u);
    return V.broad_rel_v[i / 4u][i % 4u];
}

// A broad band's attack, 0..1.
fn broad_attack(k: u32) -> f32 {
    let i = min(k, 5u);
    return V.broad_attack_v[i / 4u][i % 4u];
}

// The smoothed spectrum at t (0 = lowest band, 1 = highest), interpolated.
fn band_at(t: f32) -> f32 {
    let x = clamp(t, 0.0, 1.0) * 31.0;
    let i = u32(floor(x));
    return mix(band(i), band(min(i + 1u, 31u)), fract(x));
}

// level() at t (0 = lowest band, 1 = highest), interpolated: the spectrum
// in balance, quiet bands low.
fn level_at(t: f32) -> f32 {
    let x = clamp(t, 0.0, 1.0) * 31.0;
    let i = u32(floor(x));
    return mix(level(i), level(min(i + 1u, 31u)), fract(x));
}

// attack() at t, interpolated.
fn attack_at(t: f32) -> f32 {
    let x = clamp(t, 0.0, 1.0) * 31.0;
    let i = u32(floor(x));
    return mix(attack(i), attack(min(i + 1u, 31u)), fract(x));
}

// The sound's character, 0..1 unless said.

// How loud, 0..1 (momentary loudness from -40 to -5 LUFS): the level
// itself, not against its average as V.energy is.
fn loudness() -> f32 {
    return V.extra.x;
}

// How wide the stereo picture is, 0 mono .. 1 wide.
fn stereo_width() -> f32 {
    return V.width;
}

// The bar and the beat in it (ADR-0006 step 3). The bar is four beats.

// 0..1 through the bar.
fn bar_phase() -> f32 {
    return V.bar.x;
}

// Which beat of the bar this is: 0 (the first) .. 3.
fn beat_in_bar() -> f32 {
    return V.bar.y;
}

// How sure the analysis is where the bar starts, 0..1. The chords changing
// find it; kicks alone cannot tell the first beat from the third.
fn bar_sure() -> f32 {
    return V.bar.z;
}

// The beat pulse on the bar's first beat only, times bar_sure().
fn first_beat() -> f32 {
    return select(0.0, V.beat * V.bar.z, V.bar.y < 0.5);
}

// How steady the pulse has been over the last ~8 s, 0..1.
fn pulse_steady() -> f32 {
    return V.bar.w;
}

// Context over seconds (ADR-0006 step 4): where the music is heading. They
// fade in over the first ~10 s of a track.

// Build intensity, 0..1: energy, brightness, density and noise rising and
// the bass taken out, over seconds. High before a drop.
fn build() -> f32 {
    return V.context.x;
}

// How much sub and bass there is over the last few seconds, 0..1.
fn bass_presence() -> f32 {
    return V.context.y;
}

// How unlike the last minute the last few seconds are, 0..1.
fn contrast() -> f32 {
    return V.context.z;
}

// Novelty: the last 1.5 s against the 4 s before, against its usual level
// (0..1.5; about 1 at a change).
fn novelty() -> f32 {
    return V.context.w;
}

// Trends over seconds, -1 falling .. 1 rising.
fn brightness_trend() -> f32 {
    return V.trends.x;
}

fn density_trend() -> f32 {
    return V.trends.y;
}

fn energy_trend() -> f32 {
    return V.trends.z;
}

// 1 at a drop (after a build, the bass and energy back at once), fading
// over ~0.6 s; known as it happens. drop_sure() is how sure that one was.
fn drop_pulse() -> f32 {
    return V.moments.x;
}

fn drop_sure() -> f32 {
    return V.moments.y;
}

// 1 at a change (a transition: the sound turns into something else),
// fading; known about 1.5 s after it.
fn change_pulse() -> f32 {
    return V.moments.z;
}

// Drops and changes so far in the track: when one goes up, an event
// happened (keep last frame's in a prepare item to see it once).
fn drop_count() -> f32 {
    return V.extra.y;
}

fn change_count() -> f32 {
    return V.extra.z;
}

// Sections from the track's own history (ADR-0006 step 4). Approximate.

// 0..1 through the eight-bar phrase.
fn phrase_phase() -> f32 {
    return V.moments.w;
}

// The section's label: 0 = A, 1 = B ... A section that sounds like an
// earlier one gets that one's label, so a chorus comes back as its letter.
fn section() -> f32 {
    return V.section.x;
}

// How much this section sounds like an earlier one, 0..1.
fn seen_before() -> f32 {
    return V.section.y;
}

// Whole bars since this section began.
fn bars_in_section() -> f32 {
    return V.trends.w;
}

// Bars to the next expected change (an earlier section's length when this
// one matches it, else the next eight-bar phrase), and how sure, 0..1.
fn bars_to_change() -> f32 {
    return V.section.z;
}

fn change_sure() -> f32 {
    return V.section.w;
}

// A value 0..1 that belongs to the current section in this track: each
// section label has its own, so a section that comes back (a chorus) gets
// its value back. salt picks another value for another use. It steps when
// the section changes: ease it (see ease()) in a prepare item.
fn section_seed(salt: f32) -> f32 {
    return hash21(vec2<f32>(section() * 7.31 + salt * 17.13 + 1.0, V.track * 97.0 + salt));
}

// last moved toward want for one frame: rising with a time constant of
// `rise` seconds, falling with `fall`. Kept in a prepare item, a value that
// steps (a seed, a count, a choice) glides instead (ADR-0004: every level
// eases).
fn ease(last: f32, want: f32, rise: f32, fall: f32) -> f32 {
    let tau = max(select(fall, rise, want > last), 1e-4);
    return last + (want - last) * (1.0 - exp(-clamp(V.dt, 0.0, 0.1) / tau));
}

// Pitch class i (0..11, C first).
fn chroma(i: u32) -> f32 {
    let k = i % 12u;
    return V.chroma_v[k / 4u][k % 4u];
}

// Pitch class i, the slow profile: the chord over the last seconds.
fn chroma_slow(i: u32) -> f32 {
    let k = i % 12u;
    return V.chroma_slow_v[k / 4u][k % 4u];
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
    let x = f32(n - 1024u) + clamp(t, 0.0, 1.0) * 1023.0;
    let i = u32(floor(x));
    return mix(wave_buf[i], wave_buf[min(i + 1u, n - 1u)], fract(x));
}

// Sample i of the last 4096, newest first (0 = the newest), (left, right),
// auto-gained. The V.wave_new newest arrived since last frame.
fn sample(i: u32) -> vec2<f32> {
    let n = arrayLength(&wave_buf);
    return wave_buf[n - 1u - min(i, n - 1u)];
}

fn wave_mono(t: f32) -> f32 {
    let w = wave(t);
    return 0.5 * (w.x + w.y);
}

// One point of a visual's line (with "line": N in visual.json, fn point(i)
// returns point i): where it is in centred coordinates, the line's width
// there in pixels (0 breaks the line), and its colour (linear light). In a
// shared space, `depth` is its distance from the eye along the view (the
// camera's z; 0, the default, is the far plane), so the space knows where
// its light comes from.
struct LinePoint {
    pos: vec2<f32>,
    width: f32,
    colour: vec3<f32>,
    depth: f32,
};

// One corner of a triangle (with "triangles": N in visual.json, fn vertex(i)
// returns corner i, three per triangle, drawn in order): where it is in
// centred coordinates with w for perspective (pos = (x, y, 0, 1) when flat),
// its colour as premultiplied light plus coverage (a = 0 adds light, a = 1
// covers what is under it), and four free values handed to paint().
struct MeshVertex {
    pos: vec4<f32>,
    colour: vec4<f32>,
    data: vec4<f32>,
};

// What paint(f) gets for each pixel of a triangle: the colour and data
// interpolated across it, and the pixel's position in pixels.
struct MeshFragment {
    colour: vec4<f32>,
    data: vec4<f32>,
    pixel: vec2<f32>,
};

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

// The trail (with "trail": true in visual.json): a second picture that
// persists. trail(uv) is what shade() wrote with write_trail() last frame;
// whatever shade() writes this frame is kept for the next (black if it
// writes nothing). The trail is not drawn: add it to what shade returns.
var<private> trail_out: vec3<f32>;

fn trail(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(trail_tex, prev_samp, uv, 0.0).rgb;
}

fn write_trail(c: vec3<f32>) {
    trail_out = c;
}

// Light that diffuses (in a shared space): what paint() adds with
// write_glow(c) is blurred through the space, close round where it was
// written, and added to the picture, as light spreading through the body
// it is in. It comes on top of what paint returns, which stays sharp;
// outside a shared space it is not drawn, so a visual that moves light into
// the glow keeps it in its colour there (see in_space()). It adds up over
// calls. (shade() may call it, but its glow is not kept yet.)
var<private> glow_out: vec3<f32>;

fn write_glow(c: vec3<f32>) {
    glow_out += c;
}

// The far plane of a shared space's light depth.
const LIGHT_FAR: f32 = 50.0;

// Where shade()'s light is, as distance from the eye along the view, in a
// shared space (the far plane unless it calls this). Triangles and the line
// give theirs from where they are. Nothing reads light depth yet; depth of
// field and refraction will.
var<private> depth_out: f32 = LIGHT_FAR;

fn write_depth(z: f32) {
    depth_out = z;
}

// The playing track's album art at uv (0..1, y down, square), as linear
// colour; black when there is none (V.has_art is 0). It changes with the track.
fn art(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(art_tex, prev_samp, uv, 0.0).rgb;
}

// The current picture (with "pictures" in visual.json: the album art or a
// photo from the photos folder) at uv, 0..1 across it, y down, repeating
// mirrored outside; linear colour. picture_prev() is the one before, which
// picture() replaces over V.picture_fade.
fn picture(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(picture_tex, mirror_samp, uv, 0.0).rgb;
}

fn picture_prev(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(picture_prev_tex, mirror_samp, uv, 0.0).rgb;
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
