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
// The plan position (crown in the middle, margin 1 out) interpolates cleanly
// where an angle round would kink across a quad's diagonal. The id is
// j + 0.5, plus 16 for the inner skin, 32 for an arm, 48 for the glow.

const PART_INNER: f32 = 16.0;
const PART_ARM: f32 = 32.0;
const PART_GLOW: f32 = 48.0;
// How far the glow's quad reaches, in sizes of its glow.
const GLOW_REACH: f32 = 4.0;

fn vertex(vi: u32) -> MeshVertex {
    let P = params();
    let j = vi / CORNERS_J;
    let l = vi % CORNERS_J;
    let J = jelly_at(j);
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    if (j >= jelly_count() || J.age < 1.5) {
        return v;
    }
    let cam = the_camera();
    let eye = camera_position(cam);
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
        case 5u: { frills = 7.0; tall = 0.14; }
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
