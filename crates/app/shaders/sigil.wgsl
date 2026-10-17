// Sigil: a neon sacred-geometry glyph on a dark blue painterly ground.
// Every line is a signed-distance tube with a hot core and a coloured halo
// (bloom in post does the rest). Centre rings breathe with their band, rays
// strike on onsets at the angle of the triggering pitch class and fade
// outward, twelve circle-and-dot orbs on a counter-rotating orbit are lit by
// chroma. Elements light up and go out in order as `build.x` moves (track
// change rebuild, seeded by the track). Rays and flares write a persistence
// texture (binding 0 is last frame's) so they leave afterimages.
//
// Two targets: 0 = the picture, 1 = the new persistence state.
// Second pass (planned): nested cardinal triangles / diamonds and paint
// drips; `polygon` and the trail are the hooks for them.

@group(0) @binding(0) var trail_tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> P: GlyphParams;

const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318531;
const N_RAYS: u32 = 24u;
const N_RINGS: u32 = 6u;

struct Out {
    @location(0) color: vec4<f32>,
    @location(1) trail: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn band6(i: u32) -> f32 {
    let v = P.bands[(i / 4u) & 3u];
    switch (i % 4u) {
        case 0u: { return v.x; }
        case 1u: { return v.y; }
        case 2u: { return v.z; }
        default: { return v.w; }
    }
}

fn chroma(i: u32) -> f32 {
    let v = P.chroma[(i / 4u) % 3u];
    switch (i % 4u) {
        case 0u: { return v.x; }
        case 1u: { return v.y; }
        case 2u: { return v.z; }
        default: { return v.w; }
    }
}

// ---------------------------------------------------------------- colour

fn pal(x: f32) -> vec3<f32> {
    let t = fract(x) * 32.0;
    let i = u32(floor(t)) % 32u;
    return mix(P.palette[i].rgb, P.palette[(i + 1u) % 32u].rgb, fract(t));
}

// Tint ramp when no palette is set: the tint, leaning warm or cool by position.
fn own(t: f32) -> vec3<f32> {
    let base = P.tint.rgb;
    let warm = vec3<f32>(1.0, 0.55, 0.35);
    let cool = vec3<f32>(0.45, 0.6, 1.0);
    let lean = mix(cool, warm, 0.5 + 0.5 * sin(t * TAU));
    return normalize(mix(base, lean * length(base), 0.35)) * length(base);
}

fn col_at(pos: f32) -> vec3<f32> {
    let c = mix(own(pos), pal(pos + P.style.w), P.tint.w);
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return max(mix(vec3<f32>(l), c, 1.25), vec3<f32>(0.0));
}

fn drip_s(i: u32) -> f32 {
    let v = P.drip_s[(i / 4u) & 3u];
    switch (i % 4u) {
        case 0u: { return v.x; }
        case 1u: { return v.y; }
        case 2u: { return v.z; }
        default: { return v.w; }
    }
}

// Point at `t` segments along the trace.
fn trace_at(t: f32, tn: u32) -> vec2<f32> {
    let si = u32(clamp(floor(t), 0.0, f32(tn - 2u)));
    return mix(trace_point(si), trace_point(si + 1u), clamp(t - f32(si), 0.0, 1.0));
}

// A neon tube at distance `d` with half-width `w`: white-hot core plus a
// coloured halo with a long tail.
fn tube(d: f32, w: f32, c: vec3<f32>) -> vec3<f32> {
    let core = exp(-(d * d) / (w * w));
    let halo = exp(-d / (w * 4.0)) * 0.45 + 0.07 * w / (d + w * 0.5);
    let hot = P.pen.y;
    return mix(c, vec3<f32>(1.0), hot) * core * (1.3 + 1.2 * hot) + c * halo * P.style.x;
}

// Visibility of an element with light-up order `order` (0 = first) and a
// per-element id for the strike flicker near the threshold.
fn lit(order: f32, id: f32) -> f32 {
    let on = smoothstep(order, order + 0.1, P.build.x);
    let near = 1.0 - smoothstep(0.0, 0.22, abs(P.build.x - order - 0.05));
    let strike = step(0.4, hash21(vec2<f32>(floor(P.time * 26.0), id)));
    return on * mix(1.0, strike, near);
}

// ---------------------------------------------------------------- shapes

// Distance to a regular polygon of `n` sides and circumradius `r`, rotated
// `rot` (used by the second pass: triangles and diamonds).
fn polygon(p: vec2<f32>, n: f32, r: f32, rot: f32) -> f32 {
    let a = atan2(p.y, p.x) + rot;
    let seg = TAU / n;
    let b = (floor(0.5 + a / seg)) * seg - a;
    return length(p) * cos(b) - r * cos(seg * 0.5);
}

// Distance to the segment from `a` to `b`, and the position along it.
fn segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return vec2<f32>(length(pa - ba * h), h);
}

// Outline distance to the star polygon {n/k} of circumradius `r`: every
// vertex joined to the k-th next one (k = 1 is the plain polygon).
fn polygram(p: vec2<f32>, n: u32, k: u32, r: f32, rot: f32) -> f32 {
    var d = 1e3;
    for (var i = 0u; i < n; i++) {
        let a0 = rot + f32(i) * TAU / f32(n);
        let a1 = rot + f32((i + k) % n) * TAU / f32(n);
        d = min(d, segment(p, r * vec2<f32>(cos(a0), sin(a0)), r * vec2<f32>(cos(a1), sin(a1))).x);
    }
    return d;
}

fn trace_point(i: u32) -> vec2<f32> {
    let v = P.trace[(i / 2u) & 3u];
    if ((i & 1u) == 0u) {
        return v.xy;
    }
    return v.zw;
}

// ---------------------------------------------------------------- ground

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    return 0.5 * vnoise(p) + 0.3 * vnoise(p * 2.1 + 5.0) + 0.2 * vnoise(p * 4.3 - 2.0);
}

// Dark blue wash, brush streaks, a few violet patches, a vignette and sparse
// splatter in the glyph's colours. Barely reactive.
fn ground(p: vec2<f32>, energy: f32) -> vec3<f32> {
    let q = p * 1.1 + vec2<f32>(P.time * 0.008, P.time * 0.004);
    let wash = fbm(q);
    let streak = vnoise(vec2<f32>(p.x * 2.2 + p.y * 0.7, p.y * 16.0 + wash * 3.0));
    var g = vec3<f32>(0.020, 0.040, 0.135) * (0.5 + 1.0 * wash) * (0.8 + 0.4 * streak);
    let blotch = fbm(q * 2.3 + 9.0);
    g += vec3<f32>(0.06, 0.025, 0.14) * blotch * blotch * 0.7;
    g += vec3<f32>(0.0, 0.03, 0.05) * pow(vnoise(q * 0.7 + 31.0), 3.0);
    g *= 1.0 - 0.55 * smoothstep(0.7, 1.8, length(p));
    // Splatter: one dot in a few of the cells of a grid, static per cell.
    let cell = 16.0;
    let gi = floor(p * cell);
    let h = hash22(gi);
    if (h.x < 0.11) {
        let cp = (gi + 0.25 + 0.5 * hash22(gi + 3.1)) / cell;
        let rr = (0.12 + 0.45 * h.y * h.y) / cell * 0.5;
        let d = length(p - cp);
        let cc = col_at(hash21(gi + 7.7));
        g += cc * (0.25 + 0.6 * h.y) * smoothstep(rr, rr * 0.4, d) * (0.6 + 0.4 * lit(0.3 + 0.6 * h.y, h.x * 900.0));
    }
    return g * P.style.y * (0.92 + 0.12 * energy);
}

// ---------------------------------------------------------------- glyph

@fragment
fn fs_main(in: FsIn) -> Out {
    let res = P.resolution;
    let short = min(res.x, res.y);
    // Half-extent 1 along the short side, then the glyph scale.
    let p = (in.uv * 2.0 - 1.0) * vec2<f32>(res.x / short, -res.y / short) / P.scale;
    let px = 2.0 / (short * P.scale);   // one pixel in glyph units
    let lw = P.pen.z;
    let energy = P.music.x;
    let onset = P.music.y;
    let beat = P.music.z;
    let flare = P.music.w;
    let react = P.misc.w;
    let spin = P.misc.z;

    let prev = textureSample(trail_tex, samp, in.uv).rgb;

    let r = length(p);
    let ang = atan2(p.y, p.x);

    var acc = vec3<f32>(0.0);
    if (P.style.y > 0.001) {
        acc = ground(p, energy);
    }
    var fresh = vec3<f32>(0.0);

    // Centre rings, one per band.
    for (var i = 0u; i < N_RINGS; i++) {
        let R = P.rings[i];
        let b = band6(i);
        let rad = R.x * (1.0 + 0.06 * b * react);
        let w = max(R.y * lw * (1.0 + 0.5 * b * react), px * 1.1);
        var d = abs(r - rad);
        var m = 1.0;
        if (R.z > 0.5) {
            // Dashed: R.z ticks around the ring, soft gaps.
            let a = fract((ang + R.w + spin) * R.z / TAU);
            m = smoothstep(0.14, 0.22, abs(a - 0.5));
        } else if (R.z < -0.5) {
            // Double: two thin lines either side.
            d = abs(d - 0.018);
        }
        var bright = (0.3 + 0.6 * b * react) * P.misc.y;
        if (i == 0u) {
            bright = (bright + 0.8 * beat * react) * P.build.y;
        }
        acc += tube(d, w, col_at(f32(i) / 6.0 + 0.04)) * bright * m * lit(f32(i) * 0.11, f32(i) + 1.0);
    }

    // Rays: from the inner ring outward, tapered, fading along their length.
    let r0 = P.rings[0].x * 0.9;
    for (var s = 0u; s < N_RAYS; s++) {
        let Rr = P.rays[s];
        if (Rr.y < 0.004) {
            continue;
        }
        let a = Rr.x + spin;
        let dir = vec2<f32>(cos(a), sin(a));
        let len = Rr.z * (1.0 + 0.6 * flare * react);
        let sg = segment(p, dir * r0, dir * (r0 + len));
        let t = sg.y;
        let fade = (1.0 - t) * (1.0 - t);
        let w = max(0.0055 * lw * (1.0 - 0.6 * t), px * 1.1);
        fresh += tube(sg.x, w, col_at(Rr.w)) * Rr.y * fade * P.style.z * lit(0.25, 40.0 + f32(s));
    }

    // Orbs: twelve circle-and-dot on the orbit; only the nearest is evaluated.
    {
        let orad = P.orbit.x * (1.0 + 0.03 * band6(1u) * react);
        let step_a = TAU / 12.0;
        let k = floor(0.5 + (ang - P.orbit.y) / step_a);
        let kk = u32((i32(k) % 12 + 12) % 12);
        let ca = P.orbit.y + k * step_a;
        let c = orad * vec2<f32>(cos(ca), sin(ca));
        let dc = length(p - c);
        let ch = chroma(kk);
        let is_key = select(0.0, 1.0, u32(P.build.w) == kk);
        let bright = (0.22 + 1.3 * ch * react + 0.3 * is_key * react) * P.orbit.w;
        let col = col_at(f32(kk) / 12.0 + 0.5);
        let w = max(0.0055 * lw, px * 1.1);
        let ring = tube(abs(dc - P.orbit.z), w, col);
        let dot_r = P.orbit.z * (0.22 + 0.25 * ch * react + 0.2 * beat * react);
        let dotl = tube(max(dc - dot_r, 0.0), w * 1.3, col) * 1.2;
        acc += (ring + dotl) * bright * lit(0.62 + 0.02 * f32(kk % 3u), 100.0 + f32(kk));
    }

    // Centre polygram, nested, breathing with the bass and flashing on the beat.
    if (P.tri.w > 0.001) {
        let n = u32(P.shape.x);
        let k = u32(P.shape.y);
        let nest = u32(P.trace_meta.w);
        let bass = band6(1u);
        let pcol = col_at(0.9);
        let w = max(0.005 * lw, px * 1.1);
        for (var lv = 0u; lv < nest; lv++) {
            let shrink = pow(P.pen.w, f32(lv));
            let rr = P.shape.z * shrink * (1.0 + 0.05 * bass * react);
            // Levels overlay: each a fraction of a step round, alternate ones counter-turning.
            let rot = P.shape.w * select(1.0, -1.3, (lv & 1u) == 1u) + f32(lv) * PI / f32(n) / f32(max(nest, 1u)) * 2.0;
            let d = polygram(p, n, k, rr, rot);
            let bright = (0.45 + 0.35 * bass * react + 0.5 * beat * react) * P.tri.w;
            acc += tube(d, w, pcol) * bright * lit(0.15 + 0.08 * f32(lv), 200.0 + f32(lv));
        }
    }

    // Cardinal triangles (nested, pointing outward) and diagonal diamonds.
    if (P.tri.w > 0.001) {
        let nest = u32(P.trace_meta.w);
        let step_a = TAU / 8.0;
        let kq = floor(0.5 + ang / step_a);
        let kq8 = u32((i32(kq) % 8 + 8) % 8);
        let ca = kq * step_a;
        let dir = vec2<f32>(cos(ca), sin(ca));
        let cm = P.tri.x * dir;
        let q = p - cm;
        let w = max(0.005 * lw, px * 1.1);
        if ((kq8 & 1u) == 0u) {
            // Cardinal: triangles, apex outward, own-axis turn on bars.
            let mid = band6(3u);
            let tcol = col_at(0.65);
            for (var lv = 0u; lv < nest; lv++) {
                let size = P.tri.y * pow(0.62, f32(lv));
                let d = abs(polygon(q, 3.0, size, -ca + P.tri.z * select(1.0, -1.0, (lv & 1u) == 1u)));
                let bright = (0.4 + 0.5 * mid * react) * P.tri.w;
                acc += tube(d, w, tcol) * bright * lit(0.4 + 0.07 * f32(lv), 300.0 + f32(lv) + f32(kq8));
            }
        } else {
            // Diagonal: a diamond lit by the treble, with a dot.
            let treb = band6(4u);
            let dcol = col_at(0.3);
            let d = abs(polygon(q, 4.0, P.tri.y * 0.7 * (1.0 + 0.15 * treb * react), -ca));
            let bright = (0.3 + 0.7 * treb * react) * P.tri.w;
            acc += (tube(d, w, dcol) + tube(length(q), w * 1.2, dcol) * 0.8) * bright * lit(0.52, 320.0 + f32(kq8));
        }
    }

    // Sigil trace: a polyline through orbit points, a dim steady line with a
    // beat-driven pen whose glow stays in the trail.
    let tn = u32(P.trace_meta.x);
    if (tn >= 2u && P.trace_meta.z > 0.001) {
        let w = max(0.004 * lw, px * 1.1);
        let tcol = col_at(0.5 + P.style.w * 0.0);
        var d = 1e3;
        for (var i = 0u; i + 1u < tn; i++) {
            d = min(d, segment(p, trace_point(i), trace_point(i + 1u)).x);
        }
        let p0 = trace_point(0u);
        let pe = trace_point(tn - 1u);
        let pprev = trace_point(tn - 2u);
        // Terminals: a small circle at the start, a cross-bar at the end.
        d = min(d, abs(length(p - p0) - 0.03));
        let tdir = normalize(pe - pprev);
        let perp = vec2<f32>(-tdir.y, tdir.x);
        d = min(d, segment(p, pe - perp * 0.035, pe + perp * 0.035).x);
        let on = lit(0.75, 400.0);
        acc += tube(d, w, tcol) * 0.3 * P.trace_meta.z * on;
        // Pen: a stroke from where it was last frame to where it is, via the
        // vertex when it crossed one, so the trail is a continuous line.
        let pen = P.trace_meta.y;
        let pen0 = P.pen.x;
        let pa = trace_at(pen0, tn);
        let pb = trace_at(pen, tn);
        var dpen = 1e3;
        if (abs(pen - pen0) < 1.5 && floor(pen) != floor(pen0)) {
            let v = trace_point(u32(clamp(max(floor(pen), floor(pen0)), 0.0, f32(tn - 1u))));
            dpen = min(segment(p, pa, v).x, segment(p, v, pb).x);
        } else if (abs(pen - pen0) < 1.5) {
            dpen = segment(p, pa, pb).x;
        } else {
            dpen = length(p - pb);
        }
        fresh += tube(dpen, w * 1.6, tcol) * (0.8 + 0.6 * beat * react) * P.trace_meta.z * on;
    }

    // Drips: paint running down from bright elements, a tapering line with a
    // bead at the head, kept in the trail.
    if (P.build.z > 0.001) {
        let w = max(0.0065 * lw, px * 1.1);
        for (var i = 0u; i < 16u; i++) {
            let st = drip_s(i);
            if (st < 0.01) {
                continue;
            }
            let D = P.drips[i];
            if (abs(p.x - D.x) > 0.12) {
                continue;
            }
            let sg = segment(p, vec2<f32>(D.x, D.y), vec2<f32>(D.x, D.z));
            let taper = mix(1.0, 0.4, sg.y);
            let bead = length(p - vec2<f32>(D.x, D.z)) - w * 0.8;
            let d = min(sg.x / taper, max(bead, 0.0));
            fresh += tube(d, w, col_at(D.w)) * st * 0.8 * P.build.z;
        }
    }

    // Drops: a soft flare from the centre, left in the trail.
    fresh += col_at(P.style.w + 0.5) * flare * react * exp(-r * 4.5) * 0.45 * P.build.y * lit(0.0, 7.0);

    // Persistence: afterimages keep the brightest of the fade and the new light.
    let trail = max(prev * P.misc.x, fresh);

    var out: Out;
    out.color = vec4<f32>(acc + trail, 1.0);
    out.trail = vec4<f32>(trail, 1.0);
    return out;
}
