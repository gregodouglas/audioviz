// Waves: rows of the waveform (or the spectrum) receding in perspective,
// the Unknown Pleasures landscape. A new row is cut every 1/Rate seconds at
// the front; older rows march toward the horizon, smaller and dimmer. Each
// row is drawn as a black fill down to its baseline, then its line, far rows
// first, so every row hides what is behind it.
//
// The prepare step keeps the rows (up to 160 of 256 points):
//   item 0                 (clock seconds owed, newest row's slot, 0, 0)
//   items 2 + s*64 + g     row slot s, columns 4g .. 4g+3
//   items 2 + 160*64 + s   row slot s's level (from last frame's values)
// Every item works out the row clock from last frame's item 0 the same way.

const COLS: u32 = 256u;
const SEGS: u32 = 255u;
const MAX_ROWS: u32 = 160u;
const DECIM: u32 = 4u;
const GROUPS: u32 = 64u;
const ROW0: u32 = 2u;
const LEVEL0: u32 = 10242u;       // ROW0 + MAX_ROWS * GROUPS

fn row_count() -> u32 {
    return u32(clamp(params().rows, 2.0, f32(MAX_ROWS)));
}

// This frame's row clock from last frame's: (seconds owed, rows pushed, newest slot).
fn clock() -> vec3<f32> {
    let P = params();
    let rows = row_count();
    let last = item(0u);
    let period = 1.0 / max(P.rate, 1.0);
    var acc = last.x + V.dt;
    var head = u32(last.y);
    var pushed = 0u;
    while (acc >= period && pushed < 4u) {
        acc -= period;
        head = (head + 1u) % rows;
        pushed++;
    }
    if (acc > period * 4.0) {
        acc = 0.0;
    }
    if (head >= rows) {
        head = 0u;
    }
    return vec3<f32>(acc, f32(pushed), f32(head));
}

fn written(s: u32, ck: vec3<f32>) -> bool {
    let rows = row_count();
    let back = (head_of(ck) + rows - s) % rows;
    return s < rows && back < u32(ck.y);
}

fn head_of(ck: vec3<f32>) -> u32 {
    return u32(ck.z);
}

// Column c of a new row, before smoothing.
fn raw(c: u32) -> f32 {
    let P = params();
    if (P.source > 0.5) {
        // The spectrum, square-rooted so quiet bands still show.
        let x = f32(c) / f32(COLS - 1u) * 31.0;
        let k = u32(floor(x));
        let v = mix(band_raw(k), band_raw(min(k + 1u, 31u)), fract(x));
        return min(sqrt(max(v, 0.0)) * P.gain, 2.0);
    }
    // The newest COLS * DECIM samples, oldest first, DECIM to a column.
    var s = 0.0;
    for (var d = 0u; d < DECIM; d++) {
        let w = sample(COLS * DECIM - 1u - (c * DECIM + d));
        s += 0.5 * (w.x + w.y);
    }
    return s / f32(DECIM) * P.gain;
}

// k passes of a three-point average with the end columns held, as the
// compiled-in Waves did: worked in a window round c that shrinks by one
// each pass, so the centre is exact.
fn smoothed(c: u32) -> f32 {
    let k = i32(clamp(params().smooth_, 0.0, 8.0));
    var w: array<f32, 17>;
    for (var m = -k; m <= k; m++) {
        let j = clamp(i32(c) + m, 0, i32(COLS) - 1);
        w[m + 8] = raw(u32(j));
    }
    for (var pass_ = 1; pass_ <= k; pass_++) {
        var next = w;
        for (var m = -k + pass_; m <= k - pass_; m++) {
            let j = i32(c) + m;
            if (j > 0 && j < i32(COLS) - 1) {
                next[m + 8] = (w[m + 7] + w[m + 8] + w[m + 9]) / 3.0;
            }
        }
        w = next;
    }
    return w[8];
}

fn prepare(i: u32) -> vec4<f32> {
    let ck = clock();
    if (i == 0u) {
        return vec4<f32>(ck.x, ck.z, 0.0, 0.0);
    }
    if (i == 1u) {
        return vec4<f32>(0.0);
    }
    if (i < LEVEL0) {
        let s = (i - ROW0) / GROUPS;
        if (!written(s, ck)) {
            return item(i);
        }
        let c = ((i - ROW0) % GROUPS) * 4u;
        return vec4<f32>(smoothed(c), smoothed(c + 1u), smoothed(c + 2u), smoothed(c + 3u));
    }
    let s = i - LEVEL0;
    if (s >= MAX_ROWS) {
        return vec4<f32>(0.0);
    }
    var sum = 0.0;
    for (var g = 0u; g < GROUPS; g++) {
        let v = item(ROW0 + s * GROUPS + g);
        sum += dot(v, v);
    }
    return vec4<f32>(min(sqrt(sum / f32(COLS)) * 2.5, 1.0), 0.0, 0.0, 0.0);
}

fn row_value(slot: u32, c: u32) -> f32 {
    let v = item(ROW0 + slot * GROUPS + c / 4u);
    return v[c % 4u];
}

fn row_colour(t: f32) -> vec3<f32> {
    let x = fract(t + V.palette_shift) * 32.0;
    let i = u32(floor(x)) % 32u;
    let pal = mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
    return mix(V.tint.rgb, pal * 1.2, V.palette_mix);
}

// Clip-space (NDC) to centred units.
fn from_ndc(p: vec2<f32>) -> vec2<f32> {
    let short = min(V.resolution.x, V.resolution.y);
    return p * V.resolution / short;
}

// Corner i. Quads of six corners: per row (far first), its 255 fill quads,
// then its 255 line quads. data = (along, across, segment length, 1 for line)
// in pixels.
fn vertex(i: u32) -> MeshVertex {
    let P = params();
    let rows = row_count();
    let vi = i % 6u;
    let ii = i / 6u;
    let per_row = SEGS * 2u;
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    v.colour = vec4<f32>(0.0);
    v.data = vec4<f32>(0.0);
    let row = ii / per_row;                 // 0 = farthest, drawn first
    if (row >= rows) {
        return v;
    }
    let within = ii % per_row;
    let part = within / SEGS;               // 0 fill, 1 line
    let seg = within % SEGS;
    let age = rows - 1u - row;              // 0 = newest
    let t = f32(age) / f32(max(rows - 1u, 1u));
    let k = max(1.0 - P.perspective_, 0.03);
    let tp = t * (1.0 + k) / (t + k);
    let far_scale = mix(1.0, 0.22, P.perspective_);
    let scale = mix(1.0, far_scale, tp);
    let base_y = mix(P.near, P.horizon, tp) * 2.0 - 1.0;
    let slot = (head_of(vec3<f32>(0.0, 0.0, item(0u).y)) + rows - age) % rows;
    let x0 = (-1.0 + 2.0 * f32(seg) / f32(SEGS)) * P.spread * scale;
    let x1 = (-1.0 + 2.0 * f32(seg + 1u) / f32(SEGS)) * P.spread * scale;
    let amp = P.height * scale;
    let a = vec2<f32>(x0, base_y + row_value(slot, seg) * amp);
    let b = vec2<f32>(x1, base_y + row_value(slot, seg + 1u) * amp);
    if (part == 0u) {
        // The fill: opaque black down to the baseline.
        var p: vec2<f32>;
        switch (vi) {
            case 0u: { p = a; }
            case 1u: { p = b; }
            case 2u: { p = vec2<f32>(x0, base_y); }
            case 3u: { p = vec2<f32>(x0, base_y); }
            case 4u: { p = b; }
            default: { p = vec2<f32>(x1, base_y); }
        }
        v.pos = vec4<f32>(from_ndc(p), 0.0, 1.0);
        v.colour = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        return v;
    }
    // The line: a pixel-space quad round the segment.
    let lvl = item(LEVEL0 + slot).x;
    let bright = mix(1.0, 1.0 - P.fade, tp);
    let colour = row_colour(select(t, clamp(lvl, 0.0, 1.0), P.colour > 0.5)) * bright;
    let res = V.resolution;
    let pa = vec2<f32>((a.x + 1.0) * 0.5 * res.x, (1.0 - a.y) * 0.5 * res.y);
    let pb = vec2<f32>((b.x + 1.0) * 0.5 * res.x, (1.0 - b.y) * 0.5 * res.y);
    var d = pb - pa;
    let len = length(d);
    if (len < 1e-4) { d = vec2<f32>(1.0, 0.0); } else { d = d / len; }
    let n = vec2<f32>(-d.y, d.x);
    let w = P.width * 0.5 + 1.5;
    var along: f32;
    var across: f32;
    switch (vi) {
        case 0u: { along = -w;       across = -w; }
        case 1u: { along = len + w;  across = -w; }
        case 2u: { along = -w;       across =  w; }
        case 3u: { along = -w;       across =  w; }
        case 4u: { along = len + w;  across = -w; }
        default: { along = len + w;  across =  w; }
    }
    let px = pa + d * along + n * across;
    let ndc = vec2<f32>(px.x / res.x * 2.0 - 1.0, 1.0 - px.y / res.y * 2.0);
    v.pos = vec4<f32>(from_ndc(ndc), 0.0, 1.0);
    v.colour = vec4<f32>(colour, 1.0);
    v.data = vec4<f32>(along, across, len, 1.0);
    return v;
}

fn paint(f: MeshFragment) -> vec4<f32> {
    if (f.data.w < 0.5) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let along = clamp(f.data.x, 0.0, f.data.z);
    let dist = length(vec2<f32>(f.data.x - along, f.data.y));
    let half = max(params().width, 0.5) * 0.5;
    let cov = 1.0 - smoothstep(half - 0.5, half + 0.8, dist);
    return vec4<f32>(f.colour.rgb * cov, cov);
}
