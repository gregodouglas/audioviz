// Eclipse: a black disc ringed by spectrum dots, with a corona at the rim.
// Written against docs/visuals.md: params() from visual.json, the music in V,
// colour(t) for the layer's palette.

// Colour at a place round the ring (0..1 from the bottom, mirrored when on).
fn ring_colour(t: f32, mirror: bool, span: f32) -> vec3<f32> {
    var u = t;
    if (mirror) { u = 1.0 - abs(1.0 - 2.0 * t); }
    return colour(u * span) * 1.2;
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let p = centred(uv);
    let r = length(p);
    let aa = 0.75 * pixel();
    let mirror = P.mirror > 0.5;
    let n = u32(clamp(P.count, 4.0, 64.0));
    let span = select(f32(n), f32((n + 1u) / 2u), mirror);
    let spin = fract(V.flow * P.spin) * TAU;
    let flare = V.swell * P.react;
    let step_a = TAU / f32(n);
    var col = vec3<f32>(0.0);
    for (var d = 0u; d < n; d++) {
        // Mirrored, the bands run up both sides from the bottom.
        var src = f32(d);
        if (mirror && f32(d) >= span) { src = f32(n - 1u - d); }
        let level = band_at(src / max(span, 1.0)) * P.gain;
        let lvl = pow(clamp(level, 0.0, 4.0), max(P.curve, 0.1));
        let a = -1.5707963 + (f32(d) + 0.5) * step_a + spin;
        let rr = P.radius * (1.0 + 0.06 * P.react * min(lvl, 2.0));
        let c = vec2<f32>(cos(a), sin(a)) * rr;
        let s = max(P.size * (0.55 + 0.6 * P.react * min(lvl, 1.5)), 0.002);
        let dist = length(p - c);
        let core = 1.0 - smoothstep(s - aa, s + aa, dist);
        // Halo: a soft Lorentzian tail, stronger for loud dots.
        let halo = P.glow * s * s / (dist * dist + s * s) * (0.25 + 0.75 * min(lvl, 2.0));
        let bright = 0.15 + 1.6 * lvl;
        col += ring_colour((f32(d) + 0.5) / f32(n), mirror, P.range) * (core * bright + halo * bright * 0.6);
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
