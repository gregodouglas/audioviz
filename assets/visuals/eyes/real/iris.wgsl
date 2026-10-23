// The iris, seen through the cornea. Measured from the pupil's edge
// (u = 0) to the limbus (u = 1), so when the pupil widens the tissue
// bunches toward the rim as a real one does. Its parts, as in macro
// photographs: the stroma's radial fibres (white-ish in blue eyes, buried
// in pigment in brown ones), the collarette (a wavy ridge about a third of
// the way out, parting the pupillary zone, often another colour, from the
// ciliary zone), crypts (dark pits by the collarette), contraction furrows
// (rings near the rim), a dark limbal ring, the pupil's ruff, and now and
// then a freckle.

// Iris colours (linear): stroma, the pupillary zone, the fibres' light,
// and how much the fibres show.
struct Pigment {
    base: vec3<f32>,
    inner: vec3<f32>,
    fibre: vec3<f32>,
    show: f32,
};

fn pigment(kind: u32) -> Pigment {
    switch (kind) {
        // Hazel: green-brown, an amber ring round the pupil.
        case 1u: { return Pigment(vec3<f32>(0.16, 0.14, 0.05), vec3<f32>(0.34, 0.15, 0.03), vec3<f32>(0.42, 0.36, 0.16), 0.6); }
        // Green: green-grey, a little amber round the pupil.
        case 2u: { return Pigment(vec3<f32>(0.10, 0.18, 0.09), vec3<f32>(0.26, 0.19, 0.05), vec3<f32>(0.36, 0.50, 0.30), 0.7); }
        // Blue: little pigment; the stroma scatters blue, its fibres pale.
        case 3u: { return Pigment(vec3<f32>(0.10, 0.20, 0.36), vec3<f32>(0.20, 0.24, 0.30), vec3<f32>(0.55, 0.66, 0.80), 0.9); }
        // Grey: blue's paler, duller cousin.
        case 4u: { return Pigment(vec3<f32>(0.22, 0.25, 0.28), vec3<f32>(0.24, 0.24, 0.22), vec3<f32>(0.55, 0.58, 0.62), 0.8); }
        // Brown: dense pigment hiding most of the fibres.
        default: { return Pigment(vec3<f32>(0.12, 0.05, 0.018), vec3<f32>(0.07, 0.028, 0.01), vec3<f32>(0.28, 0.13, 0.05), 0.35); }
    }
}

// Which kind eye k has: chosen, or its own (brown most often, as in
// people).
fn iris_kind(k: u32) -> u32 {
    let pick = u32(params().iris);
    if (pick > 0u) {
        return pick - 1u;
    }
    let r = rand(k, 41.0);
    if (r < 0.4) { return 0u; }
    if (r < 0.58) { return 1u; }
    if (r < 0.72) { return 2u; }
    if (r < 0.9) { return 3u; }
    return 4u;
}

// The pupil's radius, in eyeball radii (2 to 8 mm across): the bass widens
// it, late.
fn pupil_radius() -> f32 {
    return clamp(0.12 + 0.07 * params().pupil * item(2u).y, 0.09, 0.3);
}

// Noise round the circle at angle `a` (seamless: sampled on the circle),
// `around` features a turn, `out` along u.
fn ring_noise(a: f32, u: f32, around: f32, out: f32, salt: f32) -> f32 {
    let r = around / TAU;
    return vnoise3(vec3<f32>(cos(a) * r, sin(a) * r, u * out) + vec3<f32>(salt, salt * 1.7, 0.0));
}

// The iris (or pupil) at `rad` from the middle, angle `ang`; `fa` is a
// pixel's width as an angle there, `fr` as a radius, for fading detail.
fn iris_colour(k: u32, rad: f32, ang: f32, fa: f32, fr: f32) -> vec3<f32> {
    let P = params();
    let pr = pupil_radius();
    if (!layer_on(2u)) {
        return vec3<f32>(0.0);
    }
    if (rad > IRIS_R) {
        // Past the iris, under the clouded rim: dark.
        return vec3<f32>(0.03, 0.025, 0.025);
    }
    let salt = f32(k) * 7.31 + 1.0;
    let u = (rad - pr) / (IRIS_R - pr);
    let edge = fr / (IRIS_R - pr);
    // The pupil: near black, its edge soft over a pixel, ringed by the ruff.
    let in_pupil = 1.0 - smoothstep(-edge, edge, u + 0.012 * ring_noise(ang, 0.0, 70.0, 0.0, salt) + 0.01 * ring_noise(ang, 0.0, 8.0, 0.0, salt + 2.0));
    var pg = pigment(iris_kind(k));
    if (u32(P.colours) == 1u) {
        // Palette: the irises in the palette's colours, the structure kept.
        let t = V.palette_shift + rand(k, 5.0) * 0.6;
        let pc = palette_at(t);
        let lum = dot(pg.base, vec3<f32>(0.3, 0.6, 0.1));
        pg.base = mix(pg.base, pc * lum * 2.6, V.palette_mix);
        pg.inner = mix(pg.inner, palette_at(t + 0.25) * lum * 2.0, V.palette_mix);
        pg.fibre = mix(pg.fibre, mix(pc, vec3<f32>(1.0), 0.4) * 0.6, V.palette_mix);
    }
    // The fibres: long radially, thin across, wavy, in four sizes, each
    // fading out when finer than a pixel; over them, a few broad bundles
    // (trabeculae) that curve as they run out.
    let wav = 0.05 * ring_noise(ang, u, 6.0, 1.5, salt + 3.0) + 0.02 * ring_noise(ang, u, 40.0, 6.0, salt + 4.0);
    var fib = 0.0;
    var wsum = 0.0;
    var around = 110.0;
    var out = 2.5;
    var w = 0.55;
    for (var o = 0; o < 4; o++) {
        let n = ring_noise(ang + wav * (1.0 + f32(o) * 0.5), u, around, out, salt + f32(o) * 11.0);
        let keep = 1.0 - smoothstep(0.3, 0.9, fa * around / TAU * 2.0);
        fib += pow(1.0 - abs(n), 4.0) * w * keep;
        wsum += w * keep;
        around *= 2.1;
        out *= 1.8;
        w *= 0.7;
    }
    fib = fib / max(wsum, 1e-3);
    fib *= 0.6 + 0.4 * (ring_noise(ang, u, 30.0, 4.0, salt + 51.0) * 0.5 + 0.5);
    let bundle_n = ring_noise(ang + 0.12 * u + wav * 2.0, u, 34.0, 0.9, salt + 61.0);
    let bundle = smoothstep(0.55, 0.95, 1.0 - abs(bundle_n)) * smoothstep(0.05, 0.25, u) * (1.0 - smoothstep(0.75, 0.95, u));
    // The collarette: a wavy ridge of brighter fibres a third of the way out.
    let uc = 0.3 + 0.045 * ring_noise(ang, 0.0, 9.0, 0.0, salt + 5.0) + 0.025 * ring_noise(ang, 0.0, 26.0, 0.0, salt + 8.0);
    let ridge = exp(-pow((u - uc) / 0.03, 2.0));
    let inner = 1.0 - smoothstep(uc - 0.05, uc + 0.02, u);
    // Crypts: soft dark pits just outside the collarette, pale at their
    // rims, longer than wide.
    let cn = ring_noise(ang + wav * 3.0, u, 15.0, 3.2, salt + 13.0) + 0.45 * ring_noise(ang, u, 37.0, 7.0, salt + 19.0);
    let zone = smoothstep(uc, uc + 0.05, u) * (1.0 - smoothstep(0.5, 0.78, u));
    let crypt = smoothstep(0.42, 0.78, cn) * zone;
    let rim = (smoothstep(0.3, 0.42, cn) - smoothstep(0.42, 0.55, cn)) * zone;
    // Contraction furrows: broken rings toward the rim.
    let fur = smoothstep(0.75, 1.0, sin(u * 46.0 + 5.0 * ring_noise(ang, u, 7.0, 1.0, salt + 17.0)))
        * smoothstep(0.66, 0.76, u) * (1.0 - smoothstep(0.88, 0.95, u))
        * smoothstep(-0.2, 0.4, ring_noise(ang, u, 12.0, 2.0, salt + 23.0)) * (1.0 - smoothstep(0.2, 0.6, fr * 46.0));
    // A freckle now and then.
    let fk = ring_noise(ang, u, 11.0, 6.0, salt + 29.0);
    let freckle = smoothstep(0.62, 0.68, fk) * step(0.55, rand(k, 77.0)) * smoothstep(0.4, 0.5, u) * (1.0 - smoothstep(0.85, 0.9, u));

    var col = mix(pg.base, pg.inner, inner * 0.7);
    col = mix(col, pg.fibre, clamp(fib * pg.show + bundle * 0.35 * pg.show, 0.0, 1.0));
    col = mix(col, pg.fibre, ridge * (0.25 + 0.4 * fib) * pg.show);
    col = mix(col, pg.base * 0.35, crypt * 0.85);
    col = mix(col, pg.fibre, rim * 0.25 * pg.show);
    col *= 1.0 - fur * 0.3;
    col = mix(col, vec3<f32>(0.09, 0.04, 0.015), freckle * 0.8);
    // Darker to the limbal ring, and in the ruff by the pupil.
    col *= 1.0 - 0.85 * smoothstep(0.82, 1.0, u);
    let ruff = 1.0 - smoothstep(0.0, 0.035, u);
    col = mix(col, vec3<f32>(0.045, 0.022, 0.012), ruff * 0.85);
    // The tissue's relief: fibres and bundles catch the light, pits fall
    // in shadow.
    col *= 0.7 + 0.45 * fib + 0.25 * bundle - 0.25 * crypt;
    let pupil = vec3<f32>(0.004, 0.003, 0.003);
    return mix(col, pupil, in_pupil);
}
