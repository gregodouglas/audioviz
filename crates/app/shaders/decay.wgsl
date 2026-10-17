// Phosphor persistence: out = previous * decay.

struct Params {
    resolution: vec2<f32>,
    gain: f32,
    rotation: f32,
    beam_width: f32,
    beam_intensity: f32,
    decay: f32,
    exposure: f32,
    tint: vec4<f32>,
    point_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var prev_tex: texture_2d<f32>;
@group(0) @binding(1) var prev_samp: sampler;
@group(0) @binding(2) var<uniform> P: Params;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // Fullscreen triangle.
    var out: VsOut;
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(prev_tex, prev_samp, in.uv);
    // Subtract a tiny floor so the trail actually reaches black instead of
    // lingering as a faint glow forever.
    let v = max(c.rgb * P.decay - vec3<f32>(0.0005), vec3<f32>(0.0));
    return vec4<f32>(v, 1.0);
}
