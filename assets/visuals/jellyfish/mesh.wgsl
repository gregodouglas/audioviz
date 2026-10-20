// The triangles: for each jelly, its bell's two skins (the outer
// exumbrella, the inner subumbrella under the jelly, each RINGS x SECTORS
// quads on the bell's profile), then its oral arms (V-folded sheets along
// their links). What paint gets per corner:
//   bell  colour (distance from the eye, the stroke there, plan x, plan y)
//         data   (0, 0, 0, id): paint works the skin's light out per pixel
//                from the profile (skin_frame) and its own ray
//   arm   colour (distance from the eye, along 0..1, across -1..1, 0)
//         data   (light from above, light through it, face on, id)
//   glow  data   (x, y from its centre in sizes, 0, id)
//   mane  colour (distance from the eye, along 0..1, across its gap 0..1,
//                how far behind the body it hangs, in radii)
//         data   (which gap: cluster * 8 + row * 4 + gap, 0, 0, id)
// The plan position (crown in the middle, margin 1 out) interpolates cleanly
// where an angle round would kink across a quad's diagonal. The id is
// j + 0.5, plus 16 for the inner skin, 32 for an arm, 48 for the glow, 64
// for the mane.

const PART_INNER: f32 = 16.0;
const PART_ARM: f32 = 32.0;
const PART_GLOW: f32 = 48.0;
const PART_MANE: f32 = 64.0;
// How far the glow's quad reaches, in sizes of its glow.
const GLOW_REACH: f32 = 4.0;

// A lion's mane's central mass: the oral arms and gonads hang from under
// its bell as one bulky body of folded tissue, far from see-through (the
// other species' arms are sheets; see paint's mass_look). A closed column
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

// A lion's mane's sheet of hair, corner l of the sheets (a quad a link,
// across the gap between two neighbouring hairs of a cluster's outer or
// inner row): its corners are the hairs' own links, so it moves with them.
// The line hairs alone read as wires; the sheets give the mane its mass.
fn mane_vertex(j: u32, J: Jelly, cam: Camera, l: u32, da: array<u32, 6>, db: array<u32, 6>, c6: u32) -> MeshVertex {
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    let n = strand_count();
    if (kind() != LION || n < 16u) {
        return v;
    }
    let quad = l / 6u;
    let link = quad % (SEG - 1u);
    let sheet = quad / (SEG - 1u);
    let gap = sheet % MANE_GAPS;
    let row = select(0u, 3u, (sheet / MANE_GAPS) % 2u == 1u);
    let cluster = sheet / (MANE_GAPS * 2u);
    let a = gap + db[c6];
    let s = cluster + 8u * (row + 4u * a);
    let k = link + da[c6];
    if (s >= n) {
        return v;
    }
    let p3 = select(item(STRAND0 + (j * STRANDS + s) * SEG + k).xyz, to_world(J, strand_root(j, s, n).pos), k == 0u);
    v.pos = clip_pos(cam, p3);
    let behind = (to_camera(cam, p3).z - to_camera(cam, J.pos).z) / radius(J);
    v.colour = vec4<f32>(to_camera(cam, p3).z, f32(k) / f32(SEG - 1u), f32(db[c6]), behind);
    v.data = vec4<f32>(f32(cluster * 8u + row + gap), 0.0, 0.0, f32(j) + 0.5 + PART_MANE);
    return v;
}

// A jelly's triangles in the order they draw, far to near, for the parts
// that cover what is behind them (a lion's mane's: see body_cover): seen
// from below, the outer skin, the inner skin, the arms (as they are laid
// out); seen from above, the arms, the inner skin, the outer skin.
fn draw_order(l: u32, above: bool) -> u32 {
    if (!above || l >= HALO_AT) {
        return l;
    }
    if (l < ARM_CORNERS) {
        return BELL_CORNERS + l;
    }
    let m = l - ARM_CORNERS;
    return select(m - SHELL_CORNERS, SHELL_CORNERS + m, m < SHELL_CORNERS);
}

fn vertex(vi: u32) -> MeshVertex {
    let P = params();
    let j = vi / CORNERS_J;
    let J = jelly_at(j);
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    if (j >= jelly_count() || J.age < 1.5) {
        return v;
    }
    let cam = the_camera();
    let eye = camera_position(cam);
    let l = draw_order(vi % CORNERS_J, dot(eye - J.pos, J.axis) > 0.0);
    let k = kind();
    let tri = l / 3u;
    let corner = l % 3u;
    // Corner offsets of a quad's two triangles.
    var da = array<u32, 6>(0u, 1u, 1u, 0u, 1u, 0u);
    var db = array<u32, 6>(0u, 0u, 1u, 0u, 1u, 1u);
    let c6 = (tri % 2u) * 3u + corner;
    if (l < BELL_CORNERS) {
        let shell = l / SHELL_CORNERS;
        let quad = (l % SHELL_CORNERS) / 6u;
        let n = quad / SECTORS + da[c6];
        let th = f32(quad % SECTORS + db[c6]) / f32(SECTORS);
        let sk = jelly_skin(j, n, th, shell == 1u);
        let wp = to_world(J, sk.pos);
        v.pos = clip_pos(cam, wp);
        v.data = vec4<f32>(0.0, 0.0, 0.0, f32(j) + 0.5 + PART_INNER * f32(shell));
        let plan = f32(n) / f32(RINGS) * vec2<f32>(cos(th * TAU), sin(th * TAU));
        v.colour = vec4<f32>(to_camera(cam, wp).z, profile(j, n).w, plan);
        return v;
    }
    if (l >= MANE_AT) {
        return mane_vertex(j, J, cam, l - MANE_AT, da, db, c6);
    }
    if (l >= HALO_AT) {
        // Its glow in the water: a quad facing the eye round where its bell
        // is on screen (a sprite: only the pixels near it pay for it).
        let a = item(HALO0 + 2u * j);
        if (a.w <= 0.0 || P.halo <= 0.001) {
            return v;
        }
        var qx = array<f32, 6>(-1.0, 1.0, 1.0, -1.0, 1.0, -1.0);
        var qy = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
        let at = vec2<f32>(qx[l - HALO_AT], qy[l - HALO_AT]) * GLOW_REACH;
        v.pos = vec4<f32>(a.xy + at * a.z, 0.0, 1.0);
        v.data = vec4<f32>(at, 0.0, f32(j) + 0.5 + PART_GLOW);
        return v;
    }
    // An oral arm: a sheet along a smooth curve through its links (ARM_SUB
    // rows a link, so its outline does not kink at each link), folded into
    // a V that opens away from the axis, its lips ruffled and the whole arm
    // twisting a little down its length. The lips of a moon jelly, a sea
    // nettle and a lion's mane are frilled curtains: a row of frills along
    // each edge, modelled; the finer crenulation is painted.
    let sp = species_of(k);
    if (sp.arm_len <= 0.0 || P.arms < 0.01) {
        return v;
    }
    if (k == LION) {
        return mass_vertex(j, J, cam, eye, (l - BELL_CORNERS) / 6u, da[c6], db[c6]);
    }
    let quad = (l - BELL_CORNERS) / 6u;
    let per_arm = (ARM_SEG - 1u) * ARM_SUB * ARM_ACROSS;
    let a = quad / per_arm;
    let row = (quad % per_arm) / ARM_ACROSS + da[c6];
    let xi = (quad % ARM_ACROSS) + db[c6];
    let x = f32(xi) / f32(ARM_ACROSS) * 2.0 - 1.0;
    let base = ARM0 + (j * ARMS + a) * ARM_SEG;
    // Catmull-Rom through the links either side of this row.
    let t = f32(row) / f32(ARM_SUB);
    let k1 = min(u32(t), ARM_SEG - 2u);
    let ft = t - f32(k1);
    let p0 = item(base + max(k1, 1u) - 1u).xyz;
    let p1 = item(base + k1).xyz;
    let p2 = item(base + k1 + 1u).xyz;
    let p3 = item(base + min(k1 + 2u, ARM_SEG - 1u)).xyz;
    let c1 = p2 - p0;
    let c2 = 2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3;
    let c3 = 3.0 * (p1 - p2) + p3 - p0;
    let at = p1 + 0.5 * ft * (c1 + ft * (c2 + ft * c3));
    let dir = normalize(c1 + ft * (2.0 * c2 + 3.0 * ft * c3) + vec3<f32>(1e-6, 0.0, 0.0));
    let u = t / f32(ARM_SEG - 1u);
    let moon = k == MOON;
    let tha = ((f32(a) + select(0.5, 0.0, moon)) / f32(ARMS) + u * 0.08) * TAU;
    let radial = dir_to_world(J, vec3<f32>(cos(tha), 0.0, sin(tha)));
    // Which way the arm opens: out from the axis, square to the arm. The
    // radial direction with the arm's projected out fails as the arm swings
    // into line with it (the frame flipped in one frame); the tangent round
    // the bell crossed with the arm gives the same direction and fails only
    // where the first is strong, so their sum is always well defined.
    let round = dir_to_world(J, vec3<f32>(-sin(tha), 0.0, cos(tha)));
    let e_out = normalize(radial - dir * dot(radial, dir) + cross(round, dir) + vec3<f32>(0.0, 1e-6, 0.0));
    let e_side = cross(dir, e_out);
    // A moon jelly's arm narrows a little and ends in a rounded tip.
    let tip = sqrt(max(1.0 - pow(smoothstep(0.7, 1.0, u), 2.0), 0.0));
    var width = sp.arm_w * radius(J) * P.arms * select(mix(1.0, 0.45, u), mix(1.0, 0.6, u) * max(tip, 0.12), moon);
    var fold = select(0.25, 0.55, moon);
    if (k == LION) {
        // Broad curtains, deeply folded, hardly narrowing.
        width = sp.arm_w * radius(J) * P.arms * mix(0.6, 1.0, smoothstep(0.0, 0.3, u)) * mix(1.0, 0.7, u);
        fold = 0.6;
    }
    // The lips: a slow ruffle and a slower, irregular wave, strongest at the edges.
    let lip = x * x;
    // Each lip its own ruffle, so the two edges of an arm are not mirrors.
    let sd = select(0.0, 1.7, x > 0.0);
    let ruffle = sin(u * 9.0 + f32(a) * 2.3 + sd + groove_clock() * 0.9) * 0.6 + sin(u * 4.0 - groove_clock() * 0.5 + f32(a) + sd * 2.0) * 0.4;
    // The frills: a row of waves along each edge, out of the sheet, each
    // lip its own, irregular in size and spacing, slowly moving.
    // (Frills along an arm: about five a bell radius of it, as many as its
    // rows can draw smoothly.)
    var frills = 0.0;
    var tall = 0.16;
    switch (k) {
        case 0u: { frills = 5.0; }
        case 1u: { frills = 10.0; tall = 0.22; }
        case 5u: { frills = 6.0; tall = 0.2; }
        default: {}
    }
    var frill = 0.0;
    if (frills > 0.0) {
        let edge = pow(smoothstep(0.5, 1.0, abs(x)), 2.0);
        let ph = u * TAU * frills + 1.0 * sin(u * TAU * 2.3 + f32(a) * 1.9 + sd) + sd * 2.1 + f32(a) * 1.3 + groove_clock() * 0.6;
        frill = sin(ph) * edge * (tall + 0.1 * sin(u * 17.0 + f32(a) + sd)) * select(1.0, min(tip * 2.0, 1.0), moon);
    }
    let wp = at + e_side * x * width + e_out * (abs(x) * fold + lip * ruffle * select(0.35, 0.4, moon) + frill) * width;
    let wn = normalize(cross(dir, e_side + e_out * fold * sign(x)) + vec3<f32>(0.0, 1e-6, 0.0));
    let lit = key_light(wn, normalize(wp - eye));
    v.pos = clip_pos(cam, wp);
    v.data = vec4<f32>(lit, facing(cam, wp, wn), f32(j) + 0.5 + PART_ARM);
    v.colour = vec4<f32>(to_camera(cam, wp).z, u, x, 0.0);
    return v;
}
