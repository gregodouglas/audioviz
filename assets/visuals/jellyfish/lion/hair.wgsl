// The lion's mane's hair: about a thousand hairs a jelly, the line. Only
// its 160 strands are simulated (body.wgsl: rooted in eight horseshoe
// clusters of four rows, swaying in locks); they are guides. Each hair runs
// between two neighbouring guides of its cluster's row, wandering from one
// toward the next down its length so that hairs cross, curls its own way
// and splays out toward its tip: a loose, tangled net, dense near the bell
// where they all start, not a curtain.

const HAIR_POINTS: u32 = 65536u;    // the variant's line
const HAIRS_MAX: u32 = 1024u;       // a jelly's at most: four jellies get them all, more share

fn hairs_each() -> u32 {
    return min(HAIRS_MAX, HAIR_POINTS / SEG / jelly_count());
}

fn guide(j: u32, s: u32, k: u32) -> vec3<f32> {
    return item(STRAND0 + (j * STRANDS + s) * SEG + k).xyz;
}

fn point(i: u32) -> LinePoint {
    let P = params();
    var q: LinePoint;
    q.pos = vec2<f32>(0.0);
    q.width = 0.0;
    q.colour = vec3<f32>(0.0);
    q.depth = 0.0;
    let k = i % SEG;
    let per = hairs_each();
    let j = (i / SEG) / per;
    let c = (i / SEG) % per;
    let n = strand_count();
    if (j >= jelly_count() || f32(c) >= f32(per) * P.tentacles || n < 32u || k + 1u >= SEG) {
        return q;
    }
    let J = jelly_at(j);
    if (J.age <= 2.5) {
        return q;
    }
    let cam = the_camera();
    let u = f32(k) / f32(SEG - 1u);
    let id = f32(c) + 0.5;
    let seed = 40.0 + f32(j) * 3.0;
    let cluster = c % 8u;
    let row = (c / 8u) % 4u;
    let along_n = n / 32u;
    // Its place along the horseshoe (spread evenly, each its own within its
    // stretch), wandering a little toward a neighbour down its length.
    let a0 = (f32(c / 32u) + creature_hash(id, seed)) / f32(max(per / 32u, 1u));
    let ph = creature_hash(id, seed + 1.0) * TAU;
    let a = clamp(a0 + 0.3 * creature_hash(id, seed + 2.0) * u * sin(u * 4.0 + ph), 0.0, 1.0);
    let x = clamp(a * f32(along_n) - 0.5, 0.0, f32(along_n - 1u));
    let m0 = min(u32(x), along_n - 1u);
    let m1 = min(m0 + 1u, along_n - 1u);
    let f = x - f32(m0);
    let s0 = cluster + 8u * (row + 4u * m0);
    let s1 = cluster + 8u * (row + 4u * m1);
    var p3 = mix(guide(j, s0, k), guide(j, s1, k), f);
    if (k == 0u) {
        // The root where the bell is this frame (the links follow a frame behind).
        p3 = to_world(J, mix(strand_root(j, s0, n).pos, strand_root(j, s1, n).pos, f));
    } else {
        let R = radius(J);
        let ex = frame_x(J);
        let ez = cross(J.axis, ex);
        // Each its own curl, a slow helix about the way it hangs, tighter on
        // some than others, none at the root.
        let curl = R * (0.04 + 0.16 * creature_hash(id, seed + 3.0)) * smoothstep(0.0, 0.3, u);
        let turn = f32(k) * (0.3 + 0.6 * creature_hash(id, seed + 4.0)) + ph + groove_clock() * 0.15;
        p3 += (ex * cos(turn) + ez * sin(turn)) * curl;
        // Splaying out from the axis toward its tip, a few far.
        let th = (f32(cluster) + 0.5 + (a0 * 2.0 - 1.0) * 0.42) / 8.0;
        let out_dir = dir_to_world(J, vec3<f32>(cos(th * TAU), 0.0, sin(th * TAU)));
        let sp = creature_hash(id, seed + 5.0);
        p3 += out_dir * R * (0.15 + 1.6 * sp * sp * sp) * pow(u, 1.5);
    }
    let pr = project(cam, p3);
    if (pr.z <= 0.0) {
        return q;
    }
    q.pos = pr.xy;
    q.depth = to_camera(cam, p3).z;
    let scale = pr.z / (cam.focal / cam.dist);
    let sp = species_of(LION);
    q.width = sp.strand_px * scale * sqrt(P.size * J.size) * mix(1.0, 0.4, u);
    // Waves of light running down the hair; each hair a little brighter or
    // fainter than the next.
    var light = (0.35 + 0.5 * J.glow) * (1.0 - 0.6 * u) * 0.17;
    light *= (0.2 + 4.0 * pow(0.5 + 0.5 * sin(u * 12.0 - J.pa * 6.0 + f32(cluster) * 0.4), 4.0) * J.glow) * (0.5 + creature_hash(id, seed + 6.0));
    // Hair beyond the body is mostly hidden by it and the mane in front:
    // dimmed by how far behind it hangs, less so toward the tips.
    let behind = (to_camera(cam, p3).z - to_camera(cam, J.pos).z) / radius(J);
    light *= mix(1.0, 0.3, smoothstep(-0.2, 0.7, behind) * (1.0 - 0.5 * u));
    // The stroke's light, a pulse running from the root to the tip.
    let front = (u - J.since * 1.4) / 0.14;
    let before = (u - J.prev * 1.4) / 0.14;
    let pulse = mix(exp(-before * before), exp(-front * front), stroke_handover(J)) * sp.stroke_light;
    let hue = hue_of(J);
    var hair = tone(STRANDS_C, hue + 0.1 * u);
    if (natural_bodies() > 0.5) {
        // Near white, a few warmer.
        hair = mix(vec3<f32>(0.95, 0.93, 0.85), vec3<f32>(1.0, 0.8, 0.6), creature_hash(id, seed + 7.0) * 0.7);
    }
    var col = hair * light + tone(LIGHT, hue) * pulse * 0.1 * (1.0 - 0.5 * u);
    col = water(col, q.depth, cam.dist - 1.0, natural_bodies());
    q.colour = shown(L_TENTACLES, col * P.glow);
    return q;
}
