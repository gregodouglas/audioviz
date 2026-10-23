// Spiral: one long line through points stepped round by an angle that drifts
// slowly, so the figure morphs between spiral arms, rosettes and star meshes
// as the step passes near simple fractions of a turn. Points sit on an
// even-density spiral (radius by the square root of the index) or on a rose
// curve joined in step order (a Maurer rose). The figure is the step times
// each point's index, so the music never touches the step (a nudge of a
// thousandth of a degree turns the outer points by degrees): the drift is
// steady. The music lights it instead: each beat sends a pulse of light out
// along the line, and each section of the song moves the colours along the
// palette. Its motion (the waveform pushing the points in and out, the beat
// adding to that, the sway once a bar) follows the layer's Motion.
// Colours run along the line through the palette.
//
// After the spiral family in audible-visuals by Sonia Boller, Apache-2.0.
// The idea only; no code ported (see docs/INSPIRATION.md).

// Item 0, kept from frame to frame: (drift in degrees, turn in radians, the
// beat's and drops' push, eased, 0). Item 1: (the section's colour offset, eased; how sure the beat is,
// eased).
fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    if (i == 1u) {
        let s = item(1u);
        let want = section_seed(1.0) * 0.5 * P.sections;
        return vec4<f32>(ease(s.x, want, 3.0, 3.0), ease(s.y, V.confidence, 0.5, 1.0), 0.0, 0.0);
    }
    let last = item(0u);
    let drift = last.x + V.dt * P.drift;
    let turn = last.y + V.dt * P.spin * pace();
    // A drop's pulse jumps; eased in over 40 ms like the beat's kick.
    let push = ease(last.z, max(kick(), drop_pulse()), 0.04, 0.25);
    return vec4<f32>(drift - 360.0 * floor(drift / 360.0), turn - TAU * floor(turn / TAU), push, 0.0);
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
    let step = radians(P.step + state.x);
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
    let push = w * P.amp * min(motion(), 1.0) * (1.0 + 0.6 * P.react * motion() * state.z);
    r = r * P.radius + push;
    let a = th + state.y + P.sway * 0.12 * sin(TAU * bar_phase()) * item(1u).y * motion();
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
    // Each beat a pulse of light runs out along the line from its start.
    let front = since_beat() * 0.9;
    let pulse = P.react * item(1u).y * exp(-pow((t - front) / 0.06, 2.0)) * exp(-since_beat() / 0.6);
    q.colour = line_colour(t + item(1u).x) * P.intensity * (1.0 + pulse);
    return q;
}
