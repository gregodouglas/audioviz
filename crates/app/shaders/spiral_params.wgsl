// Spiral uniform (scenes/spiral.rs::Uniform). In the prelude so spiral.wgsl sees it.
struct SpiralParams {
    resolution: vec2<f32>,
    points: u32,
    shape: u32,           // 0 spiral, 1 rose
    wave: u32,            // waveform samples in the buffer
    petals: f32,          // rose k
    step: f32,            // angle between consecutive points, radians (drifting)
    spin: f32,            // accumulated turn, radians
    radius: f32,          // fraction of the short half-side
    amp: f32,             // waveform displacement, same units
    width: f32,           // line width, pixels
    intensity: f32,
    tint: vec4<f32>,
    palette_mix: f32,
    palette_shift: f32,
    _pad0: f32,
    _pad1: f32,
    palette: array<vec4<f32>, 32>,
};

fn spiral_pal(sp: SpiralParams, t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(sp.palette[i].rgb, sp.palette[(i + 1u) % 32u].rgb, fract(x));
}
