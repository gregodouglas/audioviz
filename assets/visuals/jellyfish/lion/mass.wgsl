// A lion's mane's central mass: the oral arms and gonads hang from under
// its bell as one bulky body of folded tissue, far from see-through (the
// glass species' arms are sheets; see mass_look). A closed column
// on the arms' links (their middle: it sways as they do), MASS_AROUND x
// MASS_ALONG quads in the arms' share of the triangles: it bulges below
// the bell and tapers, cut into deep lobes that open toward its end and
// finer pleats, all slowly drifting.
const MASS_AROUND: u32 = 48u;
const MASS_ALONG: u32 = 30u;        // MASS_AROUND * MASS_ALONG = ARM_CORNERS / 6
const MASS_REACH: f32 = 0.95;       // how much of the arms' length it hangs

// The middle of the arms at u (0 the mouth, 1 their ends).
fn mass_spine(j: u32, u: f32) -> vec3<f32> {
    let t = clamp(u, 0.0, 1.0) * f32(ARM_SEG - 1u);
    let k1 = min(u32(t), ARM_SEG - 2u);
    let ft = t - f32(k1);
    var p = array<vec3<f32>, 4>(vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0));
    for (var a = 0u; a < ARMS; a++) {
        let base = ARM0 + (j * ARMS + a) * ARM_SEG;
        p[0] += item(base + max(k1, 1u) - 1u).xyz;
        p[1] += item(base + k1).xyz;
        p[2] += item(base + k1 + 1u).xyz;
        p[3] += item(base + min(k1 + 2u, ARM_SEG - 1u)).xyz;
    }
    let c1 = (p[2] - p[0]) * 0.25;
    let c2 = (2.0 * p[0] - 5.0 * p[1] + 4.0 * p[2] - p[3]) * 0.25;
    let c3 = (3.0 * (p[1] - p[2]) + p[3] - p[0]) * 0.25;
    return p[1] * 0.25 + 0.5 * ft * (c1 + ft * (c2 + ft * c3));
}

// How far out the mass's surface is at u (0..1 down it), round th (0..1),
// in bell radii, and the pleat there (-1 a fold's hollow, 1 its ridge).
fn mass_radius(u: f32, th: f32, seed: f32) -> vec2<f32> {
    let g = groove_clock();
    let a = th * TAU;
    // Gathered under the bell, swelling to nearly its width, tapering.
    let prof = mix(0.12, 0.85, smoothstep(0.0, 0.14, u)) * mix(1.0, 0.4, smoothstep(0.5, 1.0, u));
    // Five billowing curtains, each its own way, deepening down the mass.
    let w = a * 2.5 + 0.9 * sin(u * 3.5 + g * 0.2 + seed) + 0.4 * sin(a * 3.0 + seed);
    let lobes = (abs(cos(w)) - 0.5) * (0.2 + 0.45 * smoothstep(0.1, 0.8, u));
    // Folds down them, and ruffles that crowd toward the end.
    let pleat = cos(a * 11.0 + u * 5.0 + 0.8 * sin(u * 8.0 - g * 0.3 + seed * 2.0));
    // (No finer than its rows can draw: finer ruffles aliased into blocks.)
    let ruffle = sin(u * 12.0 + a * 3.0 + seed + g * 0.4) * smoothstep(0.35, 1.0, u);
    let lump = noise(vec2<f32>(a * 1.3 + seed * 7.0, u * 5.0 - g * 0.15)) - 0.5;
    return vec2<f32>(prof * (1.0 + lobes + pleat * 0.08 + ruffle * 0.12 + lump * 0.35), clamp(pleat * 0.7 + ruffle * 0.5, -1.0, 1.0));
}

fn mass_point(j: u32, J: Jelly, u: f32, th: f32) -> vec3<f32> {
    let us = u * MASS_REACH;
    let at = mass_spine(j, us);
    let dir = normalize(mass_spine(j, us + 0.03) - mass_spine(j, max(us - 0.03, 0.0)) + vec3<f32>(1e-5, 0.0, 0.0));
    let fx = frame_x(J);
    let ex = normalize(fx - dir * dot(fx, dir) + vec3<f32>(0.0, 1e-5, 0.0));
    let ez = cross(dir, ex);
    let a = th * TAU;
    let r = mass_radius(u, th, J.hue * 5.0).x * radius(J) * params().arms;
    return at + (ex * cos(a) + ez * sin(a)) * r;
}

fn mass_vertex(j: u32, J: Jelly, cam: Camera, eye: vec3<f32>, quad: u32, da: u32, db: u32) -> MeshVertex {
    var v: MeshVertex;
    let u = f32(quad / MASS_AROUND + da) / f32(MASS_ALONG);
    let th = f32(quad % MASS_AROUND + db) / f32(MASS_AROUND);
    let wp = mass_point(j, J, u, th);
    // The normal from the surface's slopes, outward.
    let du = mass_point(j, J, u + 0.01, th) - mass_point(j, J, max(u - 0.01, 0.0), th);
    let dt = mass_point(j, J, u, th + 0.004) - mass_point(j, J, u, th - 0.004);
    var wn = normalize(cross(dt, du) + vec3<f32>(0.0, 1e-6, 0.0));
    if (dot(wn, wp - mass_spine(j, u * MASS_REACH)) < 0.0) {
        wn = -wn;
    }
    let lit = key_light(wn, normalize(wp - eye));
    v.pos = clip_pos(cam, wp);
    v.data = vec4<f32>(lit, facing(cam, wp, wn), f32(j) + 0.5 + PART_ARM);
    v.colour = vec4<f32>(to_camera(cam, wp).z, u, th, mass_radius(u, th, J.hue * 5.0).y);
    return v;
}

// The lion's mane's arms are its mass.
fn arms_vertex(j: u32, J: Jelly, cam: Camera, eye: vec3<f32>, l: u32, da: array<u32, 6>, db: array<u32, 6>, c6: u32) -> MeshVertex {
    return mass_vertex(j, J, cam, eye, l / 6u, da[c6], db[c6]);
}

fn arm_paint(J: Jelly, k: u32, f: MeshFragment, fp: f32) -> vec4<f32> {
    return mass_look(J, f.colour.y, f.colour.z, f.colour.w, f.data.x, f.data.y, f.data.z, fp);
}

// A lion's mane's central mass at u (0..1 down it), th round it, `pleat`
// (-1 a fold's hollow, 1 its ridge), lit from above (`top`), the light
// through it (`through`), how face on (`face`): dense folded tissue, cream
// to orange on the ridges and deep red in the folds, a crinkle of finer
// folds over it, glowing a little from inside with its light; it hides
// most of what is behind it, less toward its ragged end.
fn mass_look(J: Jelly, u: f32, th: f32, pleat: f32, top: f32, through: f32, face: f32, fp: f32) -> vec4<f32> {
    let P = params();
    let hue = hue_of(J);
    let tissue = tone(ARMS_C, hue);
    let deep = tone(ORGANS, hue) * 0.5;
    // (Waves at odd angles, not value noise: its grid showed as blocks.)
    let q = vec2<f32>(th * 26.0, u * 18.0);
    let crinkle = (0.5 + 0.25 * sin(q.x * 1.0 + q.y * 0.7 + J.hue * 9.0 + 2.0 * sin(q.y * 0.9)) + 0.25 * sin(q.x * 1.7 - q.y * 1.3 + 3.0 * sin(q.x * 0.4))) * keep_detail(fp * 40.0);
    let ridge = smoothstep(-0.6, 0.9, pleat + 0.5 * (crinkle - 0.5));
    let pale = mix(tissue, vec3<f32>(1.0, 0.82, 0.62), 0.35 * ridge);
    let col = mix(deep, pale, ridge);
    let lit = 0.05 + 0.12 * top + 0.06 * through + 0.05 * fresnel(face, 2.5);
    let inner = tone(ORGANS, hue) * 0.015 * (0.5 + soft(J.glow) * 1.5) * (1.0 - ridge);
    let c = (col * lit * (0.7 + 0.6 * crinkle) + inner) * (1.0 - 0.4 * u) * P.body;
    let cover = 0.85 * (1.0 - 0.5 * smoothstep(0.75, 1.0, u)) * smoothstep(0.0, 0.04, u);
    return vec4<f32>(c, cover);
}
