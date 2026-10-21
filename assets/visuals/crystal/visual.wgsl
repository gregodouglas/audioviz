// Crystal: translucent tetrahedral shards tumbling in a group. The group
// swells with loudness and on a drop; every few beats one shard eases to a
// new pose (rotation, scale, opacity, colour); Scatter flings the shards
// apart over the bar, closing on its first beat. Glass facets add up; solid ones cover each other, drawn far to
// near. Edges glow in the palette.
//
// After the Crystal in Felix Turner's Uberviz (https://uberviz.io). No code
// used; written fresh.
//
// Prepare items (state the compiled-in Crystal kept on the CPU):
//   0            (smoothed energy, last beat pulse, beats counted, 1 once started)
//   1            (tumble angles x, y, z, 0)
//   2, 3         the group's tilt and the tilt it eases toward (quaternions)
//   4 + 5s ..    shard s: from rotation, to rotation, (from scale, from opacity),
//                (to scale, to opacity), (from hue, to hue, ease progress, 0)
// Triangles: 16 shards x 4 faces, then 16 shards x 6 edges as quads.

const MAX_SHARDS: u32 = 16u;
const FACE_CORNERS: u32 = 192u;    // 16 * 4 * 3
const SHARD0: u32 = 4u;

// ---------------------------------------------------------------- quaternions

fn q_mul(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(a.w * b.xyz + b.w * a.xyz + cross(a.xyz, b.xyz), a.w * b.w - dot(a.xyz, b.xyz));
}

fn q_rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

// Rotations about x, then y, then z composed as X * Y * Z (glam's XYZ).
fn q_euler(a: vec3<f32>) -> vec4<f32> {
    let qx = vec4<f32>(sin(a.x * 0.5), 0.0, 0.0, cos(a.x * 0.5));
    let qy = vec4<f32>(0.0, sin(a.y * 0.5), 0.0, cos(a.y * 0.5));
    let qz = vec4<f32>(0.0, 0.0, sin(a.z * 0.5), cos(a.z * 0.5));
    return q_mul(q_mul(qx, qy), qz);
}

fn q_slerp(a: vec4<f32>, b0: vec4<f32>, t: f32) -> vec4<f32> {
    var b = b0;
    var d = dot(a, b);
    if (d < 0.0) {
        b = -b;
        d = -d;
    }
    if (d > 0.9995) {
        return normalize(mix(a, b, t));
    }
    let th = acos(d);
    return (a * sin((1.0 - t) * th) + b * sin(t * th)) / sin(th);
}

// ---------------------------------------------------------------- randomness

fn rnd(seed: vec2<f32>, k: f32) -> f32 {
    return hash21(seed + vec2<f32>(k * 13.71, k * 7.33));
}

struct Pose {
    rot: vec4<f32>,
    scale: vec3<f32>,
    opacity: f32,
    hue: f32,
};

fn random_pose(seed: vec2<f32>) -> Pose {
    var p: Pose;
    p.rot = q_euler(vec3<f32>(rnd(seed, 0.0), rnd(seed, 1.0), rnd(seed, 2.0)) * TAU);
    p.scale = vec3<f32>(0.8 + 1.4 * rnd(seed, 3.0), 0.8 + 1.4 * rnd(seed, 4.0), 0.8 + 1.4 * rnd(seed, 5.0));
    p.opacity = 0.25 + 0.45 * rnd(seed, 6.0);
    p.hue = rnd(seed, 7.0);
    return p;
}

// Where a shard sits in the cluster, and where Scatter flings it.
fn rest_of(s: u32) -> vec3<f32> {
    let seed = vec2<f32>(f32(s), 51.0);
    return (vec3<f32>(rnd(seed, 0.0), rnd(seed, 1.0), rnd(seed, 2.0)) * 2.0 - 1.0) * 0.15;
}

fn far_of(s: u32) -> vec3<f32> {
    let seed = vec2<f32>(f32(s), 77.0);
    return (vec3<f32>(rnd(seed, 0.0), rnd(seed, 1.0), rnd(seed, 2.0)) * 2.0 - 1.0) * 2.0;
}

// ---------------------------------------------------------------- state

fn ease_in_expo(t: f32) -> f32 {
    if (t <= 0.0) { return 0.0; }
    if (t >= 1.0) { return 1.0; }
    return exp2(10.0 * (t - 1.0));
}

fn shard_count() -> u32 {
    return u32(clamp(params().count, 1.0, f32(MAX_SHARDS)));
}

// A shard's pose as its items stand (this frame's in vertex(), last frame's
// in prepare()).
fn current_pose(s: u32) -> Pose {
    let b = SHARD0 + s * 5u;
    let e = item(b + 4u);
    let k = ease_in_expo(e.z);
    var p: Pose;
    p.rot = q_slerp(item(b), item(b + 1u), k);
    p.scale = mix(item(b + 2u).xyz, item(b + 3u).xyz, k);
    p.opacity = mix(item(b + 2u).w, item(b + 3u).w, k);
    p.hue = select(mix(e.x, e.y, k), e.y, e.z >= 0.999);
    return p;
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let last0 = item(0u);
    let started = last0.w > 0.5;
    let energy0 = select(1.0, last0.x, started);
    let energy = energy0 + (V.energy - energy0) * (1.0 - exp(-V.dt / 0.25));
    // A beat: the pulse jumped since last frame.
    let hit = started && P.react > 0.0 && V.beat > last0.y + 0.4;
    let beats = last0.z + select(0.0, 1.0, hit);
    if (i == 0u) {
        return vec4<f32>(energy, V.beat, beats, 1.0);
    }
    if (i == 1u) {
        let rate = vec3<f32>(0.6, 0.15, 0.6) * V.dt * P.spin * (0.6 + 0.4 * energy * min(P.react, 1.0));
        return vec4<f32>(item(1u).xyz + rate, 0.0);
    }
    // Every fourth beat the group tilts toward a new angle.
    var tilt_to = select(vec4<f32>(0.0, 0.0, 0.0, 1.0), item(3u), started);
    if (hit && u32(beats) % 4u == 0u) {
        tilt_to = q_euler(vec3<f32>(rnd(vec2<f32>(beats, 3.0), 0.0), rnd(vec2<f32>(beats, 3.0), 1.0), rnd(vec2<f32>(beats, 3.0), 2.0)) * TAU);
    }
    if (i == 3u) {
        return tilt_to;
    }
    if (i == 2u) {
        let tilt = select(vec4<f32>(0.0, 0.0, 0.0, 1.0), item(2u), started);
        return normalize(q_slerp(tilt, tilt_to, 1.0 - exp(-V.dt / 0.6)));
    }
    let s = (i - SHARD0) / 5u;
    let part = (i - SHARD0) % 5u;
    var start: Pose;
    var to: Pose;
    var t = 1.0;
    if (!started) {
        start = random_pose(vec2<f32>(f32(s), 1000.0));
        to = start;
    } else {
        let b = SHARD0 + s * 5u;
        start.rot = item(b);
        to.rot = item(b + 1u);
        start.scale = item(b + 2u).xyz;
        start.opacity = item(b + 2u).w;
        to.scale = item(b + 3u).xyz;
        to.opacity = item(b + 3u).w;
        start.hue = item(b + 4u).x;
        to.hue = item(b + 4u).y;
        t = item(b + 4u).z;
        // On every Nth beat one shard, the same for every item, eases to a new pose.
        let every = u32(max(P.every, 1.0));
        let pick = min(u32(rnd(vec2<f32>(beats, 5.0), 0.0) * f32(shard_count())), shard_count() - 1u);
        if (hit && u32(beats) % every == 0u && s == pick) {
            start = current_pose(s);
            to = random_pose(vec2<f32>(beats, f32(s) + 9.0));
            t = 0.0;
        }
        let beat_len = clamp(60.0 / max(V.bpm, 40.0), 0.2, 2.0);
        t = min(t + V.dt / (2.0 * beat_len), 1.0);
    }
    switch (part) {
        case 0u: { return start.rot; }
        case 1u: { return to.rot; }
        case 2u: { return vec4<f32>(start.scale, start.opacity); }
        case 3u: { return vec4<f32>(to.scale, to.opacity); }
        default: { return vec4<f32>(start.hue, to.hue, t, 0.0); }
    }
}

// ---------------------------------------------------------------- drawing

struct Group {
    rot: vec4<f32>,
    scale: f32,
    fling: f32,
};

// A drop's swell, 0..0.25: it rises as the drop pulse fades (peaking ~0.4 s
// after the drop), so it blooms in rather than jumping.
fn drop_bloom() -> f32 {
    let d = drop_pulse();
    return d * (1.0 - d);
}

fn group() -> Group {
    let P = params();
    var g: Group;
    g.rot = q_mul(item(2u), q_euler(item(1u).xyz));
    g.scale = P.size * (0.55 + 0.45 * item(0u).x * min(P.react, 1.5) + 0.25 * V.onset * P.react + 1.2 * drop_bloom() * min(P.react, 1.5)) * 0.4;
    // Scatter: out to the far places and back over the bar (the analysis's
    // bar, so it closes on the first beat).
    g.fling = P.scatter * abs(sin(bar_phase() * 3.14159265));
    return g;
}

fn corner(i: u32) -> vec3<f32> {
    switch (i) {
        case 0u: { return vec3<f32>(1.0, 1.0, 1.0); }
        case 1u: { return vec3<f32>(1.0, -1.0, -1.0); }
        case 2u: { return vec3<f32>(-1.0, 1.0, -1.0); }
        default: { return vec3<f32>(-1.0, -1.0, 1.0); }
    }
}

fn face_corner(face: u32, v: u32) -> u32 {
    switch (face * 3u + v) {
        case 0u: { return 0u; } case 1u: { return 1u; } case 2u: { return 2u; }
        case 3u: { return 0u; } case 4u: { return 3u; } case 5u: { return 1u; }
        case 6u: { return 0u; } case 7u: { return 2u; } case 8u: { return 3u; }
        case 9u: { return 1u; } case 10u: { return 3u; } default: { return 2u; }
    }
}

fn edge_corners(e: u32) -> vec2<u32> {
    switch (e) {
        case 0u: { return vec2<u32>(0u, 1u); }
        case 1u: { return vec2<u32>(0u, 2u); }
        case 2u: { return vec2<u32>(0u, 3u); }
        case 3u: { return vec2<u32>(1u, 2u); }
        case 4u: { return vec2<u32>(1u, 3u); }
        default: { return vec2<u32>(2u, 3u); }
    }
}

// A corner of shard s in the world.
fn world(s: u32, pose: Pose, g: Group, c: vec3<f32>) -> vec3<f32> {
    let pos = mix(rest_of(s), far_of(s), g.fling);
    let local = pos + q_rotate(pose.rot, pose.scale * vec3<f32>(params().stretch, 1.0, 1.0) * c);
    return q_rotate(g.rot, local) * g.scale;
}

// The compiled-in camera: 45 degree field of view, six units back.
fn project(p: vec3<f32>) -> vec4<f32> {
    let f = 1.0 / tan(radians(22.5));
    let aspect = V.resolution.x / max(V.resolution.y, 1.0);
    let z = p.z - 6.0;
    return vec4<f32>(p.x * f / aspect, p.y * f, 0.0, -z);
}

// Clip space to the centred units a MeshVertex wants (w kept for perspective).
fn to_centred(clip: vec4<f32>) -> vec4<f32> {
    let short = min(V.resolution.x, V.resolution.y);
    return vec4<f32>(clip.x * V.resolution.x / short, clip.y * V.resolution.y / short, 0.0, clip.w);
}

// Solid shards are drawn far to near: the shard at draw slot r.
fn shard_at(r: u32, g: Group) -> u32 {
    let n = shard_count();
    if (params().style > 0.5) {
        return r;
    }
    for (var j = 0u; j < n; j++) {
        let dj = q_rotate(g.rot, mix(rest_of(j), far_of(j), g.fling)).z;
        var rank = 0u;
        for (var k = 0u; k < n; k++) {
            let dk = q_rotate(g.rot, mix(rest_of(k), far_of(k), g.fling)).z;
            if (dk < dj || (dk == dj && k < j)) {
                rank++;
            }
        }
        if (rank == r) {
            return j;
        }
    }
    return r;
}

fn shard_colour(pose: Pose) -> vec3<f32> {
    // Without a palette the shards vary the tint a little in brightness.
    let b = 0.7 + 0.6 * pose.hue;
    let x = fract(pose.hue + V.palette_shift) * 32.0;
    let i = u32(floor(x)) % 32u;
    let pal = mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
    return mix(V.tint.rgb * b, pal * 1.2, V.palette_mix);
}

fn vertex(i: u32) -> MeshVertex {
    let P = params();
    let n = shard_count();
    let g = group();
    let solid = P.style < 0.5;
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    v.colour = vec4<f32>(0.0);
    v.data = vec4<f32>(0.0);
    if (i < FACE_CORNERS) {
        let tri = i / 3u;
        let vi = i % 3u;
        let slot = tri / 4u;
        let face = tri % 4u;
        if (slot >= n) {
            return v;
        }
        let s = shard_at(slot, g);
        let pose = current_pose(s);
        let a = world(s, pose, g, corner(face_corner(face, 0u)));
        let b = world(s, pose, g, corner(face_corner(face, 1u)));
        let c = world(s, pose, g, corner(face_corner(face, 2u)));
        let nrm = normalize(cross(b - a, c - a));
        var shade = 0.45 + 0.55 * abs(dot(nrm, normalize(vec3<f32>(0.35, 0.7, 1.0))));
        // Solid shards are mostly opaque; glass ones thin, vivid and lightly cut.
        var alpha = pose.opacity;
        if (solid) {
            alpha = 0.6 + 0.4 * (pose.opacity - 0.1) / 0.5;
        } else {
            shade = 0.75 + 0.25 * shade;
        }
        let p = world(s, pose, g, corner(face_corner(face, vi)));
        v.pos = to_centred(project(p));
        if (solid) {
            let cover = clamp(alpha * P.facets, 0.0, 1.0);
            v.colour = vec4<f32>(shard_colour(pose) * shade * cover, cover);
        } else {
            v.colour = vec4<f32>(shard_colour(pose) * shade * alpha * P.facets, 0.0);
        }
        v.data = vec4<f32>(f32(vi == 0u), f32(vi == 1u), f32(vi == 2u), 0.0);
        return v;
    }
    // Edges: a pixel-space quad round each of the six edges.
    if (P.edges <= 0.0) {
        return v;
    }
    let k = i - FACE_CORNERS;
    let vi = k % 6u;
    let quad = k / 6u;
    let slot = quad / 6u;
    if (slot >= n) {
        return v;
    }
    let s = shard_at(slot, g);
    let pose = current_pose(s);
    let ec = edge_corners(quad % 6u);
    let ca = project(world(s, pose, g, corner(ec.x)));
    let cb = project(world(s, pose, g, corner(ec.y)));
    if (ca.w <= 0.0 || cb.w <= 0.0) {
        return v;
    }
    let res = V.resolution;
    let pa = vec2<f32>((ca.x / ca.w + 1.0) * 0.5 * res.x, (1.0 - ca.y / ca.w) * 0.5 * res.y);
    let pb = vec2<f32>((cb.x / cb.w + 1.0) * 0.5 * res.x, (1.0 - cb.y / cb.w) * 0.5 * res.y);
    var d = pb - pa;
    let len = length(d);
    if (len < 1e-4) { d = vec2<f32>(1.0, 0.0); } else { d = d / len; }
    let nrm = vec2<f32>(-d.y, d.x);
    let w = P.width * 0.5 + 2.0;
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
    let px = pa + d * along + nrm * across;
    let ndc = vec2<f32>(px.x / res.x * 2.0 - 1.0, 1.0 - px.y / res.y * 2.0);
    v.pos = to_centred(vec4<f32>(ndc, 0.0, 1.0));
    v.colour = vec4<f32>(shard_colour(pose) * P.edges, 0.0);
    v.data = vec4<f32>(along, across, len, 1.0);
    return v;
}

fn paint(f: MeshFragment) -> vec4<f32> {
    if (f.data.w < 0.5) {
        // A facet: brighter towards its edges.
        let e = min(f.data.x, min(f.data.y, f.data.z));
        let rim = 1.0 + 1.2 * exp(-e * 12.0);
        return vec4<f32>(f.colour.rgb * rim, f.colour.a);
    }
    let along = clamp(f.data.x, 0.0, f.data.z);
    let dist = length(vec2<f32>(f.data.x - along, f.data.y));
    let half = max(params().width, 0.5) * 0.5;
    let core = 1.0 - smoothstep(half - 0.5, half + 0.8, dist);
    let halo = exp(-dist * dist / (2.0 * (half + 2.0) * (half + 2.0))) * 0.35;
    return vec4<f32>(f.colour.rgb * (core + halo), 0.0);
}
