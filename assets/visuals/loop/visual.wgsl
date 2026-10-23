// Loop: concentric waveform rings. Every 1/Rate seconds the newest slice of
// the waveform is bent into a closed ring at the centre; older rings sit
// further out, so a sound travels outward ring by ring, each one lagging the
// last. A ring's brightness follows its own loudness, so quiet moments leave
// dark gaps in the ripple. The palette runs by age or by level.
//
// After the Loop in Felix Turner's Uberviz (https://uberviz.io). No code
// used; written fresh from a description of the effect.
//
// The prepare step keeps the ring history (up to 120 rings of 256 points):
//   item 0                  (clock seconds owed, newest ring's slot, turn, beat kick)
//   item 1                  (last beat pulse, the section's colour offset, how sure the beat is (both eased), 0)
//   items 2 + s*64 + g      ring slot s, columns 4g .. 4g+3
//   items 2 + 120*64 + s    ring slot s's level (from last frame's values)
// Every item works out the ring clock from last frame's item 0 the same way,
// so they all agree which slots are written this frame.

const COLS: u32 = 256u;
const MAX_RINGS: u32 = 120u;
const DECIM: u32 = 3u;
const GROUPS: u32 = 64u;           // COLS / 4
const RING0: u32 = 2u;
const LEVEL0: u32 = 7682u;         // RING0 + MAX_RINGS * GROUPS
const PER_RING: u32 = 258u;        // COLS + 1 to close the ring, + 1 to break

fn ring_count() -> u32 {
    return u32(clamp(params().rings, 2.0, f32(MAX_RINGS)));
}

// This frame's ring clock from last frame's: (seconds owed, rings pushed, newest slot).
fn clock() -> vec3<f32> {
    let P = params();
    let rings = ring_count();
    let last = item(0u);
    let period = 1.0 / max(P.rate, 1.0);
    var acc = last.x + V.dt;
    var head = u32(last.y);
    var pushed = 0u;
    while (acc >= period && pushed < 4u) {
        acc -= period;
        head = (head + 1u) % rings;
        pushed++;
    }
    if (acc > period * 4.0) {
        acc = 0.0;
    }
    if (head >= rings) {
        head = 0u;
    }
    return vec3<f32>(acc, f32(pushed), f32(head));
}

// Column c of the newest waveform window, decimated: the window spans
// COLS * DECIM samples, oldest first.
fn raw(c: u32) -> f32 {
    var s = 0.0;
    for (var d = 0u; d < DECIM; d++) {
        let p = c * DECIM + d;
        let w = sample(COLS * DECIM - 1u - p);
        s += 0.5 * (w.x + w.y);
    }
    return s / f32(DECIM) * params().gain;
}

// The ring closed without a seam: two copies half a turn apart under
// sin^2 / cos^2 windows that sum to one.
fn row(c: u32) -> f32 {
    let w = pow(sin(3.14159265 * f32(c) / f32(COLS)), 2.0);
    return raw(c) * w + raw((c + COLS / 2u) % COLS) * (1.0 - w);
}

// Smoothing: k passes of a three-point average round the ring, as one
// kernel (the coefficients of (1 + x + x^2)^k / 3^k), k = 0..8.
var<private> KERNELS: array<array<f32, 17>, 9> = array<array<f32, 17>, 9>(
    array<f32, 17>(0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0),
    array<f32, 17>(0, 0, 0, 0, 0, 0, 0, 0.333333333, 0.333333333, 0.333333333, 0, 0, 0, 0, 0, 0, 0),
    array<f32, 17>(0, 0, 0, 0, 0, 0, 0.111111111, 0.222222222, 0.333333333, 0.222222222, 0.111111111, 0, 0, 0, 0, 0, 0),
    array<f32, 17>(0, 0, 0, 0, 0, 0.037037037, 0.111111111, 0.222222222, 0.259259259, 0.222222222, 0.111111111, 0.037037037, 0, 0, 0, 0, 0),
    array<f32, 17>(0, 0, 0, 0, 0.012345679, 0.049382716, 0.12345679, 0.197530864, 0.234567901, 0.197530864, 0.12345679, 0.049382716, 0.012345679, 0, 0, 0, 0),
    array<f32, 17>(0, 0, 0, 0.00411522634, 0.0205761317, 0.0617283951, 0.12345679, 0.185185185, 0.209876543, 0.185185185, 0.12345679, 0.0617283951, 0.0205761317, 0.00411522634, 0, 0, 0),
    array<f32, 17>(0, 0, 0.00137174211, 0.00823045267, 0.0288065844, 0.0685871056, 0.12345679, 0.172839506, 0.193415638, 0.172839506, 0.12345679, 0.0685871056, 0.0288065844, 0.00823045267, 0.00137174211, 0, 0),
    array<f32, 17>(0, 0.000457247371, 0.0032007316, 0.0128029264, 0.0352080476, 0.0736168267, 0.121627801, 0.163237311, 0.179698217, 0.163237311, 0.121627801, 0.0736168267, 0.0352080476, 0.0128029264, 0.0032007316, 0.000457247371, 0),
    array<f32, 17>(0, 0.00121932632, 0.00548696845, 0.0170705685, 0.0405426002, 0.0768175583, 0.11949398, 0.154854443, 0.16872428, 0.154854443, 0.11949398, 0.0768175583, 0.0405426002, 0.0170705685, 0.00548696845, 0.00121932632, 0)
);

fn smoothed(c: u32) -> f32 {
    let k = u32(clamp(params().smooth_, 0.0, 8.0));
    var s = 0.0;
    for (var m = 8u - k; m <= 8u + k; m++) {
        s += KERNELS[k][m] * row((c + COLS + m - 8u) % COLS);
    }
    return s;
}

// Whether slot s is written this frame: the `pushed` slots up to the new head.
fn written(s: u32, ck: vec3<f32>) -> bool {
    let rings = ring_count();
    let pushed = u32(ck.y);
    let head = u32(ck.z);
    let back = (head + rings - s) % rings;
    return s < rings && back < pushed;
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let ck = clock();
    if (i == 0u) {
        let last = item(0u);
        let turn = last.z + V.dt * P.spin * pace();
        // Beats and drops swell the rings, eased in over 40 ms (a drop's
        // pulse jumps), as much as the layer's Motion says.
        let swell = ease(last.w, max(kick(), 1.5 * drop_pulse()) * motion(), 0.04, 0.25);
        return vec4<f32>(ck.x, ck.z, turn - TAU * floor(turn / TAU), swell);
    }
    if (i == 1u) {
        let s = item(1u);
        return vec4<f32>(V.beat, ease(s.y, section_seed(6.0) * 0.5 * P.sections, 3.0, 3.0), ease(s.z, V.confidence, 0.5, 1.0), 0.0);
    }
    if (i < LEVEL0) {
        let s = (i - RING0) / GROUPS;
        if (!written(s, ck)) {
            return item(i);
        }
        let c = ((i - RING0) % GROUPS) * 4u;
        return vec4<f32>(smoothed(c), smoothed(c + 1u), smoothed(c + 2u), smoothed(c + 3u));
    }
    // A ring's level: the RMS of its values as stored last frame (a ring
    // written this frame gets its level one frame later).
    let s = i - LEVEL0;
    if (s >= MAX_RINGS) {
        return vec4<f32>(0.0);
    }
    var sum = 0.0;
    for (var g = 0u; g < GROUPS; g++) {
        let v = item(RING0 + s * GROUPS + g);
        sum += dot(v, v);
    }
    return vec4<f32>(min(sqrt(sum / f32(COLS)) * 3.0, 1.0), 0.0, 0.0, 0.0);
}

fn ring_value(slot: u32, c: u32) -> f32 {
    let v = item(RING0 + slot * GROUPS + (c % COLS) / 4u);
    return v[c % 4u];
}

fn ring_colour(t: f32) -> vec3<f32> {
    let x = fract(t + V.palette_shift + item(1u).y) * 32.0;
    let i = u32(floor(x)) % 32u;
    let pal = mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
    return mix(V.tint.rgb, pal * 1.2, V.palette_mix);
}

// The line: ring by ring, newest first, each closed back on its first point
// and then broken before the next.
fn point(i: u32) -> LinePoint {
    let P = params();
    let rings = ring_count();
    let age = i / PER_RING;
    let j = i % PER_RING;
    var q: LinePoint;
    q.pos = vec2<f32>(0.0);
    q.width = 0.0;
    q.colour = vec3<f32>(0.0);
    if (age >= rings || j == PER_RING - 1u) {
        return q;
    }
    let state = item(0u);
    let head = u32(state.y);
    let slot = (head + rings - age) % rings;
    let t = f32(age) / f32(max(rings - 1u, 1u));
    let r = mix(P.inner, max(P.outer, P.inner + 0.01), t);
    let c = j % COLS;
    let sway = P.sway * 0.12 * sin(TAU * bar_phase()) * item(1u).z * motion();
    let turn = state.z + radians(P.twist) * f32(age) + sway;
    let a = TAU * f32(c) / f32(COLS) + turn;
    let rr = max(r + ring_value(slot, c) * P.amp * (1.0 + P.react * state.w), 0.0);
    q.pos = vec2<f32>(cos(a), -sin(a)) * rr;
    q.width = P.width;
    // Brightness: the ring's own loudness, dimmed with age.
    let lvl = clamp(item(LEVEL0 + slot).x, 0.0, 1.0);
    let bright = mix(1.0, 0.15 + 1.6 * lvl, P.level) * mix(1.0, 1.0 - P.fade, t);
    q.colour = ring_colour(select(t, lvl, P.colour > 0.5)) * bright;
    return q;
}
