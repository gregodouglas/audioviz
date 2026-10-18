// Layer compositor: one layer's picture (scaled by opacity and, with flag
// bit 0, by its levelling gain) meets the composite so far (`dst`) by its
// blend mode. Layers are drawn bottom to top; flag bit 1 marks the first,
// which meets black. Add is order-free; over covers by brightness, screen
// lightens, multiply masks.

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> L: LayerParams;
@group(0) @binding(3) var state: texture_2d<f32>;
@group(0) @binding(4) var dst: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    var lvl = 1.0;
    if ((L.flags & 1u) != 0u) {
        lvl = textureSampleLevel(state, samp, vec2<f32>(0.5), 0.0).r;
    }
    let raw = textureSample(tex, samp, in.uv).rgb;
    let c = raw * lvl * L.opacity;
    var d = vec3<f32>(0.0);
    if ((L.flags & 2u) == 0u) {
        d = textureSample(dst, samp, in.uv).rgb;
    }
    var out: vec3<f32>;
    switch (L.blend) {
        case 1u: {
            let a = clamp(max(c.r, max(c.g, c.b)), 0.0, 1.0);
            out = c + d * (1.0 - a);
        }
        case 2u: {
            out = d + c * clamp(vec3<f32>(1.0) - d, vec3<f32>(0.0), vec3<f32>(1.0));
        }
        case 3u: {
            let m = clamp(raw * lvl, vec3<f32>(0.0), vec3<f32>(1.0));
            out = d * mix(vec3<f32>(1.0), m, clamp(L.opacity, 0.0, 1.0));
        }
        default: {
            out = d + c;
        }
    }
    return vec4<f32>(out, 1.0);
}
