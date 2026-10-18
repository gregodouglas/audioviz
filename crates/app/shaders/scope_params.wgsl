// Scope scene uniform (scenes/scope.rs::ScopeParams, 80 + 512 bytes).
struct ScopeParams {
    resolution: vec2<f32>,
    gain: f32,
    rotation: f32,
    beam_width: f32,
    beam_intensity: f32,
    decay: f32,
    colour_mode: f32,
    tint: vec4<f32>,
    point_count: u32,
    _pad2a: u32,
    _pad2b: u32,
    _pad2c: u32,
    palette_mix: f32,
    palette_shift: f32,
    key_pos: f32,
    speed_spread: f32,
    palette: array<vec4<f32>, 32>,
};

fn scope_pal(sp: ScopeParams, t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(sp.palette[i].rgb, sp.palette[(i + 1u) % 32u].rgb, fract(x));
}
