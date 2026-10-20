// The glass species' oral arms (moon jelly, sea nettle, crystal jelly; the
// atolla and comb jelly have none): what the shared mesh and paint ask of
// a variant.

const CORNERS_J: u32 = 27078u;      // EXTRA_AT: nothing past the glow

// Corner l of a jelly's arms (from 0 at BELL_CORNERS).
fn arms_vertex(j: u32, J: Jelly, cam: Camera, eye: vec3<f32>, l: u32, da: array<u32, 6>, db: array<u32, 6>, c6: u32) -> MeshVertex {
    let P = params();
    let k = kind();
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    let sp = species_of(k);
    // An oral arm: a sheet along a smooth curve through its links (ARM_SUB
    // rows a link, so its outline does not kink at each link), folded into
    // a V that opens away from the axis, its lips ruffled and the whole arm
    // twisting a little down its length. The lips of a moon jelly and a sea
    // nettle are frilled curtains: a row of frills along each edge,
    // modelled; the finer crenulation is painted.
    let quad = l / 6u;
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
    let width = sp.arm_w * radius(J) * P.arms * select(mix(1.0, 0.45, u), mix(1.0, 0.6, u) * max(tip, 0.12), moon);
    let fold = select(0.25, 0.55, moon);
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

// Nothing past the glow.
fn extra_vertex(j: u32, J: Jelly, cam: Camera, l: u32, da: array<u32, 6>, db: array<u32, 6>, c6: u32) -> MeshVertex {
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    return v;
}

fn extra_paint(f: MeshFragment) -> vec4<f32> {
    return vec4<f32>(0.0);
}

// An arm's light; clear jelly covers nothing.
fn arm_paint(J: Jelly, k: u32, f: MeshFragment, fp: f32) -> vec4<f32> {
    return vec4<f32>(arm_look(J, k, f.colour.y, f.colour.z, f.data.z, f.data.x, fp), 0.0);
}

// An oral arm at u (root 0 to tip 1), x across (-1..1): clear jelly, the
// light in its crenulated lips and a groove down its fold.
fn arm_look(J: Jelly, k: u32, u: f32, x: f32, face: f32, top: f32, fu: f32) -> vec3<f32> {
    let P = params();
    let hue = hue_of(J);
    let c = mix(tone(ARMS_C, hue), tone(ORGANS, hue), 0.15 * (1.0 - u));
    let rim = fresnel(face, 2.0);
    // The lips: crinkled, irregular edges (each side its own), catching the
    // light where they fold; a lace of fine folds in from them; a milky
    // groove down the middle where the arm folds.
    let side = select(0.0, 9.0, x > 0.0);
    let fine = keep_detail(fu * 8.0);
    let crinkle = 0.65 * noise(vec2<f32>(u * 55.0, side + J.hue * 5.0)) + 0.35 * noise(vec2<f32>(u * 130.0, side + 3.0));
    let edge = 1.0 - abs(x) - 0.1 * crinkle * fine;
    let lip = smoothstep(0.13, 0.0, edge) * (0.45 + 0.8 * crinkle);
    let beads = mix(1.0, 0.6 + 0.6 * noise(vec2<f32>(u * 110.0, side + x * 3.0)), fine);
    // (The lace only on the moon jelly: every arm pixel pays for it.)
    var lace = 0.0;
    if (k == MOON) {
        lace = smoothstep(0.55, 0.05, edge) * (0.5 + 0.5 * noise(vec2<f32>(u * 70.0, x * 9.0 + side))) * 0.18 * fine;
    }
    let groove = thin_line(abs(x), 0.08, fu * 4.0) * 0.22 + lace;
    let tissue = 0.7 + 0.6 * fbm(vec2<f32>(u * 14.0, x * 2.5 + J.hue * 9.0), 3);
    var lips = 0.4;
    switch (k) {
        case 1u: { lips = 0.5; }
        case 2u: { lips = 0.25; }
        default: {}
    }
    let light = ((0.014 + 0.08 * rim) * tissue + lip * beads * lips + groove) * (0.55 + 0.45 * top);
    return c * light * (1.0 - 0.6 * u) * (0.6 + 0.5 * J.glow) * P.body;
}
