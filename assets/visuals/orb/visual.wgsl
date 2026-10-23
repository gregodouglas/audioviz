// Textured sphere: a sphere whose surface is pushed in and out by slowly
// boiling 3D noise. The bass swells it and deepens the lumps, a beat kicks
// it, the highs raise sharp ridges. Its texture (marble, bands or contour
// lines, from the palette) is painted per pixel from the direction on the
// sphere, so it turns with it, and flows with the mids. A light from above
// left, a rim glow, and optionally a reflection on a dark floor below.
//
// The mesh is a cube sphere: 6 faces of N x N quads, warped to even angles.
// Each corner works out its whole triangle and drops it when it faces away
// (the sphere is drawn solid, alpha 1, and lumps never fold far enough for
// front faces to overlap badly). Triangles: the reflection's copy, then the
// sphere's.
//
// Items:
//   0   (spin angle, boil time, texture time, 0)
//   1   (bass swell, bass stir, highs, last beat pulse), eased
//   2   (beat kick, seconds since the beat, 0, 0)

const N: u32 = 96u;
const QUADS: u32 = 55296u;   // 6 * N * N
const EYE_DIST: f32 = 3.2;
const FOCAL: f32 = 3.2;
const PITCH: f32 = 0.22;

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let c0 = item(0u);
    let c1 = item(1u);
    let c2 = item(2u);
    let speed_now = pace();
    if (i == 0u) {
        let spin = (c0.x + dt * P.spin * 0.4 * speed_now) % TAU;
        let boil = (c0.y + dt * 0.2 * speed_now) % 1000.0;
        let tex = (c0.z + dt * P.flow * 0.25 * (0.6 + 0.6 * clamp(V.mid, 0.0, 2.0))) % 1000.0;
        return vec4<f32>(spin, boil, tex, 0.0);
    }
    if (i == 1u) {
        let bass = broad_rel(BASS);
        let swell = ease(c1.x, P.swell * 0.2 * clamp(bass - 1.0, -0.4, 1.5), 0.06, 0.35);
        let stir = ease(c1.y, clamp(0.5 * bass + broad_attack(BASS), 0.0, 2.0), 0.05, 0.4);
        let highs = 0.5 * (broad_rel(PRESENCE) + broad_rel(AIR));
        let spikes = ease(c1.z, clamp(highs - 0.6, 0.0, 1.5), 0.04, 0.3);
        return vec4<f32>(swell, stir, spikes, V.beat);
    }
    // Seconds since the last beat; the kick rises over 40 ms, so it swells
    // rather than jumps, and dies away over a quarter second.
    var since = c2.y + dt;
    if (V.beat > c1.w + 0.4) {
        since = 0.0;
    }
    since = min(since, 10.0);
    let kick = smoothstep(0.0, 0.04, since) * exp(-since / 0.25);
    return vec4<f32>(kick, since, 0.0, 0.0);
}

fn hash3(p: vec3<i32>) -> f32 {
    var n = u32(p.x) * 374761393u + u32(p.y) * 668265263u + u32(p.z) * 2246822519u;
    n = (n ^ (n >> 13u)) * 1274126177u;
    n = n ^ (n >> 16u);
    return f32(n & 0xffffu) / 65535.0;
}

fn vnoise3(p: vec3<f32>) -> f32 {
    let i = vec3<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let x00 = mix(hash3(i), hash3(i + vec3<i32>(1, 0, 0)), u.x);
    let x10 = mix(hash3(i + vec3<i32>(0, 1, 0)), hash3(i + vec3<i32>(1, 1, 0)), u.x);
    let x01 = mix(hash3(i + vec3<i32>(0, 0, 1)), hash3(i + vec3<i32>(1, 0, 1)), u.x);
    let x11 = mix(hash3(i + vec3<i32>(0, 1, 1)), hash3(i + vec3<i32>(1, 1, 1)), u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

// Octaves of value noise, 0..1.
fn fbm3(p: vec3<f32>, octaves: u32) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var norm = 0.0;
    var q = p;
    for (var k = 0u; k < octaves; k++) {
        sum += amp * vnoise3(q);
        norm += amp;
        amp *= 0.5;
        q = q * 2.03 + vec3<f32>(1.7, 9.2, 3.4);
    }
    return sum / norm;
}

// The sphere's radius in direction d (the sphere's own frame).
fn radius_at(d: vec3<f32>) -> f32 {
    let P = params();
    let c0 = item(0u);
    let c1 = item(1u);
    let kick = item(2u).x;
    let base = P.size * (1.0 + motion() * (max(c1.x, -0.1) + 0.06 * P.kick * kick));
    let q = d * P.detail + vec3<f32>(0.0, c0.y, 0.7 * c0.y);
    let lump = fbm3(q, 3u) - 0.5;
    let amount = P.distort * (0.12 + motion() * (0.18 * P.react * c1.y + 0.1 * P.kick * kick));
    var r = base * (1.0 + amount * lump);
    if (P.spikes > 0.0) {
        let ridge = 1.0 - abs(vnoise3(d * 7.0 + vec3<f32>(0.5 * c0.y)) * 2.0 - 1.0);
        r += base * P.spikes * (0.05 + 0.35 * c1.z * motion()) * pow(ridge, 6.0);
    }
    return r;
}

// Direction for face f's point (u, v), each -1..1, warped to even angles.
fn cube_dir(f: u32, uv: vec2<f32>) -> vec3<f32> {
    let a = tan(uv.x * 0.785398);
    let b = tan(uv.y * 0.785398);
    var d = vec3<f32>(0.0);
    switch (f) {
        case 0u: { d = vec3<f32>(1.0, b, -a); }
        case 1u: { d = vec3<f32>(-1.0, b, a); }
        case 2u: { d = vec3<f32>(a, 1.0, -b); }
        case 3u: { d = vec3<f32>(a, -1.0, b); }
        case 4u: { d = vec3<f32>(a, b, 1.0); }
        default: { d = vec3<f32>(-a, b, -1.0); }
    }
    return normalize(d);
}

// The sphere's frame in the world: spun about the vertical, tipped a little.
fn to_world(p: vec3<f32>) -> vec3<f32> {
    let spin = item(0u).x;
    let tip = 0.3 + 0.1 * sin(spin * 0.37);
    let q = vec3<f32>(p.x, p.y * cos(tip) - p.z * sin(tip), p.y * sin(tip) + p.z * cos(tip));
    return vec3<f32>(q.x * cos(spin) + q.z * sin(spin), q.y, -q.x * sin(spin) + q.z * cos(spin));
}

fn surface(f: u32, uv: vec2<f32>) -> vec3<f32> {
    let d = cube_dir(f, uv);
    return to_world(d * radius_at(d));
}

fn floor_y() -> f32 {
    return -1.35 * params().size;
}

struct Eye {
    pos: vec3<f32>,
    right: vec3<f32>,
    up: vec3<f32>,
    fwd: vec3<f32>,
};

fn eye() -> Eye {
    let P = params();
    // With a reflection, look lower so the floor's mirror is in the frame.
    let aim = vec3<f32>(0.0, select(0.0, -0.9 * P.size, P.reflection > 0.0), 0.0);
    var e: Eye;
    e.pos = aim + EYE_DIST * vec3<f32>(0.0, sin(PITCH), -cos(PITCH));
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
    if (copy == 0u && P.reflection <= 0.0) {
        return v;
    }
    let qf = quad % QUADS;
    let f = qf / (N * N);
    let cell = qf % (N * N);
    let base = vec2<f32>(f32(cell % N), f32(cell / N));
    // Two triangles a quad: (0,0) (1,0) (1,1) and (0,0) (1,1) (0,1).
    var o0 = vec2<f32>(0.0, 0.0);
    var o1 = vec2<f32>(1.0, 0.0);
    var o2 = vec2<f32>(1.0, 1.0);
    if (corner >= 3u) {
        o1 = vec2<f32>(1.0, 1.0);
        o2 = vec2<f32>(0.0, 1.0);
    }
    let cs = 2.0 / f32(N);
    let uv0 = (base + o0) * cs - 1.0;
    let uv1 = (base + o1) * cs - 1.0;
    let uv2 = (base + o2) * cs - 1.0;
    let a = surface(f, uv0);
    let b = surface(f, uv1);
    let c = surface(f, uv2);
    let centroid = (a + b + c) / 3.0;
    var n = cross(b - a, c - a);
    if (dot(n, centroid) < 0.0) {
        n = -n;
    }
    let e = eye();
    let fy = floor_y();
    // The reflection is the sphere seen from the eye mirrored in the floor.
    var seen_from = e.pos;
    if (copy == 0u) {
        seen_from.y = 2.0 * fy - seen_from.y;
    }
    if (dot(n, seen_from - centroid) <= 0.0) {
        return v;
    }
    let k = corner % 3u;
    var uv = uv0;
    var p = a;
    if (k == 1u) { uv = uv1; p = b; }
    if (k == 2u) { uv = uv2; p = c; }
    // A smooth normal from two small steps across the surface.
    let h = 0.25 * cs;
    let pu = surface(f, uv + vec2<f32>(h, 0.0));
    let pv = surface(f, uv + vec2<f32>(0.0, h));
    var nn = normalize(cross(pu - p, pv - p));
    if (dot(nn, p) < 0.0) {
        nn = -nn;
    }
    var bright = 1.0;
    if (copy == 0u) {
        let above = p.y - fy;
        p.y = fy - above;
        nn.y = -nn.y;
        bright = -P.reflection * 0.25 * exp(-above * 2.0 / max(P.size, 0.1));
    }
    let rel = p - e.pos;
    let vz = max(dot(rel, e.fwd), 0.05);
    v.pos = vec4<f32>(dot(rel, e.right) * FOCAL, dot(rel, e.up) * FOCAL, 0.0, vz);
    v.colour = vec4<f32>(nn, 0.0);
    v.data = vec4<f32>(cube_dir(f, uv), bright);
    return v;
}

// The texture at direction d on the sphere: colour, before the light.
fn skin_colour(d: vec3<f32>) -> vec3<f32> {
    let P = params();
    let ft = item(0u).z;
    let q = d * P.scale;
    if (P.skin > 0.5 && P.skin < 1.5) {
        // Bands, stirred.
        let lat = d.y * P.scale * 1.2 + 1.6 * (fbm3(q * 0.8 + vec3<f32>(ft, 0.0, 0.3 * ft), 3u) - 0.5);
        let stripe = smoothstep(-0.6, 0.6, sin(lat * 9.0));
        return colour(fract(lat * 0.25)) * (0.3 + 0.9 * stripe);
    }
    // Marble: noise looked up through noise.
    let w = vec3<f32>(
        fbm3(q + vec3<f32>(0.0, ft, 0.0), 3u),
        fbm3(q + vec3<f32>(5.2, 1.3 - 0.7 * ft, 2.8), 3u),
        fbm3(q + vec3<f32>(-3.1, 7.7, 0.5 * ft), 3u));
    let m = fbm3(q + 4.5 * (w - 0.5), 3u);
    let t = clamp((m - 0.5) * 2.2 + 0.5, 0.0, 1.0);
    if (P.skin > 1.5) {
        // Contour lines of the marble.
        let x = m * 14.0;
        let lw = max(fwidth(x), 1e-4);
        let line = 1.0 - smoothstep(0.6, 1.6, abs(fract(x + 0.5) - 0.5) / lw);
        return colour(t) * (0.06 + 1.5 * line * clamp(1.5 - lw * 2.0, 0.0, 1.0));
    }
    return colour(t) * (0.3 + 0.9 * t);
}

fn paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    let n = normalize(f.colour.xyz);
    let d = normalize(f.data.xyz);
    let view = -eye().fwd;
    let light = normalize(vec3<f32>(-0.5, 0.8, -0.45));
    let diffuse = max(dot(n, light), 0.0);
    let lit = mix(1.0, 0.15 + 0.95 * diffuse, clamp(P.light, 0.0, 1.0));
    let spec = pow(max(dot(n, normalize(light + view)), 0.0), 40.0) * 0.5 * P.light;
    let rim = pow(1.0 - clamp(dot(n, view), 0.0, 1.0), 3.0) * P.rim;
    let c = skin_colour(d) * lit + colour(0.85) * rim * 0.9 + vec3<f32>(spec);
    if (f.data.w < 0.0) {
        return vec4<f32>(c * -f.data.w, 0.0);
    }
    return vec4<f32>(c * f.data.w, 1.0);
}
