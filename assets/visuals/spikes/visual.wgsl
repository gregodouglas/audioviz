// Spike sphere: a turning sphere of spikes. Spike k sits at a point of a
// Fibonacci sphere (evenly spread) and reaches out along it by the level of
// one slice of the spectrum (a log axis 40 Hz to 14 kHz in 96 slices,
// measured against the loudest, followed over seconds): Scattered picks
// each spike's slice by a hash, By latitude by its height (the lows at the
// poles). Drawn as the line: root, tip, break.
//
// Item 0: (the turn, radians; the loudest slice, followed; 0, 0).

const MAX_SPIKES: u32 = 2048u;
const SLICES: f32 = 96.0;

fn spike_count() -> u32 {
    return clamp(u32(params().count), 128u, MAX_SPIKES);
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let s = item(0u);
    let dt = clamp(V.dt, 0.0, 0.1);
    var m = 0.0;
    for (var k = 0u; k < 24u; k++) {
        m = max(m, spectrum_log((f32(k) + 0.5) / 24.0));
    }
    let turn = s.x + dt * P.spin * pace();
    return vec4<f32>(turn - TAU * floor(turn / TAU), ease(s.y, max(m, 0.2), 0.05, 3.0), 0.0, 0.0);
}

// Spike k's direction on a Fibonacci sphere.
fn spike_dir(k: u32, n: u32) -> vec3<f32> {
    let y = 1.0 - 2.0 * (f32(k) + 0.5) / f32(n);
    let r = sqrt(max(1.0 - y * y, 0.0));
    let a = f32(k) * 2.39996323;
    return vec3<f32>(r * cos(a), y, r * sin(a));
}

// The level of spike k's slice, 0..1.
fn spike_level(k: u32, dir: vec3<f32>) -> f32 {
    let P = params();
    var t: f32;
    if (P.mapping > 0.5) {
        t = abs(dir.y);
        t = 1.0 - t;
    } else {
        t = floor(hash21(vec2<f32>(f32(k), 7.31)) * SLICES) / SLICES;
    }
    let lo = 40.0 * pow(14000.0 / 40.0, t);
    let hi = 40.0 * pow(14000.0 / 40.0, min(t + 1.0 / SLICES, 1.0));
    let range = max(P.range, 6.0) / 80.0;
    let top = max(item(0u).y, 0.2);
    let tilt = 0.03 * log2(max(sqrt(lo * hi), 20.0) / 1000.0);
    return clamp((spectrum_peak(lo, hi) + tilt - (top - range)) / range * P.gain, 0.0, 1.0);
}

// Turned and tipped, then seen in perspective.
fn view(p: vec3<f32>) -> vec3<f32> {
    let P = params();
    let a = item(0u).x;
    let q = vec3<f32>(p.x * cos(a) - p.z * sin(a), p.y, p.x * sin(a) + p.z * cos(a));
    let b = P.tilt * 0.8;
    let w = vec3<f32>(q.x, q.y * cos(b) - q.z * sin(b), q.y * sin(b) + q.z * cos(b));
    let z = 3.0 - w.z;
    return vec3<f32>(w.xy * 3.0 / z, z);
}

// Along the palette by reach. (The original's yellow, red and pink are the
// scene's custom palette.)
fn spike_colour(level: f32) -> vec3<f32> {
    return colour(0.1 + 0.85 * level);
}

fn point(i: u32) -> LinePoint {
    let P = params();
    var q: LinePoint;
    q.pos = vec2<f32>(0.0);
    q.width = 0.0;
    q.colour = vec3<f32>(0.0);
    let n = spike_count();
    let k = i / 3u;
    let j = i % 3u;
    if (k >= n || j == 2u) {
        return q;
    }
    let dir = spike_dir(k, n);
    let level = spike_level(k, dir);
    if (level < P.hide) {
        return q;
    }
    let reach = select(0.0, 1.0, j == 1u) * level * P.reach;
    let v = view(dir * P.radius * (1.0 + reach));
    q.pos = v.xy;
    // Nearer spikes a little brighter and wider.
    let near = clamp(1.6 - 0.25 * v.z, 0.6, 1.2);
    q.width = P.width * near;
    q.colour = spike_colour(level) * (0.5 + 0.9 * level) * near * select(0.6, 1.0, j == 1u);
    return q;
}
