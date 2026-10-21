// Eclipse: a black disc ringed by spectrum dots, with a corona at the rim.
// Written against docs/visuals.md: params() from visual.json, the music in V,
// colour(t) for the layer's palette. The dots are worked out once a frame in
// prepare() (two items per dot), so shade() only measures distances.

// Colour at a place round the ring (0..1 from the bottom, mirrored when on).
fn ring_colour(t: f32, mirror: bool, span: f32) -> vec3<f32> {
    var u = t;
    if (mirror) { u = 1.0 - abs(1.0 - 2.0 * t); }
    return colour(u * span) * 1.2;
}

fn dot_count() -> u32 {
    return u32(clamp(params().count, 4.0, 64.0));
}

// Once a frame per item: dot d is items 2d (centre x, centre y, size,
// level) and 2d + 1 (colour).
fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let n = dot_count();
    let d = i / 2u;
    if (d >= n) {
        return vec4<f32>(0.0);
    }
    let mirror = P.mirror > 0.5;
    if (i % 2u == 1u) {
        return vec4<f32>(ring_colour((f32(d) + 0.5) / f32(n), mirror, P.range), 0.0);
    }
    // Mirrored, the bands run up both sides from the bottom.
    let span = select(f32(n), f32((n + 1u) / 2u), mirror);
    var src = f32(d);
    if (mirror && f32(d) >= span) { src = f32(n - 1u - d); }
    let t = src / max(span, 1.0);
    let level = select(band_at(t), level_at(t), P.spectrum > 0.5) * P.gain;
    let lvl = pow(clamp(level, 0.0, 4.0), max(P.curve, 0.1));
    let spin = fract(V.flow * P.spin) * TAU;
    let a = -1.5707963 + (f32(d) + 0.5) * TAU / f32(n) + spin;
    let rr = P.radius * (1.0 + 0.06 * P.react * min(lvl, 2.0));
    let s = max(P.size * (0.55 + 0.6 * P.react * min(lvl, 1.5)), 0.002);
    return vec4<f32>(cos(a) * rr, sin(a) * rr, s, lvl);
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let p = centred(uv);
    let r = length(p);
    let aa = 0.75 * pixel();
    let mirror = P.mirror > 0.5;
    let n = dot_count();
    let spin = fract(V.flow * P.spin) * TAU;
    let flare = V.swell * P.react;
    var col = vec3<f32>(0.0);
    for (var d = 0u; d < n; d++) {
        let dot_at = item(2u * d);
        let s = dot_at.z;
        let lvl = dot_at.w;
        let dist = length(p - dot_at.xy);
        let core = 1.0 - smoothstep(s - aa, s + aa, dist);
        // Halo: a soft Lorentzian tail, stronger for loud dots.
        let halo = P.glow * s * s / (dist * dist + s * s) * (0.25 + 0.75 * min(lvl, 2.0));
        let bright = 0.15 + 1.6 * lvl;
        col += item(2u * d + 1u).rgb * (core * bright + halo * bright * 0.6);
    }
    // Corona: a bright thin rim and a soft outward glow that flares on the bass.
    let rd = P.radius * P.disc;
    let outside = max(r - rd, 0.0);
    let ang = atan2(p.y, select(p.x, 1e-6, abs(p.x) < 1e-6));
    let t = fract((ang + 1.5707963 - spin) / TAU);
    let rim = exp(-outside / max(0.004, 0.006 * (1.0 + flare)));
    let soft = exp(-outside / (0.06 * (1.0 + 0.8 * flare)));
    col += ring_colour(t, mirror, P.range) * P.corona * (0.6 * rim + 0.35 * soft) * (0.4 + flare);
    // The disc is black: it masks everything behind it, dots included.
    return col * smoothstep(rd - aa, rd + aa, r);
}
