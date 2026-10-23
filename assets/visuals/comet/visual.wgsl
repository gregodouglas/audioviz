// Comet: heads on looping paths, each a white-hot nucleus in a coloured
// coma, leaving tails that widen, soften and fade as they age, and shedding
// sparks. Example 3 of docs/building-visuals.md: a picture kept from frame
// to frame. With "trail": true in visual.json, trail(uv) is last frame's
// tails and write_trail(c) keeps this frame's; reading them a little moved,
// blurred and faded, then adding the heads' new stretch, gives tails that
// last for seconds. The heads, their glow and the sparks are drawn fresh
// each frame on top and never kept, so nothing else smears. The coma swells
// on each beat (more on the bar's first), the paths widen through a build,
// and each section of the song has its own colours.
//
// Items:
//   0          (the section's palette offset eased, the path clock, 0, last beat pulse)
//   1 + h      head h: (where it is, where it was last frame), so its tail is drawn exactly
//   FLARE      (the beat's flare, which jumps and dies away; it eased, which the coma follows)
//   SPARK0 + 2s, + 1   spark s: (position, velocity), (age, life, its head, 0)
// Triangles: a quad per spark.

const MAX_HEADS: u32 = 6u;
const SPARKS_PER_HEAD: u32 = 32u;
const FLARE: u32 = 7u;
const SPARK0: u32 = 8u;

fn head_count() -> u32 {
    return clamp(u32(params().heads), 1u, MAX_HEADS);
}

fn rand(k: u32, salt: f32) -> f32 {
    var n = k * 747796405u + bitcast<u32>(salt) * 2891336453u + 1u;
    n = ((n >> ((n >> 28u) + 4u)) ^ n) * 277803737u;
    n = (n >> 22u) ^ n;
    return f32(n & 0xffffffu) / 16777215.0;
}

// Where head h is at clock s: a Lissajous figure (circle, eight or knot), or
// its own wandering loop (two sines a side, each head's own rates).
fn path(s: f32, h: u32, radius: f32) -> vec2<f32> {
    let P = params();
    let heads = f32(head_count());
    let fh = f32(h);
    if (P.figure > 2.5) {
        let r = vec4<f32>(rand(h, 1.0), rand(h, 2.0), rand(h, 3.0), rand(h, 4.0));
        let a = s * TAU;
        let x = 0.65 * sin(a * (0.7 + 0.5 * r.x) + fh * 2.1) + 0.35 * sin(a * (1.9 + 0.8 * r.y) + fh * 1.3);
        let y = 0.65 * cos(a * (0.6 + 0.5 * r.z) + fh * 2.7) + 0.35 * sin(a * (1.7 + 0.8 * r.w) + fh * 0.7);
        return vec2<f32>(x * 1.35, y) * radius;
    }
    var f = vec2<f32>(1.0, 1.0);
    if (P.figure > 1.5) {
        f = vec2<f32>(3.0, 2.0);
    } else if (P.figure > 0.5) {
        f = vec2<f32>(1.0, 2.0);
    }
    let a = s * TAU + fh / heads * TAU;
    return vec2<f32>(sin(f.x * a + 1.5707963), sin(f.y * a)) * radius;
}

fn radius() -> f32 {
    let P = params();
    return P.radius * (1.0 + 0.15 * V.swell + 0.25 * P.opens * build());
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let c0 = item(0u);
    let s = c0.y + dt * P.speed * 0.25 * (0.5 + 0.5 * V.energy);
    let struck = V.beat > c0.w + 0.4;
    if (i == 0u) {
        let section = ease(c0.x, section_seed(3.0) * 0.5 * P.sections, 2.5, 2.5);
        return vec4<f32>(section, s % 4096.0, 0.0, V.beat);
    }
    if (i == FLARE) {
        // Each beat flares the comas, the bar's first more; the coma
        // follows it eased, so it swells over a few frames.
        let f = item(FLARE);
        var flare = f.x * exp(-dt / 0.3);
        if (struck) {
            flare = max(flare, 0.5 + P.flare * first_beat());
        }
        return vec4<f32>(flare, ease(f.y, flare, 0.05, 0.3), 0.0, 0.0);
    }
    if (i < FLARE) {
        let h = i - 1u;
        let last = item(i);
        let now = path(s, h, radius());
        // At the start, last frame's place is this one's.
        let was = select(last.xy, now, all(last.xy == vec2<f32>(0.0)));
        return vec4<f32>(now, was);
    }
    // Sparks: each belongs to a head and is reborn there when its life is
    // over, drifting off the path and slowing; a first beat shortens every
    // life so a burst comes off at once.
    let k = (i - SPARK0) / 2u;
    let h = k / SPARKS_PER_HEAD;
    let a = item(SPARK0 + 2u * k);
    let b = item(SPARK0 + 2u * k + 1u);
    var pos = a.xy;
    var vel = a.zw;
    var age = b.x + dt;
    var life = b.y;
    if (struck && first_beat() > 0.5) {
        life = min(life, age + 0.05 * rand(k, V.time));
    }
    if (h >= head_count() || P.sparks <= 0.0) {
        return select(vec4<f32>(0.0), vec4<f32>(0.0, 0.0, 1.0, 0.0), (i - SPARK0) % 2u == 1u);
    }
    if (life <= 0.0 || age >= life) {
        let head = item(1u + h);
        let t = rand(k, V.time + 1.0);
        pos = mix(head.zw, head.xy, t);
        let ang = rand(k, V.time + 3.0) * TAU;
        vel = vec2<f32>(cos(ang), sin(ang)) * 0.25 * (0.3 + rand(k, V.time + 4.0));
        age = 0.0;
        // Lives stagger from the first frame, so sparks come off steadily.
        life = (0.5 + 1.0 * rand(k, V.time + 5.0)) * select(rand(k, 9.0), 1.0, b.y > 0.0);
    } else {
        vel = vel * exp(-dt * 1.5);
        pos = pos + vel * dt;
    }
    if ((i - SPARK0) % 2u == 0u) {
        return vec4<f32>(pos, vel);
    }
    return vec4<f32>(age, life, f32(h), 0.0);
}

// Distance from p to the segment a..b.
fn segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-12), 0.0, 1.0);
    return length(p - a - ab * t);
}

fn head_colour(h: u32) -> vec3<f32> {
    return colour(f32(h) / f32(head_count()) + V.flow * 0.02 + item(0u).x);
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let p = centred(uv);
    let px = pixel();

    // The tails: last frame's read slightly nearer the centre (so they
    // drift outward) and turned, blurred a little (so they widen and soften
    // as they age), then faded by a time constant, so they look the same at
    // 60 and 240 fps. A fade that only multiplies never reaches zero in a
    // half-float texture, so a little is also taken away.
    let q = rotate(p * (1.0 - P.drift * V.dt), -P.swirl * V.dt);
    let quv = uncentred(q);
    var tail = vec3<f32>(0.0);
    if (all(quv >= vec2<f32>(0.0)) && all(quv <= vec2<f32>(1.0))) {
        // Two reads half a pixel off on the diagonal: each averages four
        // pixels, so together they blur as much as four plain reads.
        let o = 0.5 / V.resolution;
        let around = (trail(quv + o) + trail(quv - o)) * 0.5;
        let spread = clamp(P.spread * V.dt * 30.0, 0.0, 1.0);
        let keep = exp(-V.dt / P.trail);
        tail = max(mix(trail(quv), around, spread) * keep - 0.02 * V.dt, vec3<f32>(0.0));
    }

    // Each head: the segment from last frame's place to this one's (so a
    // fast head leaves no gaps) adds a narrow stretch to its tail, and is
    // drawn over it as a coma and a small white-hot nucleus.
    let flare = item(FLARE).y;
    let n = head_count();
    var heads = vec3<f32>(0.0);
    for (var h = 0u; h < n; h++) {
        let at = item(1u + h);
        let d = segment(p, at.zw, at.xy);
        let nucleus = 0.5 * P.size * px;
        let coma = nucleus * (3.0 + 3.0 * flare);
        let tone = head_colour(h);
        // max, not +: a head resting in one place cannot pile up without bound.
        tail = max(tail, tone * exp(-(d * d) / (nucleus * nucleus * 1.5)) * 0.8);
        let core = clamp((nucleus - d) / px + 0.5, 0.0, 1.0);
        let glow = max(exp(-(d * d) / (coma * coma)) - 0.01, 0.0) * (0.6 + 0.8 * flare);
        heads = heads + tone * glow + mix(tone, vec3<f32>(1.0), 0.75) * core * 2.0;
    }
    write_trail(tail);
    return tail + heads;
}

// A spark: a small soft point, fading over its life.
fn vertex(i: u32) -> MeshVertex {
    let P = params();
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    v.colour = vec4<f32>(0.0);
    v.data = vec4<f32>(0.0);
    let k = i / 6u;
    let a = item(SPARK0 + 2u * k);
    let b = item(SPARK0 + 2u * k + 1u);
    let h = u32(b.z);
    if (P.sparks <= 0.0 || h >= head_count() || b.y <= 0.0 || b.x >= b.y) {
        return v;
    }
    var c = vec2<f32>(-1.0, -1.0);
    switch (i % 6u) {
        case 1u: { c = vec2<f32>(1.0, -1.0); }
        case 2u: { c = vec2<f32>(1.0, 1.0); }
        case 3u: { c = vec2<f32>(-1.0, -1.0); }
        case 4u: { c = vec2<f32>(1.0, 1.0); }
        case 5u: { c = vec2<f32>(-1.0, 1.0); }
        default: {}
    }
    let life = b.x / b.y;
    let size = (1.2 + 0.1 * P.size) * 2.0 / min(V.resolution.x, V.resolution.y);
    v.pos = vec4<f32>(a.xy + c * size, 0.0, 1.0);
    let fade = smoothstep(0.0, 0.1, life) * (1.0 - life) * (1.0 - life);
    v.colour = vec4<f32>(mix(head_colour(h), vec3<f32>(1.0), 0.5) * fade * P.sparks, 0.0);
    v.data = vec4<f32>(c, 0.0, 0.0);
    return v;
}

fn paint(f: MeshFragment) -> vec4<f32> {
    let r2 = dot(f.data.xy, f.data.xy);
    return vec4<f32>(f.colour.rgb * max(exp(-r2 * 4.0) - 0.02, 0.0), 0.0);
}
