// Jellyfish: a small swarm of glowing jellies. Each one squeezes its bell to
// swim (on the beat, on bass hits, or by its own rhythm), glides and sinks,
// wanders while keeping upright, and turns back from the edges; a drop sets
// them all off at once. Tentacles are chains that follow the bell, each
// segment trailing its parent by a frame, so waves run down them and they
// drag when the jelly swims. Bells are drawn per pixel on a quad each
// (outline glow, translucent fill, radial canals, a ring of gonads, rim
// lights); tentacles are the line; plankton drifts in the background.
//
// Prepare items:
//   0                       (1 once started, 0, 0, 0)
//   1 + 3j, +1, +2          jelly j: (x, y, vx, vy), (heading, seconds since
//                           its last squeeze, squeeze 0..1, last beat count),
//                           (depth 0..1, size, palette position, frames alive)
//   TENT0 + (j * MAX_T + t) * SEGS + s
//                           tentacle t of jelly j, segment s: (x, y, last x, last y)
// Line point i is tentacle segment TENT0 + i; triangles 2j, 2j + 1 are jelly j's quad.

const MAX_J: u32 = 12u;
const MAX_T: u32 = 8u;
const SEGS: u32 = 20u;
const JELLY0: u32 = 1u;
const TENT0: u32 = 37u;     // 1 + 3 * MAX_J
// A bell's half-width at size 1 and depth 1, in centred units.
const BELL: f32 = 0.11;

fn jelly_count() -> u32 {
    return clamp(u32(params().count), 1u, MAX_J);
}

fn tentacle_count() -> u32 {
    return clamp(u32(params().tentacles), 1u, MAX_T);
}

// The bell's "up" for a heading (radians, counter-clockwise from straight up).
fn up_of(a: f32) -> vec2<f32> {
    return vec2<f32>(-sin(a), cos(a));
}

fn wrap_angle(a: f32) -> f32 {
    return a - TAU * round(a / TAU);
}

// Far jellies are smaller.
fn depth_scale(z: f32) -> f32 {
    return mix(0.55, 1.15, z);
}

// The squeeze: quick (0.18 s), then a slow release.
fn squeeze(since: f32) -> f32 {
    if (since < 0.18) {
        return smoothstep(0.0, 0.18, since);
    }
    return exp(-(since - 0.18) / 0.45);
}

struct Jelly {
    pos: vec2<f32>,
    vel: vec2<f32>,
    heading: f32,
    since: f32,
    c: f32,
    last_beat: f32,
    z: f32,
    size: f32,
    hue: f32,
    age: f32,
};

// Jelly j as the items hold it (last frame's inside prepare, this frame's elsewhere).
fn jelly_at(j: u32) -> Jelly {
    let a = item(JELLY0 + 3u * j);
    let b = item(JELLY0 + 3u * j + 1u);
    let c = item(JELLY0 + 3u * j + 2u);
    var J: Jelly;
    J.pos = a.xy;
    J.vel = a.zw;
    J.heading = b.x;
    J.since = b.y;
    J.c = b.z;
    J.last_beat = b.w;
    J.z = c.x;
    J.size = c.y;
    J.hue = c.z;
    J.age = c.w;
    return J;
}

// How big jelly J is drawn, as a multiple of BELL.
fn jelly_scale(J: Jelly) -> f32 {
    return depth_scale(J.z) * J.size * params().size;
}

// Half the visible area in centred units, a little inside the edges.
fn arena() -> vec2<f32> {
    let short = min(V.resolution.x, V.resolution.y);
    return V.resolution / short * 0.85;
}

// One frame of jelly j: spawn, squeeze, swim, steer.
fn jelly_step(j: u32) -> Jelly {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    var J = jelly_at(j);
    if (j >= jelly_count()) {
        var none: Jelly;
        return none;
    }
    let h = hash22(vec2<f32>(f32(j) * 7.31 + 1.7, V.instance * 113.0));
    let h2 = hash22(vec2<f32>(f32(j) * 3.17 + 0.3, V.instance * 57.0 + 9.0));
    let ext = arena();
    if (J.age < 0.5) {
        J.pos = (h * 2.0 - 1.0) * ext * 0.8;
        J.vel = vec2<f32>(0.0);
        J.heading = (h2.x - 0.5) * 0.6;
        J.since = 0.5 + 2.0 * h2.y;
        J.c = 0.0;
        J.last_beat = V.beat_count;
        J.z = clamp(0.5 + (hash21(vec2<f32>(f32(j), V.instance * 31.0)) - 0.5) * P.depth, 0.0, 1.0);
        J.size = 0.8 + 0.4 * h2.y;
        J.hue = f32(j) / f32(jelly_count());
        J.age = 1.0;
        return J;
    }
    J.age = min(J.age + 1.0, 10.0);

    // When to squeeze: half of them every beat, half every other beat; on
    // bass hits most of them; by their own rhythm without a beat.
    let period = mix(1.3, 2.4, h.x) / (0.6 + 0.4 * V.energy);
    var go = false;
    if (P.swim_on < 0.5 && V.confidence > 0.3) {
        if (V.beat_count != J.last_beat) {
            let every = select(1.0, 2.0, h.y > 0.55);
            go = ((V.beat_count + f32(j)) % every) < 0.5;
        }
    } else if (P.swim_on < 1.5 && P.swim_on > 0.5) {
        go = V.hits.x > 0.5 && hash21(vec2<f32>(floor(V.time * 30.0), f32(j))) < 0.7;
    } else {
        go = J.since > period;
    }
    // A drop: everyone.
    if (V.onsets.x > 0.85 && V.swell > 1.0) {
        go = true;
    }
    if (go && J.since > 0.35) {
        J.since = 0.0;
    }
    J.last_beat = V.beat_count;
    J.since += dt;
    J.c = squeeze(J.since);

    // Swim along the bell's axis while squeezing; water slows it; it sinks.
    let up = up_of(J.heading);
    let k = jelly_scale(J);
    var acc = vec2<f32>(0.0, -0.05 * P.sink);
    if (J.since < 0.25) {
        acc += up * P.swim * 0.5 * (0.7 + 0.4 * min(V.bass, 2.0) * P.react) * k;
    }
    J.vel = J.vel * exp(-dt * 1.6) + acc * dt;
    J.pos += J.vel * dt;

    // Steer: each jelly heads for its own spot in the tank, which wanders
    // slowly, keeps clear of its neighbours, and rides upright (with a
    // little wobble) once it is near.
    let tw = V.time * 0.035 * max(P.drift, 0.05);
    let spot = clamp((vec2<f32>(noise(vec2<f32>(tw + h.x * 50.0, f32(j) * 3.0)), noise(vec2<f32>(tw + h.y * 50.0, f32(j) * 3.0 + 7.0))) * 2.0 - 1.0) * ext * 1.6, -ext * 0.8, ext * 0.8);
    var toward = spot - J.pos;
    for (var o = 0u; o < jelly_count(); o++) {
        if (o != j) {
            let q = J.pos - item(JELLY0 + 3u * o).xy;
            toward += q / max(dot(q, q), 1e-3) * 0.015 * k;
        }
    }
    let wobble = (noise(vec2<f32>(V.time * 0.3 + h.x * 20.0, f32(j))) - 0.5) * 0.6 * P.drift;
    var want = select(atan2(-toward.x, toward.y), wobble, length(toward) < 0.12);
    let edge = abs(J.pos) / ext;
    if (max(edge.x, edge.y) > 0.9) {
        want = atan2(J.pos.x, -J.pos.y);
    }
    J.heading = wrap_angle(J.heading + wrap_angle(want - J.heading) * (1.0 - exp(-dt * 0.8)));
    J.pos = clamp(J.pos, -ext * 1.1, ext * 1.1);
    return J;
}

// Where tentacle t (of n) leaves jelly J's rim, and whether it is an oral arm.
fn anchor(J: Jelly, t: u32, n: u32) -> vec3<f32> {
    let oral = n >= 4u && (t == n / 2u - 1u || t == n / 2u);
    var x = 0.0;
    if (oral) {
        x = select(-0.12, 0.12, t == n / 2u);
    } else if (n > 1u) {
        x = mix(-0.85, 0.85, f32(t) / f32(n - 1u));
    }
    x *= 1.0 - 0.22 * J.c;
    let y = -0.1 * (1.0 - x * x) * select(1.0, 0.3, oral);
    let p = J.pos + rotate(vec2<f32>(x, y) * BELL * jelly_scale(J), J.heading);
    return vec3<f32>(p, select(0.0, 1.0, oral));
}

fn segment_length(J: Jelly, oral: bool) -> f32 {
    return 0.5 * params().reach * BELL / 0.11 * jelly_scale(J) / f32(SEGS) * select(1.0, 0.45, oral);
}

// One frame of a tentacle segment. It stays where it was (the water holds
// it, so it trails when the jelly moves) while easing toward hanging below
// its parent (the rim, or the segment above as it was last frame), bent by a
// sway wave that runs down the tentacle, and is kept at its length. No
// momentum: a chain with momentum overshoots and winds itself into loops.
fn segment_step(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let idx = i - TENT0;
    let s = idx % SEGS;
    let tt = idx / SEGS;
    let j = tt / MAX_T;
    let t = tt % MAX_T;
    let n = tentacle_count();
    let J = jelly_at(j);
    let a = anchor(J, t, n);
    let oral = a.z > 0.5;
    let L = segment_length(J, oral);
    let down = -up_of(J.heading);
    // Not swimming yet (or just spawned): hang straight down from the rim.
    if (j >= jelly_count() || t >= n || J.age < 1.5) {
        let p = a.xy + down * L * f32(s + 1u);
        return vec4<f32>(p, p);
    }
    let parent = select(item(i - 1u).xy, a.xy, s == 0u);
    let cur = item(i).xy;
    // Hanging: below the bell, a little towards the world's down.
    let hang = normalize(down + vec2<f32>(0.0, -0.5));
    let wave = sin(V.time * 2.2 - f32(s) * 0.4 + f32(t) * 1.3 + f32(j) * 2.1);
    let bend = wave * 0.35 * P.sway * (0.4 + 0.6 * min(V.mid, 2.0) * P.react) * select(1.0, 1.5, oral);
    let rest = rotate(hang, bend);
    let held = normalize(cur - parent + vec2<f32>(1e-6, 0.0));
    let dir = normalize(mix(held, rest, 1.0 - exp(-dt * select(3.0, 5.0, oral))) + vec2<f32>(1e-6, 0.0));
    let p = parent + dir * L;
    return vec4<f32>(p, cur);
}

fn prepare(i: u32) -> vec4<f32> {
    if (i == 0u) {
        return vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }
    if (i < TENT0) {
        let j = (i - JELLY0) / 3u;
        let J = jelly_step(j);
        switch ((i - JELLY0) % 3u) {
            case 0u: { return vec4<f32>(J.pos, J.vel); }
            case 1u: { return vec4<f32>(J.heading, J.since, J.c, J.last_beat); }
            default: { return vec4<f32>(J.z, J.size, J.hue, J.age); }
        }
    }
    return segment_step(i);
}

// How bright jelly J is: far ones dimmer, brighter as it squeezes.
fn brightness(J: Jelly) -> f32 {
    let P = params();
    return mix(0.35, 1.0, J.z) * (0.55 + 1.2 * J.c * P.react) * P.glow;
}

// ---------------------------------------------------------------- tentacles

fn point(i: u32) -> LinePoint {
    let s = i % SEGS;
    let tt = i / SEGS;
    let j = tt / MAX_T;
    let t = tt % MAX_T;
    let n = tentacle_count();
    let J = jelly_at(j);
    var q: LinePoint;
    q.pos = item(TENT0 + i).xy;
    // A tentacle's last point ends its stroke; unused ones draw nothing.
    let on = j < jelly_count() && t < n && J.age > 2.5 && s + 1u < SEGS;
    let oral = anchor(J, t, n).z > 0.5;
    let u = f32(s) / f32(SEGS - 1u);
    let k = sqrt(jelly_scale(J));
    q.width = select(0.0, mix(2.4, 0.5, u) * k * select(1.0, 2.2, oral), on);
    q.colour = colour(J.hue + 0.08 + 0.15 * u) * pow(1.0 - u, 1.2) * brightness(J) * select(0.9, 1.3, oral);
    return q;
}

// ---------------------------------------------------------------- bells

// Each jelly is a quad round its bell, in bell units (half-width 1, height
// 1): x -1.45..1.45, y -0.55..1.45, turned by its heading.
fn vertex(i: u32) -> MeshVertex {
    let j = i / 6u;
    let J = jelly_at(j);
    var v: MeshVertex;
    v.colour = vec4<f32>(0.0);
    if (j >= jelly_count() || J.age < 1.5) {
        v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        return v;
    }
    var cx = array<f32, 6>(0.0, 1.0, 1.0, 0.0, 1.0, 0.0);
    var cy = array<f32, 6>(0.0, 0.0, 1.0, 0.0, 1.0, 1.0);
    let local = vec2<f32>(mix(-1.45, 1.45, cx[i % 6u]), mix(-0.55, 1.45, cy[i % 6u]));
    let p = J.pos + rotate(local * BELL * jelly_scale(J), J.heading);
    v.pos = vec4<f32>(p, 0.0, 1.0);
    v.data = vec4<f32>(local, J.c, f32(j) + 0.5);
    return v;
}

fn paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    let J = jelly_at(u32(f.data.w));
    let q = f.data.xy;
    let c = f.data.z;
    // The bell squeezes narrower and taller.
    let w = 1.0 - 0.22 * c;
    let h = 1.0 + 0.15 * c;
    let e = length(vec2<f32>(q.x / w, max(q.y, 0.0) / h));
    let rim_y = -0.12 * (1.0 - min(q.x * q.x / (w * w), 1.0)) * (1.0 - 0.6 * c);
    let d = max((e - 1.0) * min(w, h), rim_y - q.y);
    let inside = smoothstep(0.02, -0.02, d);
    // Outline glow, a faint translucent fill brighter towards the top.
    let outline = max(exp(-abs(d) / 0.06) - 0.004, 0.0);
    let fill = inside * 0.12 * (0.5 + 0.5 * clamp(q.y / h, 0.0, 1.0));
    // Radial canals from the crown and a ring of gonads.
    let a = atan2(q.x, -(q.y - 0.95 * h));
    let canals = pow(max(cos(a * 8.0), 0.0), 12.0) * inside * 0.3;
    let g = length(vec2<f32>(q.x * 1.15, q.y - 0.42 * h));
    let gonads = max(exp(-abs(g - 0.3) / 0.05) - 0.01, 0.0) * inside * 0.45 * (0.6 + 0.4 * sin(atan2(q.x, q.y - 0.42 * h) * 4.0));
    // Rim lights: dots along the margin, sparkling with the treble.
    let margin = max(exp(-abs(q.y - rim_y) / 0.05) - 0.01, 0.0) * step(abs(q.x), w);
    let dots = pow(max(sin(q.x * 24.0 + V.time * 3.0 + f.data.w * 5.0), 0.0), 6.0) * margin * P.rim * (0.5 + 1.5 * min(V.treble, 2.0) * P.react);
    let b = brightness(J);
    let light = colour(J.hue) * (outline * 0.9 + fill + canals + gonads) + colour(J.hue + 0.2) * dots * 1.5;
    // Fade to nothing at the quad's edges so no seam shows.
    let mask = smoothstep(1.45, 1.15, abs(q.x)) * smoothstep(-0.55, -0.3, q.y) * smoothstep(1.45, 1.2, q.y);
    return vec4<f32>(light * b * mask, 0.0);
}

// ---------------------------------------------------------------- water

// Plankton: two layers of faint specks drifting slowly upward, twinkling.
fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    var c = vec3<f32>(0.0);
    if (P.plankton < 0.001) {
        return c;
    }
    let p = centred(uv);
    for (var l = 0u; l < 2u; l++) {
        let scale = select(9.0, 17.0, l == 1u);
        let q = (p - vec2<f32>(0.0, V.flow * 0.012 * f32(l + 1u))) * scale;
        let cell = floor(q);
        let hh = hash22(cell + f32(l) * 31.0);
        if (hh.x > 0.35) {
            continue;
        }
        let at = cell + 0.2 + 0.6 * hash22(cell + 7.0 + f32(l) * 13.0);
        let d = length(q - at);
        let r = 0.05 + 0.05 * hh.y;
        let twinkle = 0.5 + 0.5 * sin(V.time * (1.0 + 2.0 * hh.y) + hh.x * 40.0);
        c += colour(hh.y) * max(exp(-d * d / (r * r)) - 0.01, 0.0) * P.plankton * 0.15 * twinkle * select(1.0, 0.6, l == 1u);
    }
    return c;
}
