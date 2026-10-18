// Modifier: mirror. One side of the picture reflected across the middle,
// replacing the far side or adding to it. A plain axis mirror (the
// kaleidoscope fold is the angular one).
//   p0 = (side 0 left | 1 right | 2 top | 3 bottom | 4 quad, additive, flipped, 0)

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    var side = u32(M.p0.x + 0.5);
    // A beat flip swaps left/right or top/bottom.
    if (M.p0.z > 0.5) {
        if (side == 0u) { side = 1u; } else if (side == 1u) { side = 0u; } else if (side == 2u) { side = 3u; } else if (side == 3u) { side = 2u; }
    }
    var uv = in.uv;
    switch (side) {
        case 0u: { uv.x = select(uv.x, 1.0 - uv.x, uv.x > 0.5); }
        case 1u: { uv.x = select(uv.x, 1.0 - uv.x, uv.x < 0.5); }
        case 2u: { uv.y = select(uv.y, 1.0 - uv.y, uv.y > 0.5); }
        case 3u: { uv.y = select(uv.y, 1.0 - uv.y, uv.y < 0.5); }
        default: {
            uv.x = select(uv.x, 1.0 - uv.x, uv.x > 0.5);
            uv.y = select(uv.y, 1.0 - uv.y, uv.y > 0.5);
        }
    }
    let here = textureSample(src, samp, in.uv).rgb;
    let there = textureSample(src, samp, uv).rgb;
    if (M.p0.y > 0.5) {
        return vec4<f32>(here + there * select(1.0, 0.0, all(uv == in.uv)), 1.0);
    }
    return vec4<f32>(there, 1.0);
}
