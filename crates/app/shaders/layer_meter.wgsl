// Layer meter, stage 2: a 1x1 state texture per layer, ping-ponged.
//   r = levelling gain, slewed over ~1.5 s, so each layer arrives at the mix
//       at a comparable brightness and opacity is the actual mix ratio.
//   g = the measured lit-area mean, for diagnostics.
// The target is the mean over the *lit* part of the picture, not the whole
// frame: a sparse layer (a few dots, a thin trace) has a tiny frame mean and
// a mean-based gain would either blow it out or never reach it.
// Bindings: 0 = the 1/8 downsample, 3 = previous state.

@group(0) @binding(0) var small_tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> L: LayerParams;
@group(0) @binding(3) var prev_state: texture_2d<f32>;

const LIT_TARGET: f32 = 0.06;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    var sum = 0.0;
    var lit = 0.0;
    for (var y = 0; y < 32; y++) {
        for (var x = 0; x < 32; x++) {
            let uv = (vec2<f32>(f32(x), f32(y)) + 0.5) / 32.0;
            let c = textureSampleLevel(small_tex, samp, uv, 0.0).rgb;
            let m = max(c.r, max(c.g, c.b));
            sum += m;
            lit += select(0.0, 1.0, m > 1e-6);
        }
    }
    // Mean over lit blocks; a near-empty frame counts as a few blocks so the
    // gain does not run away on a single pixel.
    let mean = sum / max(lit, 4.0);
    let prev = textureSampleLevel(prev_state, samp, vec2<f32>(0.5), 0.0);
    let want = clamp(LIT_TARGET / max(mean, 1e-7), 0.05, 10000.0);
    let rate = 1.0 - exp(-L.dt / 1.5);
    let gain = select(mix(prev.r, want, rate), want, prev.r <= 0.0);
    return vec4<f32>(gain, mean, lit / 1024.0, 1.0);
}
