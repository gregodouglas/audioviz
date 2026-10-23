// Pulse: nested polygons driven by the music. Example 2 of
// docs/building-visuals.md: Polygon's shape, repeated in a small loop, each
// ring reading its own slice of the spectrum, kicked by the beat, flashed by
// onsets and turned by the music clock.

const PI: f32 = 3.14159265;

// Signed distance to a regular n-gon with circumradius r (see Polygon).
fn polygon(p: vec2<f32>, r: f32, n: f32) -> f32 {
    let an = PI / n;
    let a = atan2(p.x, p.y);
    let b = a - 2.0 * an * floor(a / (2.0 * an)) - an;
    var q = length(p) * vec2<f32>(cos(b), abs(sin(b)));
    q -= r * vec2<f32>(cos(an), sin(an));
    q.y += clamp(-q.y, 0.0, r * sin(an));
    return length(q) * sign(q.x);
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let p = centred(uv);
    let n = u32(P.rings);
    // The beat kicks the rings outward: kick() rises over 40 ms and decays
    // (V.beat would jump in one frame), as much as the layer's Motion says.
    let swing = 1.0 + 0.08 * P.kick * kick() * motion();
    // Onsets in any band flash every ring a little.
    let flash = 1.0 + 2.0 * P.flash * V.onset;
    var col = vec3<f32>(0.0);
    // A loop runs for every pixel: keep it short (eight rings at most here).
    for (var i = 0u; i < n; i++) {
        // Ring i reads the spectrum at its own place, bass innermost.
        let t = (f32(i) + 0.5) / f32(n);
        let level = clamp(select(band_at(t), level_at(t), P.spectrum > 0.5) * P.gain, 0.0, 3.0);
        let r = P.radius * (f32(i) + 1.0) / f32(n) * (1.0 + 0.12 * level) * swing;
        // Alternate rings turn against each other with the music clock.
        let dir = select(-1.0, 1.0, i % 2u == 0u);
        let q = rotate(p, dir * V.flow * P.twist + f32(i) * 0.3);
        let d = abs(polygon(q, r, P.sides));
        let half = 0.5 * P.width * pixel() * (1.0 + level);
        let line = clamp((half - d) / pixel() + 0.5, 0.0, 1.0);
        let glow = P.glow * max(exp(-d / 0.04) - 0.002, 0.0);
        // Quiet bands stay faint; loud ones burn.
        let bright = (0.15 + level * level) * flash;
        col += colour(t) * (2.0 * line + glow) * bright;
    }
    return col;
}
