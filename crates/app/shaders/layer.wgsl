// Layer compositor: one scene output, scaled by opacity, added onto the
// composite (the pipeline blends additively; the first layer clears).

struct LayerParams {
    opacity: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> L: LayerParams;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv).rgb * L.opacity;
    return vec4<f32>(c, 1.0);
}
