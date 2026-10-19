// Comet: heads on looping paths with fading trails. Example 3 of
// docs/building-visuals.md: feedback. With "feedback": true in visual.json,
// prev(uv) is this visual's own last frame; reading it a little moved and
// faded, then drawing on top, gives trails that last for seconds.

// Distance from p to the segment a..b.
fn segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let h = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-12), 0.0, 1.0);
    return length(p - a - ab * h);
}

// Where head i is at phase s: a Lissajous figure (circle, eight or knot).
fn path(s: f32, i: f32, heads: f32, figure: f32, radius: f32) -> vec2<f32> {
    var f = vec2<f32>(1.0, 1.0);
    if (figure > 1.5) {
        f = vec2<f32>(3.0, 2.0);
    } else if (figure > 0.5) {
        f = vec2<f32>(1.0, 2.0);
    }
    let a = s * TAU + i / heads * TAU;
    return vec2<f32>(sin(f.x * a + 1.5707963), sin(f.y * a)) * radius;
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let p = centred(uv);

    // The trail: last frame read slightly nearer the centre (so it drifts
    // outward) and turned, then faded by a time constant, so it looks the
    // same at 60 and 240 fps. A fade that only multiplies never reaches zero
    // in a half-float texture, so a little is also taken away.
    let q = rotate(p * (1.0 - P.drift * V.dt), -P.swirl * V.dt);
    let quv = uncentred(q);
    let inside = all(quv >= vec2<f32>(0.0)) && all(quv <= vec2<f32>(1.0));
    let keep = exp(-V.dt / P.trail);
    var col = select(vec3<f32>(0.0), max(prev(quv) * keep - 0.02 * V.dt, vec3<f32>(0.0)), inside);

    // The heads move on the music clock. V.flow advanced by
    // V.dt * (0.5 + 0.5 * V.energy) this frame, so the segment from last
    // frame's place to this one's is drawn and the trail has no gaps.
    let s1 = V.flow * P.speed * 0.25;
    let s0 = (V.flow - V.dt * (0.5 + 0.5 * V.energy)) * P.speed * 0.25;
    let radius = P.radius * (1.0 + 0.15 * V.swell);
    let n = u32(P.heads);
    for (var i = 0u; i < n; i++) {
        let fi = f32(i);
        let d = segment(p, path(s0, fi, P.heads, P.figure, radius), path(s1, fi, P.heads, P.figure, radius));
        let half = 0.5 * P.size * pixel() * (1.0 + 0.5 * V.beat);
        let core = clamp((half - d) / pixel() + 0.5, 0.0, 1.0);
        let c = colour(fi / P.heads + V.flow * 0.02) * (3.0 * core + 0.5 * max(exp(-d / (4.0 * half)) - 0.01, 0.0));
        // max, not +: a head resting in one place cannot pile up without bound.
        col = max(col, c);
    }
    return col;
}
