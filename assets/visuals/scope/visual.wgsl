// Phosphor scope: the waveform as an XY oscilloscope trace (left channel
// across, right channel up, turned by Rotation), drawn as a phosphor beam
// that is bright where it lingers and dim where it races, over last frame's
// picture fading by Persistence.
//
// Each frame draws exactly the samples that arrived since the last one
// (V.wave_new of them, newest first in sample()), starting from last
// frame's final sample so the trace is continuous.
//
// With a palette the colour comes from the key and the beam's speed, or,
// with Palette by Frequency, from the pitch around each point: the distance
// between the zero crossings either side of it is half a period, mapped by
// octave over 55 Hz to 7.04 kHz.

const F_LO: f32 = 55.0;
const F_HI: f32 = 7040.0;

// The sample a point stands for, with gain and rotation, in centred units.
fn trace(k: u32) -> vec2<f32> {
    let P = params();
    var s = sample(k);
    if (P.auto_gain < 0.5) {
        s = s / max(V.auto_gain, 1e-6);
    }
    return rotate(s * P.gain, radians(P.rotation_deg));
}

fn mid(k: u32) -> f32 {
    let s = sample(k);
    return s.x + s.y;
}

// The next zero crossing older than sample k (further from 0), as a sample
// index, or k + limit when there is none that close.
fn older_crossing(k: u32, limit: u32) -> u32 {
    let here = mid(k) < 0.0;
    for (var j = 1u; j < limit; j++) {
        if ((mid(k + j) < 0.0) != here) {
            return k + j;
        }
    }
    return k + limit;
}

// The pitch around sample k as a palette position (-1 when no crossing is
// near enough to tell). Each gap between crossings is half a period; like
// the compiled-in scope, the estimate leans on the gaps before it too
// (0.4 this one, then 0.24, 0.36 for the two older), so it settles.
fn pitch_at(k: u32) -> f32 {
    // Half a period of the lowest pitch that has a colour (55 Hz).
    let limit = u32(V.sample_rate / (2.0 * F_LO)) + 1u;
    let here = mid(k) < 0.0;
    var newer = 0u;
    for (var j = 1u; j < limit && j <= k; j++) {
        if ((mid(k - j) < 0.0) != here) { newer = j; break; }
    }
    if (newer == 0u) {
        return -1.0;
    }
    let c0 = older_crossing(k, limit);
    let c1 = older_crossing(c0, limit);
    let c2 = older_crossing(c1, limit);
    if (c0 - k >= limit) {
        return -1.0;
    }
    let f0 = V.sample_rate / (2.0 * f32(newer + c0 - k));
    var f = f0;
    if (c1 - c0 < limit && c2 - c1 < limit) {
        let f1 = V.sample_rate / (2.0 * f32(c1 - c0));
        let f2 = V.sample_rate / (2.0 * f32(c2 - c1));
        f = 0.4 * f0 + 0.24 * f1 + 0.36 * f2;
    }
    return clamp(log2(f / F_LO) / log2(F_HI / F_LO), 0.0, 1.0);
}

// Item 0: the last pitch position seen, held through silence.
fn prepare(i: u32) -> vec4<f32> {
    let held = item(0u).x;
    if (params().colour < 0.5) {
        return vec4<f32>(held, 0.0, 0.0, 0.0);
    }
    let now = pitch_at(0u);
    if (now < 0.0) {
        return vec4<f32>(held, 0.0, 0.0, 0.0);
    }
    return vec4<f32>(mix(held, now, 0.4), 0.0, 0.0, 0.0);
}

fn beam_colour(t: f32) -> vec3<f32> {
    let x = fract(t + V.palette_shift) * 32.0;
    let i = u32(floor(x)) % 32u;
    let pal = mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
    return mix(V.tint.rgb, pal * 1.2, V.palette_mix);
}

// Point i of the trace: i = 0 is last frame's final sample, i = V.wave_new
// the newest. Points past that end the line.
fn point(i: u32) -> LinePoint {
    let P = params();
    let n = u32(V.wave_new);
    var q: LinePoint;
    q.pos = vec2<f32>(0.0);
    q.width = 0.0;
    q.colour = vec3<f32>(0.0);
    if (i > n) {
        return q;
    }
    let k = n - i;
    q.pos = trace(k);
    q.width = P.beam_width;
    var t: f32;
    if (P.colour > 0.5) {
        let f = pitch_at(k);
        t = select(item(0u).x, f, f >= 0.0);
    } else {
        // The key sets the base, fast travel moves along the palette.
        let key_pos = f32((u32(V.key) * 7u) % 12u) / 12.0;
        let speed = clamp(length(trace(k) - trace(k + 1u)) / pixel() / 40.0, 0.0, 1.0);
        t = 0.15 * key_pos + speed * 0.35;
    }
    q.colour = beam_colour(t) * P.beam_intensity * (1.0 + P.beat_pulse * V.beat);
    return q;
}

// The phosphor: last frame fading by Persistence (per second), with a tiny
// floor taken off so the trail reaches black.
fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let decay = pow(max(P.persistence, 1e-6), V.dt);
    return max(prev(uv) * decay - vec3<f32>(0.0005), vec3<f32>(0.0));
}
