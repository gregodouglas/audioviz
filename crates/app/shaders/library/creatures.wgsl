// Creatures: helpers for visuals with living things moving in 3D. A visual
// opts in with "uses": ["creatures"] in visual.json; this file then sits
// between the API and its code. See docs/building-visuals.md, "Creatures".
//
// The pieces: a camera into a tank (to camera space, to the screen, to a
// triangle corner), a slow water current, swim strokes, a chain link for
// tentacles, hair and arms (no momentum: chains with momentum wind into
// loops), turning an axis, keeping apart, wandering. Everything is in world
// units: the tank is a few units across, y up. How things look (light,
// glass, tissue) is the "luminous" library; bells are "jelly".

// The `Camera` (in the API, so a shared space can pass it between layers)
// looks at `look` (the origin unless set) from `dist` away, turned `yaw`
// about the vertical and tipped `pitch` (radians). `focal` is centred units
// per unit of depth: 2.2 at a distance of 4.2 shows about 1.9 units above
// and below the middle.

// A camera that swings slowly round the tank by up to `drift` radians.
fn drifting_camera(drift: f32, dist: f32, focal: f32) -> Camera {
    var c: Camera;
    c.yaw = (sin(V.time * 0.021) * 0.9 + sin(V.time * 0.0073 + 1.3) * 0.4) * drift;
    c.pitch = sin(V.time * 0.017 + 0.4) * 0.12 * drift;
    c.dist = dist;
    c.focal = focal;
    c.look = vec3<f32>(0.0);
    return c;
}

// A direction in world space turned into camera space (no translation).
fn camera_dir(c: Camera, d: vec3<f32>) -> vec3<f32> {
    let cy = cos(c.yaw);
    let sy = sin(c.yaw);
    let q = vec3<f32>(cy * d.x - sy * d.z, d.y, sy * d.x + cy * d.z);
    let cp = cos(c.pitch);
    let sp = sin(c.pitch);
    return vec3<f32>(q.x, cp * q.y - sp * q.z, sp * q.y + cp * q.z);
}

// A world point in camera space: x right, y up, z its depth (distance in
// front of the camera).
fn to_camera(c: Camera, p: vec3<f32>) -> vec3<f32> {
    return camera_dir(c, p - c.look) + vec3<f32>(0.0, 0.0, c.dist);
}

// The direction (world space, unit) from the eye through the pixel at
// `pixel` (in pixels, as MeshFragment.pixel gives it).
fn pixel_ray(c: Camera, pixel: vec2<f32>) -> vec3<f32> {
    let p = centred(pixel / V.resolution);
    let d = normalize(vec3<f32>(p / c.focal, 1.0));
    // camera_dir, undone: the pitch, then the yaw.
    let cp = cos(c.pitch);
    let sp = sin(c.pitch);
    let q = vec3<f32>(d.x, cp * d.y + sp * d.z, -sp * d.y + cp * d.z);
    let cy = cos(c.yaw);
    let sy = sin(c.yaw);
    return vec3<f32>(cy * q.x + sy * q.z, q.y, -sy * q.x + cy * q.z);
}

// Where the camera is, in world space.
fn camera_position(c: Camera) -> vec3<f32> {
    // The inverse of camera_dir applied to (0, 0, -dist).
    let cp = cos(c.pitch);
    let sp = sin(c.pitch);
    let q = vec3<f32>(0.0, sp * -c.dist, cp * -c.dist);
    let cy = cos(c.yaw);
    let sy = sin(c.yaw);
    return vec3<f32>(cy * q.x + sy * q.z, q.y, -sy * q.x + cy * q.z) + c.look;
}

// Where world point p lands on screen (centred coordinates, xy), and how
// many centred units one world unit measures there (z; 0 behind the camera).
fn project(c: Camera, p: vec3<f32>) -> vec3<f32> {
    let v = to_camera(c, p);
    let d = max(v.z, 0.05);
    return vec3<f32>(v.xy * c.focal / d, select(0.0, c.focal / d, v.z > 0.05));
}

// World point p as a triangle corner (MeshVertex.pos), with perspective.
fn clip_pos(c: Camera, p: vec3<f32>) -> vec4<f32> {
    let v = to_camera(c, p);
    return vec4<f32>(v.xy * c.focal, 0.0, max(v.z, 0.01));
}

// How much of a surface with world normal n at p faces the camera (1 face
// on, 0 edge on; either side).
fn facing(c: Camera, p: vec3<f32>, n: vec3<f32>) -> f32 {
    return abs(dot(normalize(camera_dir(c, n)), -normalize(to_camera(c, p))));
}

// Dimming with distance from the camera beyond `near`, for depth.
fn depth_fade(c: Camera, p: vec3<f32>, near: f32, rate: f32) -> f32 {
    return exp(-max(to_camera(c, p).z - near, 0.0) * rate);
}

// The water: a smooth, slowly changing current, about unit size at its
// strongest (scale it to taste: 0.05 units per second drifts gently).
fn current(p: vec3<f32>, t: f32) -> vec3<f32> {
    return vec3<f32>(
        sin(p.y * 0.9 + t * 0.11) + 0.5 * sin(p.z * 1.7 - t * 0.07),
        0.35 * sin(p.z * 0.8 + t * 0.13) + 0.2 * sin(p.x * 1.3 + t * 0.05),
        sin(p.x * 0.7 - t * 0.09) + 0.5 * sin(p.y * 1.5 + t * 0.06)
    ) * 0.5;
}

// A swim stroke `since` seconds after it began: 0 to 1 over `squeeze`
// seconds, then a release over about `release` seconds that overshoots a
// little (a bell flares past its rest shape).
fn stroke(since: f32, squeeze: f32, release: f32) -> f32 {
    if (since < 0.0) {
        return 0.0;
    }
    if (since < squeeze) {
        return smoothstep(0.0, squeeze, since);
    }
    let r = (since - squeeze) / release;
    return exp(-r) - 0.12 * sin(clamp(r * 0.9, 0.0, 3.14159));
}

// An elastic swim stroke `since` seconds after it began: closes 0 to 1 over
// `squeeze` seconds (eased), then springs back, crossing rest after
// `spring` seconds, overshooting below zero (a bell flaring past its rest
// shape) and settling. Crisper than `stroke`: what a bell of springy jelly
// does once the muscle lets go.
fn elastic_stroke(since: f32, squeeze: f32, spring: f32) -> f32 {
    if (since < 0.0) {
        return 0.0;
    }
    if (since < squeeze) {
        return 0.5 - 0.5 * cos(since / squeeze * 3.14159);
    }
    let r = since - squeeze;
    return exp(-r / spring) * cos(r / spring * 1.5708);
}

// One link of a chain (a tentacle, a hair, an arm) for this frame: `held`
// is where it was (water holds it, so it trails), `parent` the link above
// (as it was last frame, or the anchor); it eases toward lying along
// `rest` from its parent by `ease` (0..1, e.g. 1 - exp(-dt * 3)) and stays
// `len` from it. No momentum, so a chain never winds itself into loops.
fn chain_link(held: vec3<f32>, parent: vec3<f32>, rest: vec3<f32>, ease: f32, len: f32) -> vec3<f32> {
    let h = held - parent;
    let l = length(h);
    let was = select(rest, h / max(l, 1e-6), l > 1e-6);
    let d = normalize(mix(was, rest, clamp(ease, 0.0, 1.0)) + vec3<f32>(1e-6, 0.0, 0.0));
    return parent + d * len;
}

// Unit `axis` turned toward direction `to` by fraction `amount`.
fn turn_toward(axis: vec3<f32>, to: vec3<f32>, amount: f32) -> vec3<f32> {
    let t = normalize(to + vec3<f32>(0.0, 1e-6, 0.0));
    return normalize(mix(axis, t, clamp(amount, 0.0, 1.0)) + vec3<f32>(0.0, 1e-6, 0.0));
}

// A unit vector perpendicular to unit a (the second is cross(a, first)).
fn perpendicular(a: vec3<f32>) -> vec3<f32> {
    let u = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(a.x) > 0.8);
    return normalize(cross(a, u));
}

// A push away from q for something at p closer than r (zero beyond).
fn keep_apart(p: vec3<f32>, q: vec3<f32>, r: f32) -> vec3<f32> {
    let d = p - q;
    let l = length(d);
    return select(vec3<f32>(0.0), d / max(l, 1e-4) * (1.0 - l / r), l < r && l > 1e-5);
}

// A stable 0..1 number for creature `id` (differs per layer), `salt` for more.
fn creature_hash(id: f32, salt: f32) -> f32 {
    return hash21(vec2<f32>(id * 7.31 + salt * 1.7, V.instance * 113.0 + salt * 3.1));
}

// A smooth wandering point inside the box of half-size `half`, different
// per id, moving about once every 1 / rate seconds.
fn wander_point(id: f32, half: vec3<f32>, rate: f32) -> vec3<f32> {
    let t = V.time * rate;
    let s = creature_hash(id, 9.0) * 50.0;
    let n = vec3<f32>(noise(vec2<f32>(t + s, id * 3.0)), noise(vec2<f32>(t + s * 1.3, id * 3.0 + 7.0)), noise(vec2<f32>(t + s * 0.7, id * 3.0 + 13.0)));
    return clamp((n * 2.0 - 1.0) * half * 1.6, -half, half);
}
