// What hangs from a bell: tentacles and hair from the margin, oral arms
// from the mouth, each a chain of links (chain_link: no momentum, so they
// trail without winding into loops), rooted on the bell's moving profile so
// they go where the bell goes. The first link is the root itself, so a
// strand starts on the skin. The tentacles are the line.

fn strand_count() -> u32 {
    let P = params();
    if (P.tentacles < 0.01) {
        return 0u;
    }
    return clamp(u32(round(species_of(kind()).strands * P.tentacles)), 1u, STRANDS);
}

// Where strand s (of n) leaves jelly j's bell, which way the skin runs
// there (outward along it) and the way round the margin, in the bell's own
// units.
struct Root {
    pos: vec3<f32>,
    out: vec3<f32>,
    round: vec3<f32>,
};

fn strand_root(j: u32, s: u32, n: u32) -> Root {
    let k = kind();
    let b = bell_of(k);
    var r: Root;
    if (k == COMB) {
        // Two tentacles from the sides, halfway down.
        let th = select(0.0, 0.5, s == 1u);
        r.pos = jelly_skin(j, RINGS / 2u, th, false).pos * 0.9;
        r.out = vec3<f32>(cos(th * TAU), -0.3, sin(th * TAU));
        r.round = vec3<f32>(-sin(th * TAU), 0.0, cos(th * TAU));
        return r;
    }
    var th = (f32(s) + 0.5) / f32(max(n, 1u));
    var node = RINGS;
    if (k == LION) {
        // Hair in eight clusters under the lobes, set in from the margin:
        // each a horseshoe of four rows, open to the margin (its middle
        // deepest in), the hairs a little uneven along it.
        let h = lion_hair(s, n);
        let a = h.y * 2.0 - 1.0;
        th = (h.x + 0.5 + a * 0.42 + (creature_hash(f32(s), 31.0) - 0.5) * 0.03) / 8.0;
        node = RINGS - 2u - u32(h.z) - u32(round((1.0 - a * a) * 3.0));
    }
    // From the underside of the margin.
    r.pos = jelly_skin(j, node, th, true).pos;
    r.out = bell_tangent(b, profile(j, node), th);
    r.round = vec3<f32>(-sin(th * TAU), 0.0, cos(th * TAU));
    return r;
}

// Lion's mane hair s of n: (its cluster, how far along the cluster's
// horseshoe 0..1, its row 0..3, its lock: a few neighbours that sway
// together, so the mane moves in locks, not as a comb).
fn lion_hair(s: u32, n: u32) -> vec4<f32> {
    let per = max(n / 8u, 1u);
    let m = s / 8u;
    let along = (f32(m / 4u) + 0.5) / f32(max(per / 4u, 1u));
    return vec4<f32>(f32(s % 8u), along, f32(m % 4u), f32(s % 8u) * 16.0 + floor(along * 6.0));
}

fn strand_length(s: u32) -> f32 {
    let sp = species_of(kind());
    // The atolla trails one long tentacle.
    let long = kind() == ATOLLA && s == 0u;
    // Each strand its own length, so the ends are ragged, not a cut edge.
    var shortest = 0.55;
    switch (kind()) {
        case 0u: { shortest = 0.35; }
        case 5u: { shortest = 0.4; }
        default: {}
    }
    let ragged = select(mix(shortest, 1.0, fract(f32(s) * 0.618 + 0.3)), 1.0, kind() == COMB);
    return sp.strand_len * select(1.0, 7.0, long) * ragged * params().reach;
}

// A tentacle hangs from the margin: near its root it carries on the way the
// margin points (curled in on a stroke, splayed out as the bell flares),
// further down the water and its weight take it, and it sways side to side
// along the margin, out of step with its neighbours (in step they read as a
// comb; a sway that turns round a strand coils it into a spring). It is
// stiff near the root and limp at the tip, so a bend runs down it.
fn strand_step(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let idx = i - STRAND0;
    let k = idx % SEG;
    let s = (idx / SEG) % STRANDS;
    let j = idx / (SEG * STRANDS);
    let J = jelly_at(j);
    let n = strand_count();
    let kd = kind();
    let sp = species_of(kd);
    let root = strand_root(j, s, n);
    let anchor = to_world(J, root.pos);
    if (j >= jelly_count() || s >= n) {
        return vec4<f32>(anchor, 0.0);
    }
    if (k == 0u) {
        return vec4<f32>(anchor, 0.0);
    }
    let len = strand_length(s) * radius(J) / f32(SEG - 1u);
    let fine = kd == LION || kd == MOON;
    let amp = 0.3 * P.sway * (0.5 + 0.5 * item(1u).y * music()) * select(1.0, 0.6, fine);
    let u = f32(k) / f32(SEG - 1u);
    let out_dir = normalize(dir_to_world(J, root.out));
    let hang = normalize(-J.axis * 0.75 + vec3<f32>(0.0, -0.45, 0.0) - J.vel * 2.0 + current(anchor, V.time) * 0.15 * P.current);
    // Side to side along the margin. (Worked out from the hang and the way
    // out, it flipped in a frame when a tipping bell brought the two into
    // line.)
    let side = dir_to_world(J, root.round);
    var w = groove_clock() * 1.1 - f32(k) * 0.18 + creature_hash(f32(s), 24.0 + f32(j)) * TAU;
    if (kd == LION) {
        // In locks: a lock's hairs sway nearly together.
        let lock = lion_hair(s, n).w;
        w = groove_clock() * 0.8 - f32(k) * 0.16 + (creature_hash(lock, 24.0 + f32(j)) + 0.08 * creature_hash(f32(s), 24.0 + f32(j))) * TAU;
    }
    var rest = normalize(hang + side * sin(w) * amp);
    let follow = sp.follow * mix(1.0, 0.13, u);
    let h = vec3<f32>(creature_hash(f32(s), 21.0 + f32(j)), creature_hash(f32(s), 22.0 + f32(j)), creature_hash(f32(s), 23.0 + f32(j))) - 0.5;
    rest = normalize(out_dir * follow + rest * (1.0 - 0.4 * follow) + h * select(0.2, 0.45, kd == MOON));
    if (kd == LION) {
        // Each hair a slow, gentle wave, more toward the tip, so the mane
        // hangs soft and a little tangled.
        let ph = creature_hash(f32(s), 25.0 + f32(j)) * TAU;
        let curl = 0.15 + 0.45 * creature_hash(f32(s), 27.0 + f32(j));
        let wave = sin(f32(k) * 0.45 + ph + groove_clock() * 0.3) * side + cos(f32(k) * 0.3 + ph * 1.3 - groove_clock() * 0.25) * out_dir * 0.5;
        rest = normalize(rest + wave * curl * (0.2 + 0.9 * u));
    }
    if (kd == MOON) {
        // The fringe curls a little: each fine tentacle its own long, slow
        // wave, more toward the tip, some curling more than others, so the
        // fringe hangs as a soft curtain, neither a comb nor a tangle.
        let ph = creature_hash(f32(s), 25.0 + f32(j)) * TAU;
        let curl = 0.4 + 1.2 * creature_hash(f32(s), 27.0 + f32(j));
        let wave = sin(f32(k) * 0.5 + ph + groove_clock() * 0.4) * side + cos(f32(k) * 0.35 + ph * 1.3 - groove_clock() * 0.3) * out_dir * 0.6;
        rest = normalize(rest + wave * curl * (0.3 + 1.2 * u));
    }
    if (J.age < 1.5) {
        return vec4<f32>(anchor + rest * len * f32(k), 0.0);
    }
    let parent = item(i - 1u).xyz;
    var rate = mix(4.0, 1.8, u);
    if (kd == MOON) {
        rate = mix(4.5, 2.2, u);
    }
    if (kd == LION) {
        rate = mix(2.4, 1.2, u);
    }
    return vec4<f32>(chain_link(item(i).xyz, parent, rest, 1.0 - exp(-dt * rate), len), 0.0);
}

// An oral arm hangs from the mouth under the middle of the bell and billows
// (a slow wave runs down each, every arm its own); a sea nettle's spiral
// slowly round each other.
fn arm_step(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let idx = i - ARM0;
    let k = idx % ARM_SEG;
    let a = (idx / ARM_SEG) % ARMS;
    let j = idx / (ARM_SEG * ARMS);
    let J = jelly_at(j);
    let kd = kind();
    let moon = kd == MOON;
    // A moon jelly's arms are perradial: under the rhopalia, between the gonads.
    let th = (f32(a) + select(0.5, 0.0, moon)) / f32(ARMS);
    let mouth = profile(j, 0u).y - bell_thickness(bell_of(kd), 0.0) - 0.02;
    let rr = select(0.1, 0.05, moon);
    let anchor = to_world(J, vec3<f32>(rr * cos(th * TAU), mouth, rr * sin(th * TAU)));
    let len = species_of(kd).arm_len * P.arms * radius(J) / f32(ARM_SEG - 1u);
    if (j >= jelly_count() || len <= 0.0 || k == 0u) {
        return vec4<f32>(anchor, 0.0);
    }
    let lion = kd == LION;
    let twist = select(0.0, f32(k) * 0.45 + groove_clock() * 0.3, kd == NETTLE) + f32(a) * 1.5708;
    let ex = frame_x(J);
    let ez = cross(J.axis, ex);
    let amp = 0.2 * P.sway * (0.5 + 0.5 * item(1u).y * music());
    let ph = f32(a) * 1.7 + f32(j) * 2.9;
    // (A lion's mane's heavy curtains billow less, and slower.)
    let wobble = (ex * sin(groove_clock() * 0.7 - f32(k) * 0.42 + ph) + ez * cos(groove_clock() * 0.55 - f32(k) * 0.33 + ph * 1.3)) * (0.25 + 0.5 * amp) * select(1.0, 0.3, lion);
    var splay = select(0.3, 1.2, moon);
    if (lion) {
        splay = 0.35;
    }
    let rest = normalize(-J.axis + vec3<f32>(0.0, -0.3, 0.0) - J.vel * 2.0 + (ex * cos(twist) + ez * sin(twist)) * splay + current(anchor, V.time) * 0.1 * P.current + wobble);
    if (J.age < 1.5) {
        return vec4<f32>(anchor + rest * len * f32(k), 0.0);
    }
    let parent = item(i - 1u).xyz;
    let u = f32(k) / f32(ARM_SEG - 1u);
    return vec4<f32>(chain_link(item(i).xyz, parent, rest, 1.0 - exp(-dt * mix(5.0, 1.5, u)), len), 0.0);
}

// ---------------------------------------------------------------- the line

// Tentacles and hair, drawn as the line: fine, fading toward the tip, in
// the water's depth; the light of a stroke runs down them from the bell.
fn point(i: u32) -> LinePoint {
    let P = params();
    let k = i % SEG;
    let s = (i / SEG) % STRANDS;
    let j = i / (SEG * STRANDS);
    let J = jelly_at(j);
    let kd = kind();
    let sp = species_of(kd);
    let cam = the_camera();
    // The root where the bell is this frame (the links follow a frame behind).
    let p3 = select(item(STRAND0 + i).xyz, to_world(J, strand_root(j, s, strand_count()).pos), k == 0u);
    let pr = project(cam, p3);
    var q: LinePoint;
    q.pos = pr.xy;
    q.depth = to_camera(cam, p3).z;
    let on = j < jelly_count() && s < strand_count() && J.age > 2.5 && k + 1u < SEG && pr.z > 0.0;
    let u = f32(k) / f32(SEG - 1u);
    let scale = pr.z / (cam.focal / cam.dist);
    q.width = select(0.0, sp.strand_px * scale * sqrt(P.size * J.size) * mix(1.0, 0.35, u), on);
    var light = (0.35 + 0.5 * J.glow) * (1.0 - 0.7 * u) * select(1.0, 0.35, kd == LION);
    switch (kd) {
        case 5u: {
            // Waves of light running down the hair; each hair a little
            // brighter or fainter than the next.
            light *= (0.2 + 4.0 * pow(0.5 + 0.5 * sin(u * 12.0 - J.pa * 6.0 + f32(s / 8u) * 0.4), 4.0) * J.glow) * (0.5 + creature_hash(f32(s), 26.0 + f32(j)));
        }
        case 3u: {
            light *= select(0.7, 1.0, s == 0u);
        }
        case 2u: {
            light *= 0.1 + flash_of(J) * 0.5;
        }
        case 4u: {
            light *= 0.4;
        }
        case 0u: {
            // A fringe of a hundred and sixty: each one faint, some fainter.
            light *= 0.45 * (0.55 + 0.9 * creature_hash(f32(s), 26.0 + f32(j)));
        }
        default: {}
    }
    // The stroke's light, a pulse running from the root to the tip (the
    // last stroke's still running out as the new one takes over).
    let front = (u - J.since * 1.4) / 0.14;
    let before = (u - J.prev * 1.4) / 0.14;
    let pulse = mix(exp(-before * before), exp(-front * front), stroke_handover(J)) * sp.stroke_light * select(1.0, 0.0, sp.period <= 0.0);
    let hue = hue_of(J);
    var hair = tone(STRANDS_C, hue + 0.1 * u);
    if (kd == LION && natural_bodies() > 0.5) {
        // From orange to pale, hair by hair.
        hair = mix(hair, vec3<f32>(1.0, 0.8, 0.55), 0.45 * creature_hash(f32(s), 28.0 + f32(j)));
    }
    var c = hair * light + tone(LIGHT, hue) * pulse * 0.6 * (1.0 - 0.5 * u);
    c = water(c, to_camera(cam, p3).z, cam.dist - 1.0, natural_bodies());
    q.colour = shown(L_TENTACLES, c * P.glow);
    return q;
}
