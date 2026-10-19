// Spiral: one long line through points stepped round by an angle that drifts
// slowly, so the figure morphs between spiral arms, rosettes and star meshes
// as the step passes near simple fractions of a turn. Points sit on an
// even-density spiral (radius by the square root of the index) or on a rose
// curve joined in step order (a Maurer rose). The waveform pushes each point
// in and out; beats kick it, energy speeds the drift. Colours run along the
// line through the palette.
//
// After the spiral family in audible-visuals by Sonia Boller, Apache-2.0.
// The idea only; no code ported (see docs/INSPIRATION.md).

// Item 0, kept from frame to frame: (drift in degrees, turn in radians,
// beat kick 0..1, last beat pulse).
fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let last = item(0u);
    let drift = last.x + V.dt * P.drift * max(1.0 + P.react * (V.energy - 1.0), 0.1);
    let turn = last.y + V.dt * P.spin * (0.5 + 0.5 * V.energy);
    var kick = last.z * exp(-V.dt / 0.25);
    if (V.beat > last.w + 0.4) {
        kick = 1.0;
    }
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
    let step = radians(P.step + state.x);
    let th = fract(f32(i) * (step / TAU)) * TAU;
    var r = sqrt(t);
    if (P.shape > 0.5) {
        r = cos(clamp(P.petals, 1.0, 24.0) * th);
    }
    let w = wave_mono(1.0 - t) * P.gain;
    r = r * P.radius + w * P.amp * (1.0 + P.react * state.z);
    let a = th + state.y;
    q.pos = vec2<f32>(cos(a), -sin(a)) * r;
    q.width = P.width;
    q.colour = line_colour(t) * P.intensity;
    return q;
}
