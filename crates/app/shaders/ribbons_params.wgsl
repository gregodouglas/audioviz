// Ribbons scene uniform (scenes/ribbons.rs::RibbonParams, 160 bytes). The
// first 64 bytes mirror ScopeParams so decay.wgsl can be shared.
struct RibbonParams {
    resolution: vec2<f32>,
    gain: f32,
    rotation: f32,
    beam_width: f32,
    beam_intensity: f32,
    decay: f32,
    _pad: f32,
    tint: vec4<f32>,
    point_count: u32,
    _pad2a: u32,
    _pad2b: u32,
    _pad2c: u32,
    chroma: array<vec4<f32>, 3>,
    time: f32,
    dt: f32,
    speed: f32,
    field_scale: f32,
    onset: f32,
    bass: f32,
    key_hue: f32,
    saturation: f32,
    seed: f32,
    vortex: f32,
    curl: f32,
    lifetime: f32,
    palette_mix: f32,
    palette_shift: f32,
    _pp0: f32,
    _pp1: f32,
    palette: array<vec4<f32>, 32>,
};

fn ribbon_pal(rp: RibbonParams, t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    let f = fract(x);
    return mix(rp.palette[i].rgb, rp.palette[(i + 1u) % 32u].rgb, f);
}

struct Particle {
    pos: vec2<f32>,
    prev: vec2<f32>,
    age: f32,
    life: f32,
    seed: f32,
    pitch: f32,
};

fn chroma_at(rp: RibbonParams, i: u32) -> f32 {
    let v = rp.chroma[i / 4u];
    switch (i % 4u) {
        case 0u: { return v.x; }
        case 1u: { return v.y; }
        case 2u: { return v.z; }
        default: { return v.w; }
    }
}

fn hsv2rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let k = vec3<f32>(0.0, 2.0, 4.0) + h * 6.0;
    let f = abs(fract(k / 6.0) * 6.0 - 3.0) - 1.0;
    return v * mix(vec3<f32>(1.0), clamp(f, vec3<f32>(0.0), vec3<f32>(1.0)), s);
}

// 3D value noise with smooth interpolation, for a slowly evolving 2D field.
fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn vnoise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash31(i + vec3<f32>(0.0, 0.0, 0.0)), hash31(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let b = mix(hash31(i + vec3<f32>(0.0, 1.0, 0.0)), hash31(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let c = mix(hash31(i + vec3<f32>(0.0, 0.0, 1.0)), hash31(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let d = mix(hash31(i + vec3<f32>(0.0, 1.0, 1.0)), hash31(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z) * 2.0 - 1.0;
}

fn potential(p: vec2<f32>, t: f32) -> f32 {
    var s = 0.0;
    s += vnoise3(vec3<f32>(p, t)) * 1.0;
    s += vnoise3(vec3<f32>(p * 2.1 + 17.0, t * 1.3)) * 0.5;
    s += vnoise3(vec3<f32>(p * 4.3 + 31.0, t * 1.7)) * 0.25;
    return s;
}
