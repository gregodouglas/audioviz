// Equalizer rings: concentric rings of dashes round a swirling core. Ring i
// of n is the i-th slice of the spectrum on a log axis (the lows inside);
// round it, its dashes run through that slice, mirrored about the top, each
// as thick as its level (measured against the loudest slice, followed over
// seconds). Neighbouring rings turn opposite ways on the music clock. The
// core is drifting noise, swirled, that swells with the bass.
//
// Item 0: (the turn, the bass swell eased, 0, the loudest slice followed).

fn ring_count() -> u32 {
    return clamp(u32(params().rings), 2u, 12u);
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let s = item(0u);
    let dt = clamp(V.dt, 0.0, 0.1);
    var m = 0.0;
    for (var k = 0u; k < 24u; k++) {
        m = max(m, spectrum_log((f32(k) + 0.5) / 24.0));
    }
    let swell = ease(s.y, clamp(broad_rel(BASS) - 0.8, 0.0, 1.5) + 0.6 * broad_attack(BASS), 0.04, 0.3);
    let turn = s.x + dt * P.spin * 0.3 * pace();
    return vec4<f32>(turn - TAU * 64.0 * floor(turn / (TAU * 64.0)), swell, 0.0, ease(s.w, max(m, 0.2), 0.05, 3.0));
}

fn slice_level(t: f32) -> f32 {
    let P = params();
    let range = max(P.range, 6.0) / 80.0;
    let top = max(item(0u).w, 0.2);
    // A light tilt so the outer (high) rings are not starved.
    let tilt = 0.035 * (t * 9.06 - 5.0);
    return clamp((spectrum_log(t) + tilt - (top - range)) / range * P.gain, 0.0, 1.0);
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let px = pixel();
    let p = centred(uv);
    let r = length(p);
    let st = item(0u);
    let n = ring_count();
    let m = clamp(u32(P.dashes), 12u, 128u);
    let inner = P.inner * (1.0 + 0.1 * P.pulse * st.y * motion());
    let outer = max(P.outer, inner + 0.1);
    let w = (outer - inner) / f32(n);
    var out = vec3<f32>(0.0);

    // The rings.
    let fi = (r - inner) / w;
    if (fi >= 0.0 && fi < f32(n)) {
        let i = u32(floor(fi));
        let rc = inner + (f32(i) + 0.5) * w;
        let dir = select(-1.0, 1.0, i % 2u == 0u);
        let a = atan2(p.x, p.y) - dir * st.x * (1.0 + 0.25 * f32(i));
        let s = fract(a / TAU);
        let k = floor(s * f32(m));
        let f = fract(s * f32(m));
        // Through the ring's slice, mirrored about the top.
        let u = (k + 0.5) / f32(m);
        let v = 1.0 - abs(2.0 * u - 1.0);
        let t = (f32(i) + v) / f32(n);
        let level = slice_level(t);
        let half_t = 0.5 * w * (0.12 + 0.8 * level);
        let half_a = 3.14159265 / f32(m) * (1.0 - P.gap);
        let d = max(abs(r - rc) - half_t, (abs(f - 0.5) * TAU / f32(m) - half_a) * r);
        var c = colour((f32(i) + 0.5) / f32(n));
        if (P.colouring > 1.5) {
            c = colour(u);
        } else if (P.colouring > 0.5) {
            c = colour(0.15 + 0.8 * level);
        }
        c *= 0.6 + 0.8 * level;
        out += c * clamp(0.5 - d / px, 0.0, 1.0);
        out += c * max(exp(-max(d, 0.0) / (5.0 * px)) - 0.05, 0.0) * 0.25 * P.glow;
    }

    // The core: swirled drifting noise, swelling with the bass.
    let rcore = inner * 0.82;
    if (r < rcore) {
        let tm = V.flow;
        let q = rotate(p, r * 6.0 - tm * 0.7) / max(rcore, 1e-3);
        let v = 0.55 * noise(q * 3.0 + vec2<f32>(tm * 0.5, -tm * 0.3)) + 0.45 * noise(q * 7.0 - vec2<f32>(tm * 0.8, tm * 0.4));
        let fall = pow(max(1.0 - r / rcore, 0.0), 1.2);
        let c = colour(0.2 + 0.6 * v) * (0.5 + 1.4 * v * v);
        out += c * fall * P.core * (0.8 + 0.6 * P.pulse * st.y);
    }
    // A rim of light round the core.
    let rim = abs(r - rcore) - 0.6 * px;
    out += colour(0.5) * 0.6 * P.core * max(exp(-max(rim, 0.0) / (4.0 * px)) - 0.05, 0.0);
    return out;
}
