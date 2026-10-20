// The lion's mane's bell: its pigment, what it covers, its own light.

// A lion's mane's pigment: sixteen rounded gastric pouches round the middle,
// red-brown, their scalloped edge about halfway out (0..1 where they are).
fn lion_pouches(L: Look) -> f32 {
    let v = 0.5 + 0.5 * cos(L.th * TAU * 16.0);
    let edge = 0.42 + 0.14 * sqrt(v);
    return smoothstep(edge + 0.06 + L.fs, edge - 0.04 - L.fs, L.s);
}

fn body_colour(k: u32, L: Look, body: vec3<f32>) -> vec3<f32> {
    if (params().colours <= 0.5) {
        return body;
    }
    // Red-brown in the pouches, amber on the lobes.
    return body * mix(vec3<f32>(1.0), vec3<f32>(0.72, 0.36, 0.3), lion_pouches(L));
}

fn bell_pigment(J: Jelly, k: u32, L: Look, b: Bell) -> f32 {
    return 1.0;
}

// How much of what is behind its bell it covers. The glass species' cover
// nothing (their light adds); a lion's mane is thick, coloured jelly, far
// less see-through, and its inner skin carries the folded tissue.
fn bell_cover(k: u32, L: Look) -> f32 {
    let lp = lappets(k);
    let x = fract(L.th * lp.x) * 2.0 - 1.0;
    let rim_s = 1.0 - lp.y * (1.0 - sqrt(max(1.0 - x * x, 0.0)));
    let inside = smoothstep(rim_s + L.fs, rim_s - L.fs, L.s);
    // Thick in the middle, thinning to the margin.
    let thick = mix(0.6, 0.35, smoothstep(0.3, 1.0, L.s));
    return select(thick, 0.3 * (1.0 - 0.6 * smoothstep(0.4, 1.0, L.s)), L.inner) * inside;
}

fn own_light(J: Jelly, k: u32, L: Look) -> vec3<f32> {
    return lion_light(J, L);
}

// Lion's mane. Outer skin: the edge of the sixteen pouches, fine muscle
// fibres running out over the lobes, the clefts between them, and a
// rhopalium in each. Inner: the folded tissue of the stomach and gonads in
// a four-lobed mass in the middle, glowing.
fn lion_light(J: Jelly, L: Look) -> vec3<f32> {
    let hue = hue_of(J);
    let s = L.s;
    let glow = soft(J.glow);
    let org = tone(ORGANS, hue);
    if (L.inner) {
        // The sixteen pouches, red tissue seen through the jelly, and in the
        // middle the folded mass of stomach and gonads, four-lobed.
        let ang = L.th * TAU;
        let reach = 0.3 * (1.0 + 0.35 * cos(ang * 4.0));
        let mass = smoothstep(reach + 0.08, reach - 0.1, s);
        let folds = 0.55 + 0.9 * abs(fbm(L.plan * 14.0 + J.hue * 5.0, 3) - 0.5) * 2.0;
        let pouch = lion_pouches(L) * (0.6 + 0.4 * fbm(L.plan * 7.0 + J.hue * 3.0, 2));
        return diffuse(org * (mass * folds * 0.4 + pouch * 0.1) * (0.5 + glow * 1.5), ORGANS_DIFFUSE, params().light);
    }
    let body = tone(BODY, hue);
    // The lobes' own amber, warm through the thin jelly.
    var c = body * smoothstep(0.35, 0.95, s) * (0.05 + 0.02 * fbm(L.plan * 5.0 + J.hue * 3.0, 2));
    // The pouches' scalloped edge, a faint darker line.
    let v = 0.5 + 0.5 * cos(L.th * TAU * 16.0);
    let edge = 0.42 + 0.14 * sqrt(v);
    c += org * thin_line(abs(s - edge), 0.006, L.fs) * 0.05;
    // Fibres over the lobes: many, fine, uneven, fading out as they get too
    // fine to draw.
    let fib = pow(0.5 + 0.5 * cos(L.th * TAU * 96.0 + 0.6 * noise(vec2<f32>(L.th * 40.0, s * 4.0))), 6.0);
    let fine = keep_detail(L.fp * 96.0 / max(s, 0.1));
    c += body * fib * fine * smoothstep(0.45, 0.75, s) * 0.035;
    // The clefts between the lobes, and a rhopalium at the head of each.
    let x = fract(L.th * 8.0);
    let d = min(x, 1.0 - x) * TAU / 8.0 * s;
    c += org * thin_line(d, 0.004, L.fp) * smoothstep(0.7, 0.95, s) * 0.12;
    let rh = vec2<f32>(d, s - 0.9);
    c += diffuse(tone(LIGHT, hue) * exp(-dot(rh, rh) / (0.012 * 0.012)) * (0.2 + 0.8 * glow), POINTS_DIFFUSE, params().light);
    return diffuse(c * (0.6 + glow), PATTERN_DIFFUSES, params().light);
}
