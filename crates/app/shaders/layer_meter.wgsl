// Layer meter, stage 2: a 1x1 state texture per layer, ping-ponged.
//   r = levelling gain, slewed over ~1.5 s, so each layer arrives at the mix
//       at a comparable brightness and opacity is the actual mix ratio.
//   g = the measured lit-area mean, for diagnostics.
//   a = 1 once something lit has been measured. Until then (a layer still
//       building, a visual drawing nothing) the gain is not touched: on an
//       empty picture it would run up to its limit, and the first frames
//       that came in were blown out for seconds while it came back down.
// It brings an overbright layer down quickly (0.4 s, as an eye does) and
// lifts a dim one slowly (1.5 s), so the music's swells stay swells.
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

// With flag bit 2 (a visual asking to be levelled by its bright part) a
// block counts as lit only above 1/32 of the brightest block, so faint fill
// spread over the frame (plankton, glow in the water) neither drags the
// mean down nor gets the bright things lifted until they bleach.
const BRIGHT_PART: f32 = 1.0 / 32.0;
// A shared space (flag bit 3) holds every member's faint fill in one
// picture, so its floor is higher: only what is really lit counts.
const SPACE_PART: f32 = 1.0 / 8.0;

fn block(x: i32, y: i32) -> f32 {
    let uv = (vec2<f32>(f32(x), f32(y)) + 0.5) / 32.0;
    let c = textureSampleLevel(small_tex, samp, uv, 0.0).rgb;
    return max(c.r, max(c.g, c.b));
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    var floor_m = 1e-6;
    if ((L.flags & 4u) != 0u) {
        var top = 0.0;
        for (var y = 0; y < 32; y++) {
            for (var x = 0; x < 32; x++) {
                top = max(top, block(x, y));
            }
        }
        floor_m = max(top * select(BRIGHT_PART, SPACE_PART, (L.flags & 8u) != 0u), 1e-6);
    }
    var sum = 0.0;
    var lit = 0.0;
    for (var y = 0; y < 32; y++) {
        for (var x = 0; x < 32; x++) {
            let m = block(x, y);
            let on = m > floor_m;
            sum += select(0.0, m, on);
            lit += select(0.0, 1.0, on);
        }
    }
    // Mean over lit blocks; a near-empty frame counts as a few blocks so the
    // gain does not run away on a single pixel.
    let mean = sum / max(lit, 4.0);
    let prev = textureSampleLevel(prev_state, samp, vec2<f32>(0.5), 0.0);
    if (sum <= 1e-6) {
        // Nothing lit: hold (and stay unmeasured until something is).
        return vec4<f32>(prev.r, 0.0, 0.0, prev.a);
    }
    let want = clamp(LIT_TARGET / max(mean, 1e-7), 0.05, 10000.0);
    let rate = 1.0 - exp(-L.dt / select(1.5, 0.4, want < prev.r));
    let gain = select(mix(prev.r, want, rate), want, prev.r <= 0.0 || prev.a < 0.5);
    return vec4<f32>(gain, mean, lit / 1024.0, 1.0);
}
