// A lion's mane's central mass: its four oral arms and four gonads hang
// from under its bell as one bulky body of folded tissue, far from
// see-through (the glass species' arms are sheets; see mass_look). A closed
// column on the arms' links (their middle: it sways as they do),
// MASS_AROUND x MASS_ALONG quads of the arms' share of the triangles: eight
// bodies round the mouth, arms and gonads by turns, joined by a curtain
// near the bell and parting lower into folded lobes with ruffled edges; the
// gonads end sooner. All of it slowly drifting.
const MASS_AROUND: u32 = 64u;
const MASS_ALONG: u32 = 22u;        // MASS_AROUND * MASS_ALONG <= ARM_CORNERS / 6
const MASS_REACH: f32 = 0.45;       // how much of the arms' length it hangs: most of a bell across
const MASS_CORE: f32 = 0.35;        // the column's share of the mass's width: the billows are the rest

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

// How much of an arm (1) or a gonad (0) the mass is round th.
fn mass_arm(th: f32) -> f32 {
    return 0.5 + 0.5 * cos(th * TAU * 4.0);
}

// Body i of the eight (0, 2, 4, 6 arms; 1, 3, 5, 7 gonads): its length
// (in u), width, the phase of its billow, and how much of an arm it is.
fn mass_body(i: f32, seed: f32) -> vec4<f32> {
    let n = i - 8.0 * floor(i / 8.0);
    let arm = select(0.0, 1.0, u32(n) % 2u == 0u);
    let h = hash22(vec2<f32>(n, seed * 3.0));
    return vec4<f32>(mix(0.6 + 0.14 * h.x, 0.88 + 0.12 * h.x, arm), mix(0.85, 1.08, h.y), h.x * TAU, arm);
}

// How far out the mass's surface is at u (0..1 down it), round th (0..1),
// in bell radii, and the fold there (-1 a hollow, 1 a ridge).
fn mass_radius(u: f32, th: f32, seed: f32) -> vec2<f32> {
    let g = groove_clock();
    // Each body drifts round a little on its own.
    let a = th * TAU + 0.08 * sin(u * 3.0 + g * 0.15 + seed);
    // This body's own length, width and billow, blended into its
    // neighbours' between them (so no seam where they meet).
    let f = th * 8.0;
    let i0 = floor(f);
    let B = mix(mass_body(i0, seed), mass_body(i0 + 1.0, seed), smoothstep(0.0, 1.0, f - i0));
    // Gathered under the bell, swelling to most of its width and hanging
    // broad, each body narrowing toward its own end (the gonads end sooner
    // than the arms), closed off at the bottom.
    let prof = mix(0.15, 0.82, smoothstep(0.0, 0.12, u)) * (1.0 - 0.55 * smoothstep(0.35 * B.x, B.x, u)) * B.y * (1.0 - smoothstep(0.92, 1.0, u));
    // Eight bodies: the valleys between them shallow under the bell, where
    // a curtain joins them, deep lower down, where they part.
    let body = abs(cos(a * 4.0));
    let part = smoothstep(0.06, 0.34, u);
    let lobe = mix(1.0, 0.16 + 0.84 * pow(body, 0.55), part);
    // Each body swells in and out down its length; folds down it; big
    // ruffles along its edges, crowding toward its end (no finer than the
    // mesh's rows and columns can draw).
    let billow = sin(u * 5.0 + B.z + g * 0.25) * 0.12 * part * body;
    let fold = cos(a * 16.0 + 0.9 * sin(u * 6.0 - g * 0.3 + seed * 2.0));
    let frill = sin(u * 13.0 + a * 5.0 + B.z + g * 0.4) * (1.0 - body) * smoothstep(0.2, 0.9, u);
    let lump = noise(vec2<f32>(a * 1.3 + seed * 7.0, u * 5.0 - g * 0.15)) - 0.5;
    // (Never through the middle: a frill deeper than its valley turned the
    // surface inside out, and it showed through on the far side as shards.)
    let r = prof * max(lobe + billow + fold * 0.04 + frill * 0.26 * lobe + lump * 0.2, 0.12);
    let ridge = clamp(mix(fold * 0.5, body * 2.0 - 1.0, part * 0.7) + frill * 0.5, -1.0, 1.0);
    return vec2<f32>(r, ridge);
}

fn mass_point(j: u32, J: Jelly, u: f32, th: f32) -> vec3<f32> {
    let us = u * MASS_REACH;
    let at = mass_spine(j, us);
    let dir = normalize(mass_spine(j, us + 0.03) - mass_spine(j, max(us - 0.03, 0.0)) + vec3<f32>(1e-5, 0.0, 0.0));
    let fx = frame_x(J);
    let ex = normalize(fx - dir * dot(fx, dir) + vec3<f32>(0.0, 1e-5, 0.0));
    let ez = cross(dir, ex);
    let a = th * TAU;
    let r = mass_radius(u, th, J.hue * 5.0).x * radius(J) * params().arms * MASS_CORE;
    return at + (ex * cos(a) + ez * sin(a)) * r;
}

fn mass_vertex(j: u32, J: Jelly, cam: Camera, eye: vec3<f32>, quad: u32, da: u32, db: u32) -> MeshVertex {
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    // (The arms' share has a few quads more than the mass uses.)
    if (quad >= MASS_AROUND * MASS_ALONG) {
        return v;
    }
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

fn arm_paint(J: Jelly, k: u32, f: MeshFragment, fp: f32, fy: f32) -> vec4<f32> {
    // (Its fine detail fades by a pixel along the mass, fy: colour.zw are
    // its angle and fold, whose change says nothing of a pixel's size.)
    return mass_look(J, f.colour.y, f.colour.z, f.colour.w, f.data.x, f.data.y, f.data.z, fy);
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
    // The gonads paler, creamier, than the arms.
    let tissue = mix(mix(tone(ARMS_C, hue), vec3<f32>(1.0, 0.86, 0.66), 0.45), tone(ARMS_C, hue), mass_arm(th));
    let deep = mix(tone(ORGANS, hue), tone(ARMS_C, hue), 0.5) * 0.55;
    // (Waves at odd angles, not value noise: its grid showed as blocks.)
    let q = vec2<f32>(th * 26.0, u * 18.0);
    let crinkle = (0.5 + 0.25 * sin(q.x * 1.0 + q.y * 0.7 + J.hue * 9.0 + 2.0 * sin(q.y * 0.9)) + 0.25 * sin(q.x * 1.7 - q.y * 1.3 + 3.0 * sin(q.x * 0.4))) * keep_detail(fp * 40.0);
    // Finer frills than the mesh can shape, painted: wavy crinkles across
    // each body, crowding toward its edges and its end, each catching the
    // light on one side.
    let body = abs(cos(th * TAU * 4.0));
    let wav = u * 110.0 + 7.0 * noise(vec2<f32>(th * 40.0, u * 6.0) + J.hue * 4.0) + th * 13.0;
    let strength = mix(0.25, 1.0, 1.0 - body) * smoothstep(0.1, 0.6, u) * (0.4 + 0.8 * noise(vec2<f32>(th * 20.0, u * 9.0) + 7.0));
    let frills = pow(sin(wav) * 0.5 + 0.5, 5.0) * keep_detail(fp * 110.0) * strength;
    let ridge = smoothstep(-0.6, 0.9, pleat + 0.5 * (crinkle - 0.5) + 0.9 * frills - 0.12 * strength);
    let pale = mix(tissue, vec3<f32>(1.0, 0.82, 0.62), 0.35 * ridge);
    let col = mix(deep, pale, ridge);
    // (Light passes through it: in the photos the mass glows cream under the bell.)
    let lit = 0.26 + 0.22 * top + 0.14 * through + 0.1 * fresnel(face, 2.5);
    let inner = tone(ORGANS, hue) * 0.015 * (0.5 + soft(J.glow) * 1.5) * (1.0 - ridge);
    // (The core, deep in the billows: darker.)
    let c = (col * lit * (0.7 + 0.6 * crinkle) + inner) * (1.0 - 0.4 * u) * P.body * 0.85;
    let cover = 0.85 * (1.0 - 0.5 * smoothstep(0.75, 1.0, u)) * smoothstep(0.0, 0.04, u);
    return vec4<f32>(c, cover);
}
