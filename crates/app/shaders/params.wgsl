// Shared uniform block. Prepended to every shader by shaders.rs; keep it in
// sync with scope::Params (80 bytes).

struct Params {
    resolution: vec2<f32>,
    gain: f32,
    rotation: f32,
    beam_width: f32,
    beam_intensity: f32,
    decay: f32,
    exposure: f32,
    tint: vec4<f32>,
    bloom: f32,
    grain: f32,
    time: f32,
    dt: f32,
    point_count: u32,
    flags: u32,       // bit0: auto exposure
    _pad1: u32,
    _pad2: u32,
};

struct FsIn {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Fullscreen triangle.
fn fullscreen_vertex(vi: u32) -> FsIn {
    var out: FsIn;
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}
