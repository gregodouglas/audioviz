// The lion's mane's bell (ADR-0005; the photos in tmp/ref/lions-mane): solid,
// pigmented tissue over the middle two thirds, amber deepening to red-brown
// toward the crown, with a web of polygon canals across it; fading into a
// broad band of clear, milky, bluish jelly, the lobes: sixteen in eight
// pairs (the mesh cuts the deep clefts between the pairs, paint the shallow
// notch between the two of a pair), a rhopalium in each shallow notch. The
// tissue covers what is behind it; the band is light.

// Where the tissue gives way to the clear band, in s (it reaches a little
// into each lobe).
const CORE: f32 = 0.56;

// How far out the margin is at th (1 the lobes' tips): each pair of the
// mesh's eight lobes rounded into two, a shallow notch between them.
fn lion_margin(th: f32) -> f32 {
    let x = fract(th * 16.0) * 2.0 - 1.0;
    return 1.0 - 0.045 * (1.0 - sqrt(max(1.0 - x * x, 0.0)));
}

// How much tissue there is at (s, th): 1 in the middle, 0 in the clear band.
fn lion_tissue(L: Look) -> f32 {
    let edge = CORE + 0.05 * cos(L.th * TAU * 16.0) + 0.03 * (noise(vec2<f32>(L.th * 40.0, 3.0)) - 0.5);
    return smoothstep(edge + 0.07 + L.fs, edge - 0.05 - L.fs, L.s);
}

fn bell_shade(J: Jelly, k: u32, L: Look) -> vec4<f32> {
    let P = params();
    let hue = hue_of(J);
    let s = L.s;
    let glow = soft(J.glow);
    let natural = natural_bodies() > 0.5;
    let rim = lion_margin(L.th);
    let cut = smoothstep(rim + L.fs, rim - L.fs, s);
    let t = lion_tissue(L);
    let lit = 0.35 + 0.65 * L.top;
    // The tissue: amber, deepening toward the crown, mottled; the canals a
    // web of polygons across it, finer toward the middle, darker and redder.
    let seed = vec2<f32>(J.hue * 13.0, J.hue * 7.0);
    let amber = tone(BODY, hue);
    let red = tone(ORGANS, hue);
    var tissue = mix(amber, red, 0.6 * (1.0 - smoothstep(0.05, CORE, s)));
    tissue *= 0.8 + 0.4 * fbm(L.plan * 7.0 + seed, 3);
    // (Cells larger toward the band: the plan shrunk as it goes out.)
    // (Warped, so they wander as canals do, not ruled like tiles.)
    let q = L.plan * (7.0 / (0.45 + s)) + seed;
    let w = cells(q + 0.35 * vec2<f32>(noise(q * 1.7), noise(q * 1.7 + 5.0)), 1.0, 0.0);
    let web = smoothstep(0.06, 0.0, w.y - w.x) * keep_detail(L.fp * 8.0) * smoothstep(0.06, 0.16, s);
    tissue = mix(tissue, red * 0.55, web * 0.5);
    // Lit as a body, not glass: from above, a little light scattered up
    // through it where it thins toward the band, the wet skin's sheen.
    // (Light passes through it: the photos show the whole body glowing.)
    var c = tissue * (0.11 + 0.2 * lit * L.top + 0.14 * smoothstep(0.2, CORE, s) + 0.06 * fresnel(L.face, 2.0));
    // The middle, thickest, is the deeper and darker (bright, its red would
    // bleach toward yellow in the tonemap).
    c *= mix(0.55, 1.0, smoothstep(0.0, CORE, s));
    c += mix(amber, vec3<f32>(1.0), 0.5) * L.sheen * 0.35;
    // Its own light, faint, from the pouches inside, swelling with the music.
    c += diffuse(red * (0.012 + 0.03 * glow) * (1.0 - web), ORGANS_DIFFUSE, P.light);
    if (L.inner) {
        // Seen from below, through the mass: darker, the folds of the
        // stomach and gonads in the middle.
        c *= 0.55 + 0.3 * fbm(L.plan * 12.0 + seed * 1.7, 3);
    }
    // The clear band: milky, faintly blue, its light where the eye looks
    // through more of it and at its edges, lit by the sheen; a thin bright rim.
    // Cream-white next to the tissue, bluish toward the edge.
    let edge_blue = smoothstep(CORE + 0.05, 1.0, s);
    let milky = select(mix(amber, vec3<f32>(1.0), 0.6), mix(vec3<f32>(1.0, 0.92, 0.8), vec3<f32>(0.7, 0.83, 1.0), edge_blue), natural);
    var g = milky * (0.05 + 0.08 * (1.0 - edge_blue) + 0.1 * fresnel(L.face, 3.0) * (0.4 + 0.6 * L.sky)) * lit;
    g += milky * L.sheen * 0.3;
    g += milky * thin_line(abs(rim - s), 0.01, L.fs) * 0.25;
    // A rhopalium in each shallow notch, a small warm bead.
    let x = fract(L.th * 8.0 + 0.5) - 0.5;
    let d = vec2<f32>(x / 8.0 * TAU * s, s - rim + 0.035);
    g += diffuse(tone(LIGHT, hue) * exp(-dot(d, d) / (0.01 * 0.01)) * (0.15 + 0.6 * glow), POINTS_DIFFUSE, P.light);
    // (The band at the inner skin is fainter: the outer one has its light.)
    g *= select(1.0, 0.4, L.inner);
    let rgb = mix(shown(L_EDGES, g), shown(L_GLASS, c), t);
    return vec4<f32>(rgb * P.body * cut, t * cut);
}
