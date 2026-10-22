// Spiral: one long line through points stepped round by an angle that drifts
// slowly, so the figure morphs between spiral arms, rosettes and star meshes
// as the step passes near simple fractions of a turn. Points sit on an
// even-density spiral (radius by the square root of the index) or on a rose
// curve joined in step order (a Maurer rose). The waveform pushes each point
// in and out; beats and drops kick it, energy speeds the drift, it sways
// once a bar, and each section of the song glides the step to its own
// figure. Colours run along the line through the palette.
//
// After the spiral family in audible-visuals by Sonia Boller, Apache-2.0.
// The idea only; no code ported (see docs/INSPIRATION.md).

// Item 0, kept from frame to frame: (drift in degrees, turn in radians,
// beat kick 0..1, last beat pulse). Item 1: (the section's step offset in
// degrees, eased; how sure the beat is, eased).
fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    if (i == 1u) {
        let s = item(1u);
        let want = (section_seed(1.0) * 2.0 - 1.0) * 12.0 * P.sections;
        return vec4<f32>(ease(s.x, want, 3.0, 3.0), ease(s.y, V.confidence, 0.5, 1.0), 0.0, 0.0);
    }
    let last = item(0u);
    let drift = last.x + V.dt * P.drift * max(1.0 + P.react * (V.energy - 1.0), 0.1);
    let turn = last.y + V.dt * P.spin * (0.5 + 0.5 * V.energy);
    var kick = last.z * exp(-V.dt / 0.25);
    if (V.beat > last.w + 0.4) {
        kick = 1.0;
    }
    kick = max(kick, 1.5 * drop_pulse());
    return vec4<f32>(drift - 360.0 * floor(drift / 360.0), turn - TAU * floor(turn / TAU), kick, V.beat);
}

// The palette over the tint, as the compiled-in Spiral mixed it.
fn line_colour(t: f32) -> vec3<f32> {
    let x = fract(t + V.palette_shift) * 32.0;
    let i = u32(floor(x)) % 32u;
    let pal = mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
    return mix(V.tint.rgb, pal * 1.2, V.palette_mix);
}

fn point(i: u32) -> LinePoint {
    let P = params();
    let state = item(0u);
    let n = u32(clamp(P.points, 16.0, 4096.0));
    var q: LinePoint;
    q.pos = vec2<f32>(0.0);
    q.width = 0.0;
    q.colour = vec3<f32>(0.0);
    if (i >= n) {
        return q;
    }
    let t = f32(i) / f32(n - 1u);
    // The step angle is applied per index; wrap it first so a large index
    // keeps its precision.
    let step = radians(P.step + state.x + item(1u).x);
    let th = fract(f32(i) * (step / TAU)) * TAU;
    let shape = u32(P.shape + 0.5);
    var r = sqrt(t);
    if (shape == 1u) {
        r = cos(clamp(P.petals, 1.0, 24.0) * th);
    } else if (shape == 3u) {
        // Flower: a circle the scatter spreads into petals.
        r = 0.6;
    }
    let w = wave_mono(1.0 - t) * P.gain;
    let push = w * P.amp * (1.0 + P.react * state.z);
    r = r * P.radius + push;
    let a = th + state.y + P.sway * 0.12 * sin(TAU * bar_phase()) * item(1u).y;
    q.pos = vec2<f32>(cos(a), -sin(a)) * r;
    if (shape == 2u || shape == 3u) {
        // Wavy and Flower: a scatter whose frequency along the line is so
        // high that the points alias into rosettes, which morph as its
        // period drifts (after Sonia Boller's wavy spiral and flower).
        let base = select(2.44, 2.86, shape == 3u);
        let k = 100.0 / (base + 0.025 * sin(radians(state.x) * 20.0));
        let ph = fract(f32(i) * k / TAU) * TAU;
        q.pos += vec2<f32>(sin(ph), cos(ph)) * select(0.07, 0.32, shape == 3u) * P.radius;
    } else if (shape == 4u) {
        // Circle: a ring seen from above at an angle, the waveform lifting it.
        let c = t * TAU + state.y;
        // The lift eases to nothing at both ends, so the ring closes.
        let seam = smoothstep(0.0, 0.06, t) * smoothstep(1.0, 0.94, t);
        q.pos = vec2<f32>(sin(c), 0.45 * cos(c)) * P.radius + vec2<f32>(0.0, 2.0 * push * seam);
    }
    q.width = P.width;
    q.colour = line_colour(t) * P.intensity;
    return q;
}
