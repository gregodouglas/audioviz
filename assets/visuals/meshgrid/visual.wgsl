// Mesh grid: the spectrum's history as a 3D wireframe. The newest row is
// the spectrum now (64 log slices, measured as Spectrum bars measures them);
// every 1 / Speed seconds it is copied into a row slot and the rows roll
// away, so the music leaves a landscape behind it. Laid out as a plane, as
// rings round the middle, or as rings each turned a little further round,
// so the columns wind into spiral arms; beats drop ripples in
// the middle that run out across it; slow rolling hills underneath keep it
// moving in quiet passages.
//
// Items, every one working out the same clock from last frame's:
//   0                 (rows travelled, rows at the last birth, births, view turn)
//   1                 (last beat pulse, loudest slice, bass swell, 1 once started)
//   2                 (next ripple slot, 0, 0, 0)
//   RIPPLES0 + r      ripple r: (birth time, strength, 0, 0)
//   COLS0 + g         the newest row's slices 4g .. 4g+3, live
//   ROWS0 + 16s + g   row slot s's slices 4g .. 4g+3, as born
// A row of age a >= 1 is in slot (births - a) % MAX_ROWS; age 0 is the live
// row. Row ages sit at 0 (live) and a - 1 + frac, frac the clock's fraction
// of a row, so a new row leaves the live one and slides into place.
//
// Triangles: two copies (the reflection, then the mesh), each MAX_ROWS - 1
// bands of up to SEGS quads, the oldest band first, so on the plane the
// near ridges cover the far ones.

const MAX_ROWS: u32 = 128u;
const COLS: u32 = 64u;
const SEGS: u32 = 126u;
const QUADS: u32 = 16128u;       // MAX_ROWS * SEGS
const RIPPLES0: u32 = 3u;
const COLS0: u32 = 7u;
const ROWS0: u32 = 23u;
const FOCAL: f32 = 1.6;
const PLANE_LEN: f32 = 7.0;
const RING_IN: f32 = 0.25;
const RING_OUT: f32 = 2.4;

fn row_count() -> u32 {
    return clamp(u32(params().rows), 16u, MAX_ROWS);
}

fn mirrored() -> bool {
    return params().mirror > 0.5;
}

fn seg_count() -> u32 {
    return select(COLS - 1u, SEGS, mirrored());
}

// A slice's level with the highs lifted (0.056 is 4.5 dB on the
// spectrum's 80 dB scale).
fn tilted(r: vec2<f32>) -> f32 {
    let lift = params().lift * 0.056 * log2(max(sqrt(r.x * r.y), 20.0) / 1000.0);
    return spectrum_peak(r.x, r.y) + lift;
}

// The frequencies slice c of n covers: 35 Hz to 16 kHz, log.
fn slice_range(c: u32, n: u32) -> vec2<f32> {
    let lo = 35.0;
    let hi = 16000.0;
    return vec2<f32>(lo * pow(hi / lo, f32(c) / f32(n)), lo * pow(hi / lo, f32(c + 1u) / f32(n)));
}

struct Clock {
    d: f32,
    last: f32,
    births: f32,
    born: u32,     // the slot a row is born into this frame, or MAX_ROWS
    turn: f32,
};

fn clock() -> Clock {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let c0 = item(0u);
    let pace = 0.6 + 0.4 * clamp(V.energy, 0.0, 2.0);
    var k: Clock;
    k.d = c0.x + P.speed * pace * dt;
    k.last = c0.y;
    k.births = c0.z;
    k.born = MAX_ROWS;
    if (k.d - k.last >= 1.0) {
        k.born = u32(k.births) % MAX_ROWS;
        k.births = (k.births + 1.0) % 65536.0;
        k.last = max(k.last + 1.0, k.d - 1.0);
    }
    if (k.last > 1000.0) {
        k.d = k.d - 1000.0;
        k.last = k.last - 1000.0;
    }
    k.turn = (c0.w + P.turn * 0.15 * pace * dt) % TAU;
    return k;
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let k = clock();
    let c1 = item(1u);
    let struck = V.beat > c1.x + 0.4 && P.ripples > 0.0;
    if (i == 0u) {
        return vec4<f32>(k.d, k.last, k.births, k.turn);
    }
    if (i == 1u) {
        // The loudest of 32 slices, followed quickly up and over seconds down.
        var m = 0.0;
        for (var s = 0u; s < 32u; s++) {
            m = max(m, tilted(slice_range(s, 32u)));
        }
        let loudest = ease(c1.y, max(m, 0.2), 0.05, 3.0);
        let want = 1.0 + P.swell * 0.4 * clamp(broad_rel(BASS) - 1.0, -0.5, 1.5);
        let swell = ease(select(c1.z, 1.0, c1.w < 0.5), want, 0.08, 0.4);
        return vec4<f32>(V.beat, loudest, swell, 1.0);
    }
    let slot = u32(item(2u).x);
    if (i == 2u) {
        return vec4<f32>(f32((slot + select(0u, 1u, struck)) % 4u), 0.0, 0.0, 0.0);
    }
    if (i < COLS0) {
        if (struck && i - RIPPLES0 == slot) {
            let first = beat_in_bar() < 0.5 && bar_sure() > 0.4;
            let strength = select(1.0, 1.4, first) * (0.6 + 0.4 * min(V.beat, 1.0));
            return vec4<f32>(V.time, P.ripples * strength, 0.0, 0.0);
        }
        return item(i);
    }
    if (i < ROWS0) {
        // Four live slices against the loudest: up over 40 ms, down at Fall.
        let g = i - COLS0;
        let range = max(P.range, 6.0) / 80.0;
        let top = max(c1.y, 0.2);
        let last = item(i);
        var lv = vec4<f32>(0.0);
        for (var j = 0u; j < 4u; j++) {
            let want = clamp((tilted(slice_range(4u * g + j, COLS)) - (top - range)) / range * P.gain, 0.0, 1.0);
            lv[j] = max(ease(last[j], want, 0.04, 0.04), last[j] - P.fall * dt);
        }
        return lv;
    }
    let s = (i - ROWS0) / 16u;
    let g = (i - ROWS0) % 16u;
    if (c1.w < 0.5) {
        return vec4<f32>(0.0);
    }
    if (s == k.born) {
        // Born smoothed across neighbouring slices (1 2 1).
        let mid = item(COLS0 + g);
        let lo = item(COLS0 + max(g, 1u) - 1u);
        let hi = item(COLS0 + min(g + 1u, 15u));
        let left = vec4<f32>(select(lo.w, mid.x, g == 0u), mid.xyz);
        let right = vec4<f32>(mid.yzw, select(hi.x, mid.w, g == 15u));
        return 0.25 * left + 0.5 * mid + 0.25 * right;
    }
    return item(i);
}

// Row age a's slice level at t 0..1 (low to high), interpolated.
fn row_level(a: u32, t: f32) -> f32 {
    let births = u32(item(0u).z);
    var base = COLS0;
    if (a > 0u) {
        if (a > births) {
            return 0.0;
        }
        base = ROWS0 + ((births - a) % MAX_ROWS) * 16u;
    }
    let x = clamp(t, 0.0, 1.0) * f32(COLS - 1u);
    let c0 = u32(floor(x));
    let c1 = min(c0 + 1u, COLS - 1u);
    let v0 = item(base + c0 / 4u)[c0 % 4u];
    let v1 = item(base + c1 / 4u)[c1 % 4u];
    return mix(v0, v1, fract(x));
}

// Where row age a sits, 0 (newest) to row_count() - 1 (leaving).
fn row_place(a: u32) -> f32 {
    if (a == 0u) {
        return 0.0;
    }
    let c = item(0u);
    return f32(a) - 1.0 + clamp(c.x - c.y, 0.0, 1.0);
}

// A point on the ground (y = 0) for place q 0..1 (newest to oldest along
// the history) and across s -1..1.
fn ground(q: f32, s: f32) -> vec3<f32> {
    let P = params();
    var along = q;
    if (P.direction > 0.5) {
        along = 1.0 - q;
    }
    if (P.shape < 0.5) {
        let half_w = 1.5 + 3.0 * P.width;
        return vec3<f32>(s * half_w, 0.0, along * PLANE_LEN);
    }
    // Ring, and Spiral: the ring with each row turned a little further
    // round than the one inside it, so its columns wind into arms.
    let r = mix(RING_IN, RING_OUT, along);
    var a = select(s * 0.5, s * 0.5 + 0.5, mirrored()) * TAU + 0.5 * TAU;
    if (P.shape > 1.5) {
        a = a + TAU * (0.3 + 1.2 * P.width) * along;
    }
    return vec3<f32>(r * sin(a), 0.0, r * cos(a));
}

// The beat ripples' lift at a ground point.
fn ripple_lift(p: vec3<f32>) -> f32 {
    let P = params();
    var centre = vec2<f32>(0.0);
    var speed = 1.2;
    if (P.shape < 0.5) {
        centre = vec2<f32>(0.0, 0.3 * PLANE_LEN);
        speed = 3.0;
    }
    let d = length(p.xz - centre);
    var h = 0.0;
    for (var r = 0u; r < 4u; r++) {
        let ring = item(RIPPLES0 + r);
        let age = V.time - ring.x;
        if (ring.y > 0.0 && age >= 0.0 && age < 4.0) {
            let front = d - age * speed;
            h += ring.y * sin(front * 9.0) * exp(-front * front * 4.0) * exp(-age * 0.9);
        }
    }
    return h;
}

struct Eye {
    pos: vec3<f32>,
    right: vec3<f32>,
    up: vec3<f32>,
    fwd: vec3<f32>,
};

fn eye() -> Eye {
    let P = params();
    var aim = vec3<f32>(0.0, 0.0, 0.0);
    var dist = 4.4;
    var yaw = item(0u).w;
    let pitch = mix(0.2, 1.5, clamp(P.view, 0.0, 1.0));
    if (P.shape < 0.5) {
        aim = vec3<f32>(0.0, 0.0, 0.42 * PLANE_LEN);
        dist = 0.68 * PLANE_LEN;
        yaw = 0.18 * P.turn * sin(item(0u).w * 3.0);
    }
    var e: Eye;
    e.pos = aim + dist * vec3<f32>(sin(yaw) * cos(pitch), sin(pitch), -cos(yaw) * cos(pitch));
    e.fwd = normalize(aim - e.pos);
    e.right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), e.fwd));
    e.up = cross(e.fwd, e.right);
    return e;
}

fn vertex(i: u32) -> MeshVertex {
    let P = params();
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    v.colour = vec4<f32>(0.0);
    v.data = vec4<f32>(0.0);
    let quad = i / 6u;
    let corner = i % 6u;
    let copy = quad / QUADS;
    let qq = quad % QUADS;
    let strip_i = qq / SEGS;
    let j = qq % SEGS;
    let n = row_count();
    let segs = seg_count();
    if (strip_i >= n - 1u || j >= segs || (copy == 0u && P.reflection <= 0.0)) {
        return v;
    }
    var dx = 0u;
    var da = 0u;
    switch (corner) {
        case 1u: { dx = 1u; }
        case 2u: { da = 1u; }
        case 3u: { da = 1u; }
        case 4u: { dx = 1u; }
        case 5u: { dx = 1u; da = 1u; }
        default: {}
    }
    // The oldest band first.
    let a = (n - 2u - strip_i) + da;
    let col = j + dx;
    let s = f32(col) / f32(segs) * 2.0 - 1.0;
    let t = select((s + 1.0) * 0.5, abs(s), mirrored());
    let place = row_place(a);
    let q = place / f32(n - 1u);
    let level = row_level(a, t);
    var p = ground(q, s);
    // Rolling hills tied to the row, so they ride with it.
    let c = item(0u);
    let row_id = c.z - 1.0 + clamp(c.x - c.y, 0.0, 1.0) - place;
    let hills = (noise(vec2<f32>(s * 3.0 + 11.0, row_id * 0.12)) - 0.4) * P.terrain * 0.6;
    // The ring is smaller than the plane.
    let tall = P.height * item(1u).z * select(0.6, 1.0, P.shape < 0.5);
    p.y = (pow(level, 1.8) + hills + 0.25 * ripple_lift(p)) * tall;
    if (copy == 0u) {
        p.y = -p.y;
    }
    let e = eye();
    let rel = p - e.pos;
    let vz = max(dot(rel, e.fwd), 0.02);
    v.pos = vec4<f32>(dot(rel, e.right) * FOCAL, dot(rel, e.up) * FOCAL, 0.0, vz);
    var col_c = colour(0.1 + 0.85 * level);
    if (P.colouring > 1.5) {
        col_c = colour(fract(row_id * 0.01));
    } else if (P.colouring > 0.5) {
        col_c = colour(t);
    }
    let fog = exp(-P.fade * (vz - 2.0) * 0.2);
    // The leaving row fades out, the live row is full.
    let leaving = clamp(f32(n - 1u) - place, 0.0, 1.0);
    var bright = min(fog, 1.5) * leaving * (0.55 + 0.9 * level);
    if (copy == 0u) {
        bright = bright * P.reflection * 0.4;
    }
    v.colour = vec4<f32>(col_c * bright, 0.0);
    // For paint: the grid coordinates (rows and columns at integers), and
    // whether this copy covers.
    let covers = copy == 1u && P.solid > 0.5 && P.shape < 0.5;
    v.data = vec4<f32>(f32(col), f32(a), select(0.0, 1.0, covers), 0.0);
    return v;
}

// Thin lines along the grid, or dots where they cross, a pixel-sized width
// whatever the distance; lines too close to tell apart fade out.
fn paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    let g = f.data.xy;
    let fg = max(fwidth(g), vec2<f32>(1e-5));
    let w = max(P.linewidth, 0.3);
    let gl = abs(fract(g + 0.5) - 0.5) / fg;
    let dense = clamp(vec2<f32>(1.6) - fg * 3.0, vec2<f32>(0.0), vec2<f32>(1.0));
    let line = (vec2<f32>(1.0) - smoothstep(vec2<f32>(w * 0.5 - 0.5), vec2<f32>(w * 0.5 + 0.5), gl)) * dense;
    var cov = max(line.x, line.y);
    if (P.wires > 1.5) {
        cov = (1.0 - smoothstep(w - 0.2, w + 0.9, length(gl))) * min(dense.x, dense.y);
    } else if (P.wires > 0.5) {
        cov = line.y;
    }
    return vec4<f32>(f.colour.rgb * cov, f.data.z);
}
