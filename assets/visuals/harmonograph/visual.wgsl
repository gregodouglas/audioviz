// Harmonograph: one long damped Lissajous figure drawn as a line. The swing
// across runs at 1, the swing up at the ratio of the music's two strongest
// pitch classes (an interval of k semitones is 2^(k/12): a fifth is ~3:2),
// each with a slightly detuned partner that makes the figure precess.
//
// prepare() keeps the state: the ratio gliding toward the current interval,
// and the precession phase advancing on the music clock. A build opens the
// figure (less damping) until the drop, it sways once a bar, and each
// section of the song has its own place on the palette. point(i) gives the
// line; there is no shade(), so the line is the whole picture.

// Item 0: (ratio, phase, 0, 0), last frame's inside prepare(). Item 1: (the
// build, the section's palette offset, how sure the beat is), eased.
fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    if (i == 1u) {
        let s = item(1u);
        return vec4<f32>(ease(s.x, build(), 1.0, 1.2), ease(s.y, section_seed(2.0) * 0.5 * P.sections, 2.5, 2.5), ease(s.z, V.confidence, 0.5, 1.0), 0.0);
    }
    // The two strongest pitch classes.
    var a = 0u;
    for (var k = 1u; k < 12u; k++) {
        if (chroma(k) > chroma(a)) { a = k; }
    }
    var b = (a + 7u) % 12u;
    for (var k = 0u; k < 12u; k++) {
        if (k != a && chroma(k) > chroma(b)) { b = k; }
    }
    let semis = f32((b + 12u - a) % 12u);
    let want = exp2(semis / 12.0);
    let last = item(0u);
    // First frame (nothing stored yet): start on the interval.
    let ratio = select(mix(last.x, want, 1.0 - exp(-V.dt / P.glide)), want, last.x <= 0.0);
    let phase = fract(last.y + V.dt * (0.5 + 0.5 * V.energy) * P.speed * 0.2);
    return vec4<f32>(ratio, phase, 0.0, 0.0);
}

fn point(i: u32) -> LinePoint {
    let P = params();
    let state = item(0u);
    let ratio = max(state.x, 1.0);
    let ph = state.y * TAU;
    let u = f32(i) / f32(LINE_POINTS - 1u);
    let t = u * P.turns * TAU;
    // Damping per whole figure, so Turns changes density, not how far it dies.
    let shape = item(1u);
    let damp = exp(-40.0 * P.damping * (1.0 - 0.7 * P.opens * shape.x) * u);
    let f2 = 1.0 + P.detune;
    let r2 = ratio * (1.0 + P.detune * 0.7);
    var p = vec2<f32>(
        0.8 * sin(t + ph) + 0.2 * sin(f2 * t + 2.0 * ph),
        0.8 * sin(ratio * t + 1.5707963) + 0.2 * sin(r2 * t - ph),
    );
    let size = P.size * (1.0 + 0.15 * V.swell);
    p = rotate(p * damp * size, P.sway * 0.1 * sin(TAU * bar_phase()) * shape.z);
    // The waveform shakes the line along its normal-ish direction.
    p += vec2<f32>(wave(u).x, wave(u).y) * P.wave * 0.04;
    var q: LinePoint;
    q.pos = p;
    q.width = P.width * (0.5 + 0.7 * damp);
    q.colour = colour(u * P.span + 0.1 * state.y + shape.y) * (0.25 + 1.2 * damp) * (0.6 + 0.6 * V.energy);
    return q;
}
