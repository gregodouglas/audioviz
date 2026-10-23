// Style Real: eyes traced like real ones, per pixel. Each eye is the
// eyeball (a sphere: the white) with the cornea, a smaller clear sphere,
// bulging from it over the iris. A ray that meets the cornea is split by
// its Fresnel factor into the light it mirrors (the room: light.wgsl) and
// the light it lets through, bent by the cornea onto the iris (iris.wgsl),
// so the iris shifts with the angle as a real one does. A ray that meets
// the white is lit as wet, translucent tissue with its veins.
//
// The measures, in eyeball radii (a real eyeball's is 12 mm): the cornea's
// radius 0.65 (7.8 mm), its centre 0.443 in front of the eyeball's, so it
// meets the white at the limbus, 0.4875 from the axis (11.7 mm across), at
// 0.873 along it; the iris lies at 0.80, 0.47 wide.
//
// Items (as the cartoon's):
//   0        (gaze point, last beat pulse)
//   1        (the point the gaze is heading for, time of the next look)
//   2        (beats, pupil, how open the lids are, the cluster's reach)
//   E0 + k   eye k: (blink start time, 0, 0, 0)
//   L0 + k   eye k's place: (centre, radius)

const E0: u32 = 3u;
const L0: u32 = 35u;
const MAX_EYES: u32 = 32u;
const CAMERA: vec3<f32> = vec3<f32>(0.0, 0.0, 4.0);
const FOCAL: f32 = 4.0;

const CORNEA_R: f32 = 0.65;
const CORNEA_C: f32 = 0.443;
const LIMBUS_R: f32 = 0.4875;
const LIMBUS_Z: f32 = 0.873;
const IRIS_Z: f32 = 0.80;
const IRIS_R: f32 = 0.47;
const CORNEA_N: f32 = 1.376;

fn macro_view() -> bool {
    return u32(params().view) == 2u;
}

fn eye_count() -> u32 {
    return select(clamp(u32(params().count), 1u, MAX_EYES), 1u, macro_view());
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let c0 = item(0u);
    let c1 = item(1u);
    let c2 = item(2u);
    let struck = V.beat > c0.w + 0.4;
    if (i == 0u) {
        var gaze = c0.xyz;
        if (dot(gaze, gaze) == 0.0) {
            gaze = CAMERA;
        }
        // A saccade: most of the way in a few hundredths of a second.
        gaze = mix(gaze, c1.xyz, 1.0 - exp(-dt / 0.022));
        return vec4<f32>(gaze, V.beat);
    }
    if (i == 1u) {
        let dart = V.hits.w > 0.5 && rand(7u, V.time) < P.darting * 0.5;
        if (dot(c1.xyz, c1.xyz) == 0.0 || V.time > c1.w || dart) {
            let s = V.time + 0.123;
            var aim = CAMERA;
            if (rand(1u, s) >= P.stare) {
                aim = vec3<f32>(
                    P.wander * (rand(2u, s) * 2.0 - 1.0) * 2.0,
                    P.wander * (rand(3u, s) * 2.0 - 1.0) * 1.0,
                    2.0 + 1.5 * rand(4u, s));
            }
            return vec4<f32>(aim, V.time + 1.2 + 2.5 * rand(5u, s));
        }
        return c1;
    }
    if (i == 2u) {
        let beats = (c2.x + select(0.0, 1.0, struck)) % 65536.0;
        // Real pupils answer late (about a fifth of a second) and slowly.
        let pupil = ease(c2.y, clamp(broad_rel(BASS) - 0.6, 0.0, 1.5), 0.25, 0.9);
        let open = ease(select(c2.z, 1.0, c2.z <= 0.0), select(1.0, 0.8, V.quiet > 0.5), 0.5, 1.5);
        var reach = 0.0;
        for (var k = 0u; k < eye_count(); k++) {
            let e = item(L0 + k);
            reach = max(reach, length(e.xy) + e.w * 1.15);
        }
        return vec4<f32>(beats, pupil, open, reach * 1.05 + 0.02);
    }
    let n = eye_count();
    if (i >= L0) {
        return eye_at(i - L0, n);
    }
    let k = i - E0;
    let e = item(i);
    var start = select(e.x, -10.0, e.x == 0.0);
    if (struck && rand(k, c2.x + 1.0) < P.blink) {
        start = V.time;
    }
    return vec4<f32>(start, 0.0, 0.0, 0.0);
}

// Eye k: (centre, radius). Macro: one eye filling much of the picture.
// Otherwise a sunflower cluster from the middle out, the middle ones nearer.
fn eye_at(k: u32, n: u32) -> vec4<f32> {
    let P = params();
    if (macro_view()) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.62);
    }
    let a = f32(k) * 2.39996 + 0.3;
    let rr = P.spread * sqrt((f32(k) + 0.5) / f32(n));
    let sway = 0.02 * vec2<f32>(sin(V.time * 0.31), sin(V.time * 0.23 + 1.0));
    let xy = vec2<f32>(cos(a) * 1.15, sin(a) * 0.85) * rr + sway;
    let r = P.spread * 0.85 / sqrt(f32(n)) * (0.85 + 0.3 * rand(k, 91.0));
    let z = 0.4 * P.spread * (1.0 - rr * rr / max(P.spread * P.spread, 1e-3));
    return vec4<f32>(xy, z, r);
}

// ---------------------------------------------------------------- tracing

// The nearer of a ray's two crossings of a sphere (-1 if it misses).
fn sphere_hit(ro: vec3<f32>, rd: vec3<f32>, c: vec3<f32>, r: f32) -> f32 {
    let oc = ro - c;
    let b = dot(oc, rd);
    let h = b * b - (dot(oc, oc) - r * r);
    if (h < 0.0) {
        return -1.0;
    }
    return -b - sqrt(h);
}

// An eye's frame: where it looks (with the tremor every eye has, a few
// hundredths of a degree), and two directions across it.
struct Frame {
    c: vec3<f32>,
    r: f32,
    g: vec3<f32>,
    x: vec3<f32>,
    y: vec3<f32>,
};

fn frame_of(k: u32) -> Frame {
    let e = item(L0 + k);
    var f: Frame;
    f.c = e.xyz;
    f.r = e.w;
    let tremor = vec3<f32>(noise(vec2<f32>(V.time * 40.0, f32(k))), noise(vec2<f32>(V.time * 37.0, f32(k) + 9.0)), 0.0) - 0.5;
    f.g = normalize(item(0u).xyz - e.xyz + tremor * 0.004);
    f.x = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), f.g));
    f.y = cross(f.g, f.x);
    return f;
}

// Where a ray first meets eye k (the white or the cornea), -1 if nowhere;
// `cornea` says which.
struct Hit {
    t: f32,
    cornea: bool,
};

fn eye_hit(f: Frame, ro: vec3<f32>, rd: vec3<f32>) -> Hit {
    var h: Hit;
    h.t = -1.0;
    h.cornea = false;
    let ts = sphere_hit(ro, rd, f.c, f.r);
    if (ts > 0.0 && dot(ro + rd * ts - f.c, f.g) < (LIMBUS_Z + 0.01) * f.r) {
        h.t = ts;
    }
    let cc = f.c + f.g * (CORNEA_C * f.r);
    let tc = sphere_hit(ro, rd, cc, CORNEA_R * f.r);
    if (tc > 0.0 && dot(ro + rd * tc - f.c, f.g) >= (LIMBUS_Z - 0.01) * f.r && (h.t < 0.0 || tc < h.t)) {
        h.t = tc;
        h.cornea = true;
    }
    return h;
}

fn eye_colour(f: Frame, k: u32, ro: vec3<f32>, rd: vec3<f32>, h: Hit) -> vec3<f32> {
    let p = ro + rd * h.t;
    // A pixel's width in world units here, for fading detail finer than it.
    let pw = pixel() * h.t / FOCAL;
    if (h.cornea) {
        return cornea_colour(f, k, p, rd, pw);
    }
    return white_colour(f, k, p, rd, pw);
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let p = centred(uv);
    if (length(p) > item(2u).w * 1.3 + pixel()) {
        return vec3<f32>(0.0);
    }
    let ro = CAMERA;
    let rd = normalize(vec3<f32>(p / FOCAL, -1.0));
    var best = 1e9;
    var kb = MAX_EYES;
    var hb: Hit;
    for (var k = 0u; k < eye_count(); k++) {
        let e = item(L0 + k);
        // A quick test against a sphere round the whole eye first.
        if (sphere_hit(ro, rd, e.xyz, e.w * 1.12) < 0.0) {
            continue;
        }
        let h = eye_hit(frame_of(k), ro, rd);
        if (h.t > 0.0 && h.t < best) {
            best = h.t;
            kb = k;
            hb = h;
        }
    }
    if (kb == MAX_EYES) {
        return vec3<f32>(0.0);
    }
    let f = frame_of(kb);
    var col = eye_colour(f, kb, ro, rd, hb);
    // The silhouette, smoothed over a pixel.
    let miss = length(cross(f.c - ro, rd));
    let pw = pixel() * best / FOCAL;
    col *= clamp((f.r - miss) / pw + 0.5, 0.0, 1.0);
    return col;
}

// ---------------------------------------------------------------- noise

// Value noise in 3D, -1..1, on an integer hash (a float hash keeps too few
// bits once the coordinates grow) with a quintic fade (no grid creases).
fn hash3i(p: vec3<i32>) -> f32 {
    var n = bitcast<u32>(p.x) * 1597334677u ^ bitcast<u32>(p.y) * 3812015801u ^ bitcast<u32>(p.z) * 2798796415u;
    n = (n ^ (n >> 15u)) * 2246822519u;
    n = (n ^ (n >> 13u)) * 3266489917u;
    n = n ^ (n >> 16u);
    return f32(n & 0xffffffu) / 16777215.0;
}

fn vnoise3(p: vec3<f32>) -> f32 {
    let i = vec3<i32>(floor(p));
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let a = mix(hash3i(i), hash3i(i + vec3<i32>(1, 0, 0)), u.x);
    let b = mix(hash3i(i + vec3<i32>(0, 1, 0)), hash3i(i + vec3<i32>(1, 1, 0)), u.x);
    let c = mix(hash3i(i + vec3<i32>(0, 0, 1)), hash3i(i + vec3<i32>(1, 0, 1)), u.x);
    let d = mix(hash3i(i + vec3<i32>(0, 1, 1)), hash3i(i + vec3<i32>(1, 1, 1)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z) * 2.0 - 1.0;
}
