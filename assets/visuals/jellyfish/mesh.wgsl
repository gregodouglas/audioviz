// The triangles: for each jelly, its bell's two skins (the outer
// exumbrella, the inner subumbrella under the jelly, each RINGS x SECTORS
// quads on the bell's profile), then its oral arms, then its glow in the
// water, then whatever more its variant draws (from EXTRA_AT to CORNERS_J).
// The arms and the rest are the variant's (arms_vertex, extra_vertex: the
// glass species' V-folded sheets; a lion's mane's mass and its mane's
// sheets). What paint gets per corner:
//   bell  colour (distance from the eye, the stroke there, plan x, plan y)
//         data   (0, 0, 0, id): paint works the skin's light out per pixel
//                from the profile (skin_frame) and its own ray
//   arm   colour (distance from the eye, along 0..1, across -1..1, 0); a
//                lion's mane's mass (distance, down it 0..1, round it 0..1, its pleat)
//         data   (light from above, light through it, face on, id)
//   glow  data   (x, y from its centre in sizes, 0, id)
//   mane  colour (distance from the eye, along 0..1, across its gap 0..1,
//                how far behind the body it hangs, in radii)
//         data   (which gap: cluster * 8 + row * 4 + gap, 0, 0, id)
// The plan position (crown in the middle, margin 1 out) interpolates cleanly
// where an angle round would kink across a quad's diagonal. The id is
// j + 0.5, plus 16 for the inner skin, 32 for an arm, 48 for the glow, 64
// for what the variant draws past it (a lion's mane's mane).

const PART_INNER: f32 = 16.0;
const PART_ARM: f32 = 32.0;
const PART_GLOW: f32 = 48.0;
const PART_EXTRA: f32 = 64.0;     // what a variant draws past the glow
// How far the glow's quad reaches, in sizes of its glow.
const GLOW_REACH: f32 = 4.0;

// A jelly's triangles in the order they draw, far to near, for the parts
// that cover what is behind them (a lion's mane's: see its bell_cover): seen
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
    // The cover pass (a variant that covers) draws the bells and the arms:
    // the glow and what comes after it are light.
    if (covering() && l >= HALO_AT) {
        return v;
    }
    if (l >= EXTRA_AT) {
        return extra_vertex(j, J, cam, l - EXTRA_AT, da, db, c6);
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
    // The oral arms (the variant's: arms_vertex).
    let sp = species_of(k);
    if (sp.arm_len <= 0.0 || P.arms < 0.01) {
        return v;
    }
    return arms_vertex(j, J, cam, eye, l - BELL_CORNERS, da, db, c6);
}
