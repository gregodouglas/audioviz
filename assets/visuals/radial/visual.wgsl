// Radial bars: spectrum bars round a ring. Mirrored, n bars run down each
// side from the top (the lows at the top, both halves alike); full, they go
// once round. The ring swells with the bass; bars stand out, in or both
// ways, drawn as bars, a closed line through their tips, or dots. The
// middle holds a dark disc, the album art, or nothing.
//
// Items: bar b is item b (height 0..1, kept); item STATE: (the loudest
// tilted slice, followed; the bass swell, eased; the turn, radians; 0).

const MAX_BARS: u32 = 256u;
const STATE: u32 = 256u;

fn bar_count() -> u32 {
    return clamp(u32(params().count), 16u, MAX_BARS);
}

fn mirrored() -> bool {
    return params().arrangement < 0.5;
}

fn bar_range(b: u32, n: u32) -> vec2<f32> {
    let P = params();
    let lo = max(P.lowest, 10.0);
    let hi = max(P.highest, lo * 1.5);
    return vec2<f32>(lo * pow(hi / lo, f32(b) / f32(n)), lo * pow(hi / lo, f32(b + 1u) / f32(n)));
}

// A slice's level with the highs lifted (4.5 dB an octave at full tilt).
fn tilted(r: vec2<f32>) -> f32 {
    let tilt = params().tilt * 0.056 * log2(max(sqrt(r.x * r.y), 20.0) / 1000.0);
    return spectrum_peak(r.x, r.y) + tilt;
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let n = bar_count();
    let dt = clamp(V.dt, 0.0, 0.1);
    if (i == STATE) {
        let s = item(STATE);
        var m = 0.0;
        for (var k = 0u; k < 32u; k++) {
            m = max(m, tilted(bar_range(k, 32u)));
        }
        let swell = ease(s.y, clamp(broad_rel(BASS) - 0.8, 0.0, 1.5) + 0.6 * broad_attack(BASS), 0.04, 0.3);
        let turn = s.z + dt * P.spin * (0.6 + 0.4 * V.energy);
        return vec4<f32>(ease(s.x, max(m, 0.2), 0.05, 3.0), swell, turn - TAU * floor(turn / TAU), 0.0);
    }
    if (i >= n) {
        return vec4<f32>(0.0);
    }
    let range = max(P.range, 6.0) / 80.0;
    let top = max(item(STATE).x, 0.2);
    let want = clamp((tilted(bar_range(i, n)) - (top - range)) / range * P.gain, 0.0, 1.0);
    return vec4<f32>(max(want, item(i).x - P.fall * dt), 0.0, 0.0, 0.0);
}

// The ring's radius this frame.
fn ring_radius() -> f32 {
    let P = params();
    return P.radius * (1.0 + 0.12 * P.pulse * item(STATE).y);
}

// Bar k's direction (clockwise from the top) on the side `side` (+1 or -1).
fn bar_angle(k: i32, n: u32, side: f32) -> f32 {
    let turn = item(STATE).z;
    if (mirrored()) {
        return side * (f32(k) + 0.5) / f32(n) * 3.14159265 + turn;
    }
    return (f32(k) + 0.5) / f32(n) * TAU + turn;
}

// Where bar k's level runs from and to along its direction.
fn bar_span(level: f32) -> vec2<f32> {
    let P = params();
    let R = ring_radius();
    let L = max(level, 0.004) * P.length;
    if (P.direction > 1.5) {
        return vec2<f32>(R - 0.5 * L, R + L);
    }
    if (P.direction > 0.5) {
        return vec2<f32>(max(R - L, 0.02), R);
    }
    return vec2<f32>(R, R + L);
}

// The far end of bar k (its tip, where the line runs).
fn bar_tip(k: i32, n: u32, side: f32) -> vec2<f32> {
    let kk = u32((k + i32(n)) % i32(n));
    let a = bar_angle(k, n, side);
    let span = bar_span(item(kk).x);
    let r = select(span.y, span.x, params().direction > 0.5 && params().direction < 1.5);
    return vec2<f32>(sin(a), cos(a)) * r;
}

fn seg_dist(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let h = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-9), 0.0, 1.0);
    return length(p - a - ab * h);
}

fn bar_colour(k: u32, n: u32, level: f32) -> vec3<f32> {
    let P = params();
    if (P.colouring < 0.5) {
        return colour((f32(k) + 0.5) / f32(n));
    }
    if (P.colouring < 1.5) {
        return colour(0.12 + 0.8 * level);
    }
    return colour(0.4);
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let n = bar_count();
    let px = pixel();
    let p = centred(uv);
    let r = length(p);
    let R = ring_radius();
    // Which bar this direction is in, and on which side.
    let a = atan2(p.x, p.y) - item(STATE).z;
    var side = 1.0;
    var s: f32;
    if (mirrored()) {
        let w = a - TAU * floor((a + 3.14159265) / TAU);
        side = select(-1.0, 1.0, w >= 0.0);
        s = abs(w) / 3.14159265;
    } else {
        s = fract(a / TAU);
    }
    let b = i32(floor(s * f32(n)));
    let slot = select(TAU, 3.14159265, mirrored()) / f32(n);
    let hw = 0.5 * R * slot * (1.0 - P.gap);
    var light = vec3<f32>(0.0);
    var glow = vec3<f32>(0.0);
    for (var o = -1; o <= 1; o++) {
        var k = b + o;
        if (mirrored()) {
            if (k < 0 || k >= i32(n)) {
                continue;
            }
        } else {
            k = (k + i32(n)) % i32(n);
        }
        let ku = u32(k);
        let level = item(ku).x;
        let ang = bar_angle(k, n, side);
        let dir = vec2<f32>(sin(ang), cos(ang));
        let across = vec2<f32>(dir.y, -dir.x);
        let c = bar_colour(ku, n, level) * (0.8 + 0.6 * level);
        var d: f32;
        if (P.style > 1.5) {
            // Dots at the tips.
            d = length(p - bar_tip(k, n, side)) - hw * 1.3;
        } else if (P.style > 0.5) {
            // A closed line through the tips.
            d = seg_dist(p, bar_tip(k, n, side), bar_tip(k + 1, n, side)) - 1.2 * px;
            if (mirrored() && k == i32(n) - 1) {
                // Across the bottom, to the other side's last tip.
                d = min(d, seg_dist(p, bar_tip(k, n, side), bar_tip(k, n, -side)) - 1.2 * px);
            }
            if (mirrored() && k == 0) {
                d = min(d, seg_dist(p, bar_tip(0, n, side), bar_tip(0, n, -side)) - 1.2 * px);
            }
        } else {
            // A bar: a box along its direction.
            let span = bar_span(level);
            let q = vec2<f32>(dot(p, across), dot(p, dir) - 0.5 * (span.x + span.y));
            let e = abs(q) - vec2<f32>(hw, 0.5 * (span.y - span.x));
            d = length(max(e, vec2<f32>(0.0))) + min(max(e.x, e.y), 0.0);
        }
        light = max(light, c * clamp(0.5 - d / px, 0.0, 1.0));
        glow += c * max(exp(-max(d, 0.0) / (10.0 * px)) - 0.02, 0.0) * 0.1 * P.glow;
    }
    // The middle.
    var mid = vec3<f32>(0.0);
    let inner = R * select(0.96, 0.9, P.direction > 0.5);
    if (P.centre < 1.5 && r < inner) {
        light = vec3<f32>(0.0);
        glow *= 0.2;
        if (P.centre > 0.5 && V.has_art > 0.5) {
            mid = art(vec2<f32>(p.x, -p.y) / inner * 0.5 + 0.5) * 0.9 * steady();
        }
    }
    // A thin rim round the disc or the art.
    if (P.centre < 1.5) {
        let rim = abs(r - inner) - 0.8 * px;
        mid += colour(0.0) * 0.5 * clamp(0.5 - rim / px, 0.0, 1.0);
    }
    return light + glow + mid;
}
