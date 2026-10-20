// The lion's mane's mass as it looks in the water (photos 02, 19, 42): a
// dense, milky, billowing body, shaped like a thick fluid rolling into the
// water or a plume of smoke, but solid: the four oral arms' curtains and
// the gonads, pleated and frilled over and over, one fold hiding the next.
// Round a small core (mass.wgsl) LUMPS lumps hang along the arms' middle
// (mass_spine), each a sphere squashed and turned its own way and pushed
// out by rolling noise at two scales. They are real surfaces: they cover
// each other and the hair behind them by depth, from every side. Lit as
// milky tissue (the light scattered through it): a soft wrapped light from
// above, cream; orange where it is thin and the light comes through toward
// the eye (an edge, a bump, the underside seen from below); darker and
// warmer in the creases and deep in the mass. Each lives a slow cycle: it
// grows out of the mass from nothing, rolls out and down, and shrinks back
// in, so the mass churns and nothing pops.

const LUMPS: u32 = 40u;
const LUMP_LAT: u32 = 14u;
const LUMP_LON: u32 = 28u;
const LUMP_CORNERS: u32 = 2352u;     // LUMP_LAT * LUMP_LON * 6
const PLUME_CORNERS: u32 = 94080u;   // LUMPS * LUMP_CORNERS
const PART_LUMP: f32 = 80.0;         // PART_EXTRA + 16: a lump, not the mane's sheets

// Value noise in 3D, for the lumps' surfaces (the library's is 2D).
fn lump_hash(q: vec3<f32>) -> f32 {
    return fract(sin(dot(q, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

fn lump_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    let a = mix(lump_hash(i), lump_hash(i + vec3<f32>(1.0, 0.0, 0.0)), w.x);
    let b = mix(lump_hash(i + vec3<f32>(0.0, 1.0, 0.0)), lump_hash(i + vec3<f32>(1.0, 1.0, 0.0)), w.x);
    let c = mix(lump_hash(i + vec3<f32>(0.0, 0.0, 1.0)), lump_hash(i + vec3<f32>(1.0, 0.0, 1.0)), w.x);
    let d = mix(lump_hash(i + vec3<f32>(0.0, 1.0, 1.0)), lump_hash(i + vec3<f32>(1.0, 1.0, 1.0)), w.x);
    return mix(mix(a, b, w.y), mix(c, d, w.y), w.z);
}

// Lump b of a jelly: its middle and size (world units), its frame (out
// from the mass, down it, round it), where in its cycle (0..1), how far out
// in the mass (0 deep, 1 its surface), its own numbers.
struct Lump {
    at: vec3<f32>,
    size: f32,
    out: vec3<f32>,
    t: f32,
    down: vec3<f32>,
    deep: f32,
    round: vec3<f32>,
    seed: f32,
    shape: vec3<f32>,
};

fn lump_of(j: u32, J: Jelly, b: u32) -> Lump {
    let seed = J.hue * 5.0 + 80.0;
    let id = f32(b) + 0.5;
    let h = vec4<f32>(creature_hash(id, seed), creature_hash(id, seed + 1.0), creature_hash(id, seed + 2.0), creature_hash(id, seed + 3.0));
    let h2 = vec4<f32>(creature_hash(id, seed + 4.0), creature_hash(id, seed + 5.0), creature_hash(id, seed + 6.0), creature_hash(id, seed + 7.0));
    var L: Lump;
    L.seed = h.x * 97.0;
    // A slow cycle each, 25 to 50 seconds (quickening with the music).
    L.t = fract(groove_clock() * (0.02 + 0.02 * h.y) + h.x);
    // Down the mass, sinking a little as it rolls; round it, drifting.
    // (Spread evenly down it and round it, each its own within its share.)
    let u = clamp(0.04 + (f32(b % 8u) + h.z) / 8.0 * 0.86 + 0.1 * L.t, 0.0, 1.0);
    let us = u * MASS_REACH;
    let at = mass_spine(j, us);
    let dir = normalize(mass_spine(j, us + 0.03) - mass_spine(j, max(us - 0.03, 0.0)) + vec3<f32>(1e-5, 0.0, 0.0));
    let fx = frame_x(J);
    let ex = normalize(fx - dir * dot(fx, dir) + vec3<f32>(0.0, 1e-5, 0.0));
    let ez = cross(dir, ex);
    let a = (f32(b / 8u) + h.w) / f32(LUMPS / 8u) * TAU + 0.4 * (h2.x - 0.5) * L.t;
    let R = radius(J) * params().arms;
    L.out = ex * cos(a) + ez * sin(a);
    L.round = ez * cos(a) - ex * sin(a);
    L.down = dir;
    // Gathered under the bell, swelling to most of its width, narrowing
    // toward the end; rolled outward over its cycle.
    let width = mix(0.6, 1.0, smoothstep(0.0, 0.2, u)) * (1.0 - 0.5 * smoothstep(0.4, 1.0, u));
    L.deep = clamp(0.25 + 0.6 * sqrt(h2.y) + 0.2 * L.t, 0.0, 1.0);
    L.at = at + L.out * (0.12 + 0.5 * L.deep) * width * R;
    // Growing from nothing and shrinking back into the mass; smaller toward
    // the end, where the mass thins into the hair.
    let grow = smoothstep(0.0, 0.3, L.t) * (1.0 - smoothstep(0.7, 1.0, L.t));
    L.size = R * (0.2 + 0.14 * h2.x) * (0.8 + 0.4 * L.t) * grow * mix(0.75, 1.0, smoothstep(0.0, 0.2, u)) * (1.0 - 0.4 * smoothstep(0.4, 1.0, u));
    // Each squashed its own way: thinner out from the mass, as a fold is,
    // longer down it or round it.
    L.shape = vec3<f32>(0.65 + 0.3 * h2.z, 0.9 + 0.4 * h2.w, 0.85 + 0.4 * h.y);
    return L;
}

// How far lump L's surface stands out along its own direction d (a unit
// vector in its frame), in its sizes: big rolling lumps and smaller ones on
// them, rolling outward and down with its cycle.
fn lump_relief(L: Lump, d: vec3<f32>) -> f32 {
    let roll = vec3<f32>(-L.t * 1.2, L.t * 0.8, 0.0);
    let s = vec3<f32>(L.seed, L.seed * 0.37, L.seed * 0.71);
    let big = lump_noise(d * 1.7 + s + roll) - 0.5;
    let small = lump_noise(d * 4.1 + s * 1.3 + roll * 2.0) - 0.5;
    return 0.42 * big + 0.16 * small;
}

fn lump_point(L: Lump, d: vec3<f32>) -> vec3<f32> {
    let r = L.size * (1.0 + lump_relief(L, d));
    let q = d * L.shape * r;
    return L.at + L.out * q.x + L.down * q.y + L.round * q.z;
}

fn lump_dir(lat: f32, lon: f32) -> vec3<f32> {
    // (Held off the poles, where the grid closes up and has no slope.)
    let th = mix(0.02, 0.98, lat) * TAU * 0.5;
    let ph = lon * TAU;
    return vec3<f32>(sin(th) * cos(ph), cos(th), sin(th) * sin(ph));
}

fn lump_vertex(j: u32, J: Jelly, cam: Camera, l: u32, da: array<u32, 6>, db: array<u32, 6>, c6: u32) -> MeshVertex {
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    if (params().arms < 0.01) {
        return v;
    }
    let L = lump_of(j, J, l / LUMP_CORNERS);
    if (L.size <= 0.0) {
        return v;
    }
    let quad = (l % LUMP_CORNERS) / 6u;
    let lat = f32(quad / LUMP_LON + da[c6]) / f32(LUMP_LAT);
    let lon = f32(quad % LUMP_LON + db[c6]) / f32(LUMP_LON);
    let d = lump_dir(lat, lon);
    let wp = lump_point(L, d);
    // The normal from the surface's slopes, outward.
    let e = 0.02;
    let pa = lump_point(L, lump_dir(lat + e, lon)) - lump_point(L, lump_dir(lat - e, lon));
    let pb = lump_point(L, lump_dir(lat, lon + e)) - lump_point(L, lump_dir(lat, lon - e));
    var n = cross(pb, pa);
    let radial = normalize(wp - L.at);
    n = select(radial, normalize(n), dot(n, n) > 1e-12);
    if (dot(n, radial) < 0.0) {
        n = -n;
    }
    let eye = camera_position(cam);
    let V = normalize(eye - wp);
    let Ld = normalize(vec3<f32>(0.25, 1.0, 0.15));
    // Wrapped light from above: soft, waxy.
    let wrap = clamp((dot(n, Ld) + 0.5) / 1.5, 0.0, 1.0);
    // How thin it is here: at its edges seen from the eye, on its bumps.
    let relief = lump_relief(L, d);
    let crease = clamp((-relief - 0.06) * 6.0, 0.0, 1.0);
    let thin = (1.0 - clamp(dot(n, V), 0.0, 1.0)) * (0.5 + 0.5 * clamp(relief * 3.0 + 0.5, 0.0, 1.0));
    // The light coming through it toward the eye (Barre-Brisebois and
    // Bouchard's fast translucency), more where it is thin.
    let through = pow(clamp(dot(V, -(Ld + n * 0.4)), 0.0, 1.0), 3.0) * (0.3 + 0.7 * thin) + 0.25 * thin;
    // Darker deep in the mass and in its creases.
    let ao = mix(0.45, 1.0, L.deep) * (1.0 - 0.45 * crease);
    v.pos = clip_pos(cam, wp);
    v.colour = vec4<f32>(to_camera(cam, wp).z, wrap * ao, through, crease);
    // (Where on it, for finer relief painted per pixel: its own place, so the
    // detail rolls with it and never swims.)
    v.data = vec4<f32>(d * 3.0 + vec3<f32>(L.seed, L.seed * 0.37, L.seed * 0.71) * 1.3, f32(j) + 0.5 + PART_LUMP);
    return v;
}

// A lump's look: cream where the light from above falls on it, orange where
// the light comes through, deeper orange in its creases, a warm floor so
// its underside never goes black; finer relief painted over it.
fn lump_look(J: Jelly, lit: f32, through: f32, crease: f32, q: vec3<f32>) -> vec3<f32> {
    let P = params();
    let hue = hue_of(J);
    let cream = mix(tone(ARMS_C, hue), vec3<f32>(1.0, 0.92, 0.8), 0.65);
    let warm = mix(tone(ARMS_C, hue), tone(ORGANS, hue), 0.45);
    let fine = 0.65 * lump_noise(q * 2.3) + 0.35 * lump_noise(q * 5.1);
    let b = lit * (0.75 + 0.5 * fine);
    // (The light scattered inside it is most of what it shows: it glows,
    // cream where the light from above falls, orange elsewhere.)
    var c = mix(warm, cream, clamp(b * 1.3, 0.0, 1.0)) * (0.16 + 0.5 * b);
    c += warm * through * 0.5;
    c = mix(c, warm * (0.07 + 0.15 * lit), crease * 0.6);
    return c * P.body;
}

fn lump_paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    let J = jelly_at(u32(f.data.w) % 16u);
    let m = lump_look(J, f.colour.y, f.colour.z, f.colour.w, f.data.xyz);
    let c = water(shown(L_ARMS, m), f.colour.x, eye_near(), natural_bodies());
    let cover = water_keep(f.colour.x, eye_near()) * select(0.0, 1.0, layer_on(L_ARMS));
    // (Solid, from a half, it was drawn in the cover pass.)
    if (cover_pass() && !covering() && cover >= 0.5) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(c * P.glow, cover);
}
