// Layer compositor: one scene output, scaled by opacity (and by the layer's
// own levelling gain when flag bit 0 is set), added onto the composite (the
// pipeline blends additively; the first layer clears).

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> L: LayerParams;
@group(0) @binding(3) var state: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    var gain = L.opacity;
    if ((L.flags & 1u) != 0u) {
        gain *= textureSampleLevel(state, samp, vec2<f32>(0.5), 0.0).r;
    }
    let c = textureSample(tex, samp, in.uv).rgb * gain;
    return vec4<f32>(c, 1.0);
}
