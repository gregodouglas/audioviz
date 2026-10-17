// Tonemap the HDR accumulation buffer to the display and apply the phosphor
// tint. Bright spots bleach toward white like a real phosphor.

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
    _pad: vec3<u32>,
};

@group(0) @binding(0) var hdr_tex: texture_2d<f32>;
@group(0) @binding(1) var hdr_samp: sampler;
@group(0) @binding(2) var<uniform> P: Params;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let v = textureSample(hdr_tex, hdr_samp, in.uv).r * P.exposure;
    let body = 1.0 - exp(-v);          // soft shoulder
    let bleach = 1.0 - exp(-v * 0.12); // slow approach to white
    let col = mix(P.tint.rgb * body, vec3<f32>(1.0), bleach);
    // Faint vignette so the edges read as a tube, not a flat rectangle.
    let d = in.uv - 0.5;
    let vig = 1.0 - 0.35 * dot(d, d) * 2.0;
    return vec4<f32>(col * vig, 1.0);
}
