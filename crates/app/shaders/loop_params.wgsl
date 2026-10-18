// Loop uniform (scenes/loops.rs::Uniform). In the prelude so loop.wgsl sees it.
struct LoopParams {
    resolution: vec2<f32>,
    rings: u32,
    cols: u32,
    head: u32,            // ring index of the newest ring
    colour: u32,          // 0 palette by age, 1 palette by level
    _pad0a: u32,
    _pad0b: u32,
    inner: f32,           // newest ring's radius, fraction of the short half-side
    outer: f32,           // oldest ring's radius
    amp: f32,             // waveform displacement, same units
    width: f32,           // line width, pixels
    fade: f32,            // how dim the oldest rings get
    level: f32,           // 0 equal brightness .. 1 brightness by the ring's level
    spin: f32,            // accumulated turn, radians
    twist: f32,           // extra turn per ring, radians
    tint: vec4<f32>,
    palette_mix: f32,
    palette_shift: f32,
    _pad1a: f32,
    _pad1b: f32,
    palette: array<vec4<f32>, 32>,
};

fn loop_pal(lp: LoopParams, t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(lp.palette[i].rgb, lp.palette[(i + 1u) % 32u].rgb, fract(x));
}
