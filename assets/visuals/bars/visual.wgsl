// Spectrum bars: the classic analyser. Each bar is a slice of the fine
// spectrum (spectrum_peak over its frequency range, on a log or linear
// axis), lifted by a tilt so the highs read level, falling at a steady
// rate, with a peak cap that holds and drops. Solid, LED segments or
// outlines; standing on the floor with a reflection, mirrored about the
// middle line, or with the lows in the middle.
//
// Items: bar b is item b, kept from frame to frame: (height 0..1, peak cap
// height, seconds the cap still holds, 0). Item LOUDEST: the loudest
// tilted slice, followed (quickly up, over seconds down), which the bars
// are measured against, so any track fills the row.

const MAX_BARS: u32 = 256u;
const LOUDEST: u32 = 256u;

// A slice's level with the highs lifted (0.056 is 4.5 dB on the
// spectrum's 80 dB scale).
fn tilted(r: vec2<f32>) -> f32 {
    let tilt = params().tilt * 0.056 * log2(max(sqrt(r.x * r.y), 20.0) / 1000.0);
    return spectrum_peak(r.x, r.y) + tilt;
}

fn bar_count() -> u32 {
    return clamp(u32(params().count), 8u, MAX_BARS);
}

// The frequencies bar b of n covers.
fn bar_range(b: u32, n: u32) -> vec2<f32> {
    let P = params();
    let lo = max(P.lowest, 10.0);
    let hi = max(P.highest, lo * 1.5);
    let t0 = f32(b) / f32(n);
    let t1 = f32(b + 1u) / f32(n);
    if (P.axis > 0.5) {
        return vec2<f32>(mix(lo, hi, t0), mix(lo, hi, t1));
    }
    return vec2<f32>(lo * pow(hi / lo, t0), lo * pow(hi / lo, t1));
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let n = bar_count();
    let dt = clamp(V.dt, 0.0, 0.1);
    if (i == LOUDEST) {
        // The loudest of 32 slices across the row.
        var m = 0.0;
        for (var k = 0u; k < 32u; k++) {
            m = max(m, tilted(bar_range(k, 32u)));
        }
        let last = item(LOUDEST).x;
        return vec4<f32>(ease(last, max(m, 0.2), 0.05, 3.0), 0.0, 0.0, 0.0);
    }
    if (i >= n) {
        return vec4<f32>(0.0);
    }
    let r = bar_range(i, n);
    // The slice against the loudest: the top of the range is full height,
    // the range below it the floor; then the gain.
    let range = max(P.range, 6.0) / 80.0;
    let top = max(item(LOUDEST).x, 0.2);
    let want = clamp((tilted(r) - (top - range)) / range * P.gain, 0.0, 1.0);
    let last = item(i);
    // Up at once (the spectrum is already smoothed), down at a steady rate.
    let level = max(want, last.x - P.fall * dt);
    var peak = last.y;
    var hold = last.z - dt;
    if (level >= peak) {
        peak = level;
        hold = P.hold;
    } else if (hold <= 0.0) {
        peak = max(peak - P.fall * 0.5 * dt, level);
    }
    return vec4<f32>(level, peak, hold, 0.0);
}

// The frame the bars stand in, in centred units.
struct Frame {
    half_w: f32,   // half the row's width
    slot: f32,     // one bar's slot width
    base: f32,     // the floor (Bottom, Middle out) or the middle line (Mirrored)
    tall: f32,     // a full bar's height
    mirrored: bool,
    middle: bool,
};

fn frame() -> Frame {
    let P = params();
    let n = f32(bar_count());
    let ext = V.resolution / min(V.resolution.x, V.resolution.y);
    var f: Frame;
    f.half_w = P.width * ext.x;
    f.middle = P.layout_ > 1.5;
    f.mirrored = P.layout_ > 0.5 && P.layout_ < 1.5;
    f.slot = select(2.0 * f.half_w / n, f.half_w / n, f.middle);
    if (f.mirrored) {
        f.base = 0.0;
        f.tall = P.height * ext.y * 0.95;
    } else {
        f.base = -0.72 * ext.y;
        f.tall = P.height * ext.y * 1.6;
    }
    return f;
}

// Bar k's centre across, on the side of x (Middle out has two of each).
fn bar_x(f: Frame, k: u32, x: f32) -> f32 {
    if (f.middle) {
        return sign(x + 1e-6) * (f32(k) + 0.5) * f.slot;
    }
    return -f.half_w + (f32(k) + 0.5) * f.slot;
}

// Signed distance from p to a box from y0 to y1 around cx, half-width hw,
// corners rounded by rr.
fn box(p: vec2<f32>, cx: f32, hw: f32, y0: f32, y1: f32, rr: f32) -> f32 {
    let c = vec2<f32>(cx, 0.5 * (y0 + y1));
    let h = vec2<f32>(hw, 0.5 * abs(y1 - y0));
    let r = min(rr, min(h.x, h.y));
    let q = abs(p - c) - h + r;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Where along the row bar k sits, 0..1 (for colouring by position).
fn row_t(k: u32) -> f32 {
    return (f32(k) + 0.5) / f32(bar_count());
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let n = bar_count();
    let f = frame();
    let px = pixel();
    let p0 = centred(uv);
    // Below the floor (Bottom), the reflection: the bars mirrored, fading.
    let reflect = !f.mirrored && !f.middle && p0.y < f.base;
    let mirror_y = select(p0.y, 2.0 * f.base - p0.y, reflect);
    var p = vec2<f32>(p0.x, mirror_y);
    if (f.mirrored) {
        p.y = abs(p.y);
    }
    // Which bar this column is in.
    var t = (p.x + f.half_w) / (2.0 * f.half_w);
    if (f.middle) {
        t = abs(p.x) / f.half_w;
    }
    let b = i32(floor(t * f32(n)));
    let hw = 0.5 * f.slot * (1.0 - P.gap);
    let rr = P.round * hw;
    let base = select(f.base, 0.0, f.mirrored);
    var light = vec3<f32>(0.0);
    var glow = vec3<f32>(0.0);
    for (var o = -1; o <= 1; o++) {
        let k = b + o;
        if (k < 0 || k >= i32(n)) {
            continue;
        }
        let ku = u32(k);
        let it = item(ku);
        let cx = bar_x(f, ku, p.x);
        let top = base + max(it.x * f.tall, 0.0);
        let d = box(p, cx, hw, base, max(top, base + px), rr);
        // The bar's colour at this height or place.
        let hgt = clamp((p.y - base) / f.tall, 0.0, 1.0);
        var c = colour(0.35);
        if (P.colouring < 0.5) {
            c = colour(0.1 + 0.8 * hgt);
        } else if (P.colouring < 1.5) {
            c = colour(row_t(ku));
        }
        var cover = clamp(0.5 - d / px, 0.0, 1.0);
        if (P.style > 1.5) {
            // Outline: a line two pixels wide along the edge.
            cover = clamp(1.0 - abs(d) / px + 0.0, 0.0, 1.0) * select(1.0, 0.0, it.x <= 0.0);
        } else if (P.style > 0.5) {
            // LED segments: whole cells lit up to the level, the rest faint.
            let segs = max(P.segments, 2.0);
            let cell = f.tall / segs;
            let ci = floor((p.y - base) / cell);
            let in_cell = fract((p.y - base) / cell);
            let lit = ci < floor(it.x * segs + 0.5);
            let cell_box = box(p, cx, hw, base + ci * cell, base + (ci + 0.72) * cell, rr * 0.5);
            let cell_cover = clamp(0.5 - cell_box / px, 0.0, 1.0) * select(0.0, 1.0, ci >= 0.0 && ci < segs);
            cover = cell_cover * select(0.025, 1.0, lit);
            _ = in_cell;
        }
        light = max(light, c * cover * (0.85 + 0.4 * it.x));
        // The peak cap: a thin bar at the held peak.
        if (P.peaks > 0.5 && it.y > 0.01) {
            let py = base + it.y * f.tall;
            let cap = box(p, cx, hw, py + 1.5 * px, py + 4.5 * px, px);
            let cc = select(colour(0.1 + 0.8 * it.y), c, P.colouring > 0.5);
            light = max(light, cc * 1.4 * clamp(0.5 - cap / px, 0.0, 1.0));
        }
        // A soft glow round the bar, ending at zero.
        let g = max(exp(-max(d, 0.0) / (8.0 * px)) - 0.02, 0.0) * select(1.0, 0.0, it.x <= 0.0);
        glow += c * g * 0.12 * P.glow * (0.5 + it.x);
    }
    var out = light + glow;
    if (reflect) {
        let below = (f.base - p0.y) / max(0.28 * (V.resolution.y / min(V.resolution.x, V.resolution.y)), 1e-3);
        out *= P.reflection * 0.35 * max(1.0 - below, 0.0);
    }
    return out;
}
