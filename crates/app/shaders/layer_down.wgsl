// Layer meter, stage 1: box-downsample a layer's output by 8 with 16 bilinear
// taps per texel (full coverage), so thin lines are counted, not sampled.

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> L: LayerParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let src = vec2<f32>(textureDimensions(tex));
    let px = 1.0 / src;
    // This texel covers an 8x8 block; 4x4 bilinear taps at 2 px spacing cover it.
    let base = in.uv - px * 3.0;
    var acc = vec3<f32>(0.0);
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let uv = base + px * vec2<f32>(f32(x) * 2.0, f32(y) * 2.0);
            acc += textureSampleLevel(tex, samp, uv, 0.0).rgb;
        }
    }
    return vec4<f32>(acc / 16.0, 1.0);
}
