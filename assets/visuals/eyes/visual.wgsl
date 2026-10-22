// Bug eyed: a cluster of eyeballs, traced per pixel (each a sphere facing
// the viewer, nearest in front, edges blended over the one behind). Every
// eye looks at the same point: it drifts to a new place every few seconds,
// jumps there on a sharp sound (Darting), and a share of new looks are
// straight at the viewer (Stare). Each beat makes a share of the eyes
// blink; each eye swells with one band (lows in the middle, where the
// cluster starts); the bass widens the pupils; in quiet passages the lids
// droop.
//
// Items:
//   0        (gaze point, last beat pulse)
//   1        (the point the gaze is heading for, time of the next drift)
//   2        (beats, pupil, how open the lids are, the cluster's reach), eased
//   E0 + k   eye k: (blink start time, its band's level eased, 0, 0)
//   L0 + k   eye k's place: (centre, radius), so each pixel only reads it

const E0: u32 = 3u;
const L0: u32 = 35u;
const MAX_EYES: u32 = 32u;
const VIEWER: vec3<f32> = vec3<f32>(0.0, 0.0, 4.0);

fn eye_count() -> u32 {
    return clamp(u32(params().count), 1u, MAX_EYES);
}

fn rand(k: u32, salt: f32) -> f32 {
    var n = k * 747796405u + bitcast<u32>(salt) * 2891336453u + 1u;
    n = ((n >> ((n >> 28u) + 4u)) ^ n) * 277803737u;
    n = (n >> 22u) ^ n;
    return f32(n & 0xffffffu) / 16777215.0;
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
            gaze = VIEWER;
        }
        // Eyes move fast: most of the way in a tenth of a second.
        gaze = mix(gaze, c1.xyz, 1.0 - exp(-dt / 0.035));
        return vec4<f32>(gaze, V.beat);
    }
    if (i == 1u) {
        let dart = V.hits.w > 0.5 && rand(7u, V.time) < P.darting * 0.5;
        if (dot(c1.xyz, c1.xyz) == 0.0 || V.time > c1.w || dart) {
            let s = V.time + 0.123;
            var aim = VIEWER;
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
        let pupil = ease(c2.y, clamp(broad_rel(BASS) - 0.6, 0.0, 1.5), 0.06, 0.4);
        let open = ease(select(c2.z, 1.0, c2.z <= 0.0), select(1.0, 0.72, V.quiet > 0.5), 0.5, 1.5);
        // How far from the middle any eye reaches (last frame's, padded).
        var reach = 0.0;
        for (var k = 0u; k < eye_count(); k++) {
            let e = item(L0 + k);
            reach = max(reach, length(e.xy) + e.w);
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
    let b = u32(f32(min(k, n - 1u)) / f32(max(n - 1u, 1u)) * 28.0);
    let level = ease(e.y, band(b), 0.03, 0.15);
    return vec4<f32>(start, level, 0.0, 0.0);
}

// Eye k: (centre, radius). A sunflower cluster from the middle out, the
// middle ones nearer the viewer.
fn eye_at(k: u32, n: u32) -> vec4<f32> {
    let P = params();
    let a = f32(k) * 2.39996 + 0.3;
    let rr = P.spread * sqrt((f32(k) + 0.5) / f32(n));
    let sway = 0.025 * vec2<f32>(sin(V.time * 0.37), sin(V.time * 0.29 + 1.0));
    let xy = vec2<f32>(cos(a) * 1.15, sin(a) * 0.85) * rr + sway;
    let base = P.spread * 0.92 / sqrt(f32(n));
    let r = base * mix(1.0, 0.6 + 0.8 * rand(k, 91.0), P.sizes) * (1.0 + P.bounce * 0.12 * item(E0 + k).y);
    let z = 0.4 * P.spread * (1.0 - rr * rr / max(P.spread * P.spread, 1e-3));
    return vec4<f32>(xy, z, r);
}

fn eye_colour(k: u32, p: vec2<f32>) -> vec3<f32> {
    let P = params();
    let e = item(L0 + k);
    let d = (p - e.xy) / e.w;
    let nz = sqrt(max(1.0 - dot(d, d), 0.0));
    let nrm = vec3<f32>(d, nz);
    let w = 1.5 * pixel() / e.w;
    // Where it looks.
    let g = normalize(item(0u).xyz - e.xyz);
    let ang = acos(clamp(dot(nrm, g), -1.0, 1.0));
    let a_iris = mix(0.38, 0.72, P.iris);
    let a_pupil = a_iris * clamp(0.32 + 0.28 * P.pupil * item(2u).y, 0.2, 0.85);
    // The white, its veins toward the rim.
    var white = vec3<f32>(0.95, 0.93, 0.9) * mix(vec3<f32>(1.0), vec3<f32>(1.0, 0.78, 0.78), 0.5 * smoothstep(1.0, 1.7, ang));
    if (P.veins > 0.0) {
        let v = abs(noise(d * 4.5 + vec2<f32>(f32(k) * 7.1, 3.0)) * 2.0 - 1.0);
        let vein = (1.0 - smoothstep(0.0, 0.07, v)) * smoothstep(0.8, 1.6, ang) * P.veins;
        white = mix(white, vec3<f32>(0.7, 0.06, 0.08), vein);
    }
    // The iris: fibres round the pupil, darker at its rim.
    let tone = select(rand(k, 17.0), 0.55, P.colouring > 0.5);
    let u = normalize(cross(g, vec3<f32>(0.0, 1.0, 0.0)) + vec3<f32>(1e-4, 0.0, 0.0));
    let vv = cross(g, u);
    let around = normalize(vec2<f32>(dot(nrm, u), dot(nrm, vv)) + vec2<f32>(1e-5));
    let t = clamp((ang - a_pupil) / max(a_iris - a_pupil, 1e-3), 0.0, 1.0);
    let fibre = noise(around * 9.0 + vec2<f32>(t * 1.2, f32(k) * 3.3));
    var iris = colour(tone) * (0.45 + 0.9 * fibre) * (1.2 - 0.5 * t) * (1.0 - 0.75 * smoothstep(0.75, 1.0, t));
    var col = mix(white, iris, smoothstep(a_iris + w, a_iris - w, ang));
    col = mix(col, vec3<f32>(0.01), smoothstep(a_pupil + w, a_pupil - w, ang));
    // Light from above left, and a wet glint.
    let light = normalize(vec3<f32>(-0.4, 0.5, 0.75));
    col = col * (0.3 + 0.7 * max(dot(nrm, light), 0.0));
    let glint = pow(max(dot(nrm, normalize(light + vec3<f32>(0.0, 0.0, 1.0))), 0.0), 90.0) * 1.5;
    if (P.lids < 0.5) {
        return col + vec3<f32>(glint);
    }
    // The lids, tipped a little per eye, closing to the middle on a blink.
    let since = V.time - item(E0 + k).x;
    let shut = smoothstep(0.0, 0.05, since) * (1.0 - smoothstep(0.09, 0.2, since));
    let open = item(2u).z * (1.0 - shut);
    let tip = (rand(k, 23.0) - 0.5) * 0.4;
    let ny = d.y * cos(tip) + d.x * sin(tip);
    let nx = d.x * cos(tip) - d.y * sin(tip);
    let top = mix(-0.04, 0.8, open) - 0.5 * nx * nx;
    let bottom = mix(-0.06, -0.8, open) + 0.4 * nx * nx;
    let lid = max(smoothstep(top - w, top + w, ny), smoothstep(bottom + w, bottom - w, ny));
    // The upper lid shades the eye just under it.
    col = col * (1.0 - 0.5 * (1.0 - smoothstep(0.0, 0.15, top - ny)));
    col = col + vec3<f32>(glint) * (1.0 - lid);
    var skin = (colour(tone) * 0.1 + vec3<f32>(0.035, 0.025, 0.03)) * (0.3 + 0.7 * nz);
    let lash = 1.0 - smoothstep(0.0, 0.08, min(abs(ny - top), abs(ny - bottom)));
    skin = skin * (1.0 - 0.7 * lash);
    return mix(col, skin, lid);
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let p = centred(uv);
    let n = eye_count();
    let px = pixel();
    if (length(p) > item(2u).w + px) {
        return vec3<f32>(0.0);
    }
    // The two nearest eyes under this pixel, and how much each covers it.
    var z1 = -1e9;
    var k1 = MAX_EYES;
    var cov1 = 0.0;
    var z2 = -1e9;
    var k2 = MAX_EYES;
    var cov2 = 0.0;
    for (var k = 0u; k < n; k++) {
        let e = item(L0 + k);
        let dist = length(p - e.xy);
        if (dist < e.w + px) {
            let nz = sqrt(max(1.0 - dist * dist / (e.w * e.w), 0.0));
            let z = e.z + e.w * nz;
            let cov = clamp((e.w - dist) / px + 0.5, 0.0, 1.0);
            if (z > z1) {
                z2 = z1;
                k2 = k1;
                cov2 = cov1;
                z1 = z;
                k1 = k;
                cov1 = cov;
            } else if (z > z2) {
                z2 = z;
                k2 = k;
                cov2 = cov;
            }
        }
    }
    var col = vec3<f32>(0.0);
    if (k2 < MAX_EYES && cov1 < 1.0) {
        col = eye_colour(k2, p) * cov2;
    }
    if (k1 < MAX_EYES) {
        col = mix(col, eye_colour(k1, p), cov1);
    }
    return col;
}
