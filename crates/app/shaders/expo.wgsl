// Auto-exposure: a 1x1 state texture tracking the mean luminance of the
// composite (read from the smallest bloom level), smoothed over ~2 s.
// Bindings: 0 = smallest bloom level, 3 = previous exposure state.

@group(0) @binding(0) var mean_tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> P: PostParams;
@group(0) @binding(3) var prev_expo: texture_2d<f32>;
@group(0) @binding(4) var unused_tex: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    var mean = 0.0;
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let uv = (vec2<f32>(f32(x), f32(y)) + 0.5) / 4.0;
            let c = textureSampleLevel(mean_tex, samp, uv, 0.0).rgb;
            // Max channel, not luminance: the tint color must not change exposure.
            mean += max(c.r, max(c.g, c.b));
        }
    }
    mean /= 16.0;

    let prev = textureSampleLevel(prev_expo, samp, vec2<f32>(0.5), 0.0).r;
    var e = 1.0;
    if ((P.flags & 1u) != 0u) {
        // Aim the pre-tonemap mean at a dark-field level; lines still clip white.
        let want = clamp(0.03 / max(mean, 1e-5), 0.25, 6.0);
        let rate = 1.0 - exp(-P.dt / 2.0);
        e = select(mix(prev, want, rate), want, prev <= 0.0);
    }
    return vec4<f32>(e, 0.0, 0.0, 1.0);
}
