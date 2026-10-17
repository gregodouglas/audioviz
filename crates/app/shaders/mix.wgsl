// Cross-fade two scene outputs.

struct MixParams {
    t: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var tex_a: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> M: MixParams;
@group(0) @binding(3) var tex_b: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let a = textureSample(tex_a, samp, in.uv).rgb;
    let b = textureSample(tex_b, samp, in.uv).rgb;
    return vec4<f32>(mix(a, b, M.t), 1.0);
}
