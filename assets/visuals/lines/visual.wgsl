// Spectrum lines: three traces across the frame. The top is the spectrum
// on a linear axis (0 to Highest; each point the smoothed row there,
// against the loudest, followed over seconds), the middle the waveform, the
// bottom the spectrum again upside down and running the other way. Drawn as
// the line: as lines, or as dots (each point a dash of its own).
//
// Item 0: (the loudest row, followed; 0, 0, 0).

const MAX_POINTS: u32 = 512u;
const PER_TRACE: u32 = 1536u;

fn prepare(i: u32) -> vec4<f32> {
    var m = 0.0;
    for (var k = 0u; k < 48u; k++) {
        m = max(m, spectrum_hz((f32(k) + 0.5) / 48.0 * params().highest));
    }
    return vec4<f32>(ease(item(0u).x, max(m, 0.2), 0.05, 3.0), 0.0, 0.0, 0.0);
}

fn spec_level(t: f32) -> f32 {
    let P = params();
    let range = max(P.range, 6.0) / 80.0;
    let top = max(item(0u).x, 0.2);
    return clamp((spectrum_hz(t * P.highest) - (top - range)) / range * P.gain, 0.0, 1.0);
}

// Along the palette by level. (The original's white going to magenta is
// the scene's custom palette.)
fn trace_colour(level: f32) -> vec3<f32> {
    return colour(0.1 + 0.85 * level);
}

fn point(i: u32) -> LinePoint {
    let P = params();
    var q: LinePoint;
    q.pos = vec2<f32>(0.0);
    q.width = 0.0;
    q.colour = vec3<f32>(0.0);
    let trace = i / PER_TRACE;
    let n = clamp(u32(P.points), 64u, MAX_POINTS);
    let dots = P.style > 0.5;
    // In dots, three line points a sample: here, a step on, a break.
    let k = select(i % PER_TRACE, (i % PER_TRACE) / 3u, dots);
    let sub = (i % PER_TRACE) % 3u;
    if (k >= n || (dots && sub == 2u)) {
        return q;
    }
    let ext = V.resolution / min(V.resolution.x, V.resolution.y);
    let t = (f32(k) + select(0.0, 0.35, dots && sub == 1u)) / f32(n - 1u);
    var x = mix(-0.92, 0.92, t) * ext.x;
    var y = 0.0;
    var level = 0.0;
    if (trace == 1u) {
        if (P.wave <= 0.0) {
            return q;
        }
        let w = wave_mono(t);
        y = w * P.wave;
        level = clamp(abs(w), 0.0, 1.0) * 0.6;
    } else {
        level = spec_level(t);
        y = P.spread + level * P.height;
        if (trace == 2u) {
            // Upside down, running the other way.
            y = -y;
            x = -x;
        }
    }
    q.pos = vec2<f32>(x, y);
    q.width = P.width;
    q.colour = trace_colour(level) * (0.45 + 0.8 * level);
    return q;
}
