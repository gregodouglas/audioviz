// Auto-exposure + flash-limiter state: a 1x1 texture, ping-ponged per frame.
//   r = exposure gain e: aims the mean luminance of the composite (read from
//       the smallest bloom level) at a dark-field level, smoothed over ~2 s.
//   g = displayed mean luminance L this frame: mean * exposure * e, after the
//       flash limiter. Scene-referred (pre-tonemap, pre-bloom) units, i.e. the
//       level that enters AgX; 0.18 is mid grey, ~0.08 is 10% of peak output.
//   b = flash-limiter gain in (0, 1], 1.0 unless the limiter is biting.
//       present.wgsl multiplies the picture by it.
// Bindings: 0 = smallest bloom level, 3 = previous state.
//
// Reduce flashing (P.flags bit1): L may rise by at most a factor of
// (1 + P.flash_rise) per second relative to the previous frame's displayed L.
// Decreases are never limited (only luminance increases trigger photosensitive
// responses), and rises that stay under FLASH_FLOOR are free: through AgX that
// is ~10% of peak display luminance, the WCAG "general flash" threshold under
// which a dark-to-dark change does not count as a flash. The limiter sits on
// top of auto exposure: e adapts slowly to the raw mean and is never fed the
// limited value, so the two do not fight; the limiter only clips the sudden
// rise that exposure has not yet caught up with.
const FLASH_FLOOR: f32 = 0.08;

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

    let prev = textureSampleLevel(prev_expo, samp, vec2<f32>(0.5), 0.0);
    var e = 1.0;
    if ((P.flags & 1u) != 0u) {
        // Aim the pre-tonemap mean at a dark-field level; lines still clip white.
        let want = clamp(0.03 / max(mean, 1e-5), 0.25, 6.0);
        let rate = 1.0 - exp(-P.dt / 2.0);
        e = select(mix(prev.r, want, rate), want, prev.r <= 0.0);
    }

    // Intended displayed mean this frame: the raw mean through the exposure
    // chain, smoothed only as much as e is.
    let l_cur = mean * P.exposure * e;
    var gain = 1.0;
    // No limit for the first second: prev.g starts at 0 and the scene is settling.
    if ((P.flags & 2u) != 0u && P.time > 1.0) {
        let allowed = max(prev.g, FLASH_FLOOR) * pow(1.0 + max(P.flash_rise, 0.0), P.dt);
        if (l_cur > allowed) {
            gain = allowed / l_cur;
        }
    }
    return vec4<f32>(e, l_cur * gain, gain, 1.0);
}
