// The lion's mane's mane: sheets of hair past its glow in the water, then
// the lumps of its mass (plume.wgsl).

const MANE_GAPS: u32 = 4u;          // sheets along a row of a cluster (between its five hairs)
const MANE_CORNERS: u32 = 5760u;    // 8 clusters * 2 rows * MANE_GAPS * (SEG - 1) * 2 * 3
const PLUME_AT: u32 = 32838u;       // EXTRA_AT + MANE_CORNERS
const CORNERS_J: u32 = 126918u;     // PLUME_AT + PLUME_CORNERS

// A lion's mane's sheet of hair, corner l of the sheets (a quad a link,
// across the gap between two neighbouring hairs of a cluster's outer or
// inner row): its corners are the hairs' own links, so it moves with them.
// The line hairs alone read as wires; the sheets give the mane its mass.
fn mane_vertex(j: u32, J: Jelly, cam: Camera, l: u32, da: array<u32, 6>, db: array<u32, 6>, c6: u32) -> MeshVertex {
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    let n = strand_count();
    if (n < 16u || covering()) {
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
    v.data = vec4<f32>(f32(cluster * 8u + row + gap), 0.0, 0.0, f32(j) + 0.5 + PART_EXTRA);
    return v;
}

fn extra_vertex(j: u32, J: Jelly, cam: Camera, l: u32, da: array<u32, 6>, db: array<u32, 6>, c6: u32) -> MeshVertex {
    if (l >= PLUME_AT - EXTRA_AT) {
        return lump_vertex(j, J, cam, l - (PLUME_AT - EXTRA_AT), da, db, c6);
    }
    return mane_vertex(j, J, cam, l, da, db, c6);
}

fn extra_paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    let id = f.data.w;
    if (id > PART_LUMP) {
        return lump_paint(f);
    }
    let J = jelly_at(u32(id) % 16u);
    let m = mane_look(J, f.colour.y, f.colour.z, f.colour.w, f.data.x, fwidth(f.colour.z));
    let keep = water_keep(f.colour.x, eye_near());
    return vec4<f32>(shown(L_TENTACLES, water(m.rgb, f.colour.x, eye_near(), natural_bodies()) * P.glow), m.a * keep * select(0.0, 1.0, layer_on(L_TENTACLES)));
}

// A lion's mane's sheet of hair at u (root 0 to tip 1), x across its gap
// (0..1), `behind` how far beyond the body it hangs (radii): a few fine
// hairs between the two it spans, each wavering its own way, over a soft
// fill; it covers a little of what is behind it, fading toward the tips,
// and on the far side the body and the mane in front hide most of it.
fn mane_look(J: Jelly, u: f32, x: f32, behind: f32, gap: f32, fx: f32) -> vec4<f32> {
    let hue = hue_of(J);
    var hair = tone(STRANDS_C, hue + 0.1 * u);
    if (natural_bodies() > 0.5) {
        hair = mix(hair, vec3<f32>(1.0, 0.8, 0.55), 0.25);
    }
    let w = x + 0.08 * sin(u * 9.0 + gap * 1.7) + 0.05 * sin(u * 23.0 + gap * 3.1);
    let hairs = 5.0;
    let line = pow(0.5 + 0.5 * cos(w * TAU * hairs), 6.0);
    let fine = keep_detail(fx * hairs * 2.0);
    let fibres = mix(0.35, 0.3 + 0.9 * line, fine);
    // (Thin near the bell, where the mass glows through the hair.)
    let fade = (1.0 - u) * smoothstep(0.05, 0.35, u);
    let hidden = mix(1.0, 0.3, smoothstep(-0.2, 0.7, behind) * (1.0 - 0.5 * u));
    let light = 0.05 * fibres * fade * hidden * (0.5 + 0.5 * soft(J.glow));
    return vec4<f32>(hair * light, 0.22 * fibres * fade * hidden);
}
