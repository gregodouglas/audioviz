// The lion's mane's mass as it looks in the water (photos 02, 19, 42): a
// dense, milky, billowing body, shaped like a thick fluid rolling into the
// water or a plume of smoke, but solid. Round a small core (mass.wgsl)
// PUFFS billows hang along the arms' middle (mass_spine), each a quad
// facing the eye with a lumpy outline from noise, shaded as a rounded lump
// (lit on top, warm in its hollows and where light comes through its
// edge). They cover: facing the eye, all their quads are parallel, so they
// hide each other and the hair behind them without seams. Each lives a
// slow cycle: it swells out of the mass from nothing, rolls out and down
// as it grows, and shrinks back in, so the mass rolls and churns without
// ever popping.

const PUFFS: u32 = 512u;
const PLUME_CORNERS: u32 = 3072u;    // PUFFS * 6
const PART_PUFF: f32 = 80.0;         // PART_EXTRA + 16: a billow, not the mane's sheets
const PUFF_REACH: f32 = 1.4;         // how far a billow's quad reaches past its soft middle

// Billow b of jelly j: (centre, size, world units), (where in its cycle 0..1, its seed).
struct Puff {
    at: vec3<f32>,
    size: f32,
    t: f32,
    seed: f32,
    u: f32,
};

fn puff_of(j: u32, J: Jelly, b: u32) -> Puff {
    let seed = J.hue * 5.0 + 80.0;
    let id = f32(b) + 0.5;
    let h = vec4<f32>(creature_hash(id, seed), creature_hash(id, seed + 1.0), creature_hash(id, seed + 2.0), creature_hash(id, seed + 3.0));
    let h2 = vec2<f32>(creature_hash(id, seed + 4.0), creature_hash(id, seed + 5.0));
    var p: Puff;
    p.u = 0.0;
    p.seed = h.x * 97.0;
    // A slow cycle each, 20 to 40 seconds (quickening with the music).
    p.t = fract(groove_clock() * (0.025 + 0.025 * h.y) + h.x);
    // Down the mass, sinking a little as it rolls; round it, drifting.
    let u = clamp(0.06 + h.z * 0.84 + 0.14 * p.t, 0.0, 1.0);
    let us = u * MASS_REACH;
    let at = mass_spine(j, us);
    let dir = normalize(mass_spine(j, us + 0.03) - mass_spine(j, max(us - 0.03, 0.0)) + vec3<f32>(1e-5, 0.0, 0.0));
    let fx = frame_x(J);
    let ex = normalize(fx - dir * dot(fx, dir) + vec3<f32>(0.0, 1e-5, 0.0));
    let ez = cross(dir, ex);
    let a = h.w * TAU + 0.5 * (h2.x - 0.5) * p.t;
    let R = radius(J) * params().arms;
    // Gathered under the bell, swelling to most of its width, narrowing
    // toward the end; rolled outward over its cycle.
    let width = mix(0.55, 1.0, smoothstep(0.0, 0.2, u)) * (1.0 - 0.55 * smoothstep(0.35, 1.0, u));
    let rad = (0.15 + 0.4 * sqrt(h2.y) + 0.15 * p.t) * width;
    p.u = u;
    p.at = at + (ex * cos(a) + ez * sin(a)) * rad * R;
    // Growing from nothing and shrinking back into the mass; smaller
    // toward the end, where the mass thins into the hair.
    let grow = smoothstep(0.0, 0.3, p.t) * (1.0 - smoothstep(0.7, 1.0, p.t));
    p.size = R * (0.13 + 0.13 * h2.x) * (0.75 + 0.6 * p.t) * grow * mix(0.7, 1.0, smoothstep(0.0, 0.2, u)) * (1.0 - 0.5 * smoothstep(0.4, 1.0, u));
    return p;
}

fn puff_vertex(j: u32, J: Jelly, cam: Camera, l: u32) -> MeshVertex {
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    if (params().arms < 0.01) {
        return v;
    }
    let p = puff_of(j, J, l / 6u);
    if (p.size <= 0.0) {
        return v;
    }
    var qx = array<f32, 6>(-1.0, 1.0, 1.0, -1.0, 1.0, -1.0);
    var qy = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    let at = vec2<f32>(qx[l % 6u], qy[l % 6u]) * PUFF_REACH;
    var c = to_camera(cam, p.at);
    c = vec3<f32>(c.xy + at * p.size, c.z);
    v.pos = vec4<f32>(c.xy * cam.focal, 0.0, max(c.z, 0.01));
    v.data = vec4<f32>(at, p.t, f32(j) + 0.5 + PART_PUFF);
    v.colour = vec4<f32>(c.z, p.seed, p.u, 0.0);
    return v;
}

// A billow at (x, y) from its middle (in its sizes; y up on screen), t in
// its cycle, u how far down the mass: a rounded lump whose outline rolls
// with noise, its surface bumped by the same noise; lit from above, milky
// cream where lit, warm orange in its hollows and underneath, and glowing
// warm at its edge, where the light comes through.
fn puff_look(J: Jelly, xy: vec2<f32>, t: f32, seed: f32, u: f32) -> vec4<f32> {
    let P = params();
    let hue = hue_of(J);
    let r = length(xy);
    let q = xy * 1.1 + vec2<f32>(seed, seed * 0.7) + vec2<f32>(0.0, t * 0.8);
    let n = fbm(q, 4);
    let body = (1.0 - r) + 0.5 * (n - 0.5);
    // A crisp, lumpy outline (a pixel soft), gone before the quad's edge.
    let edge = clamp(body / max(fwidth(body), 1e-4) + 0.5, 0.0, 1.0) * step(r, PUFF_REACH * 0.98);
    // Its surface: a dome, bumped by the noise.
    let e = 0.05;
    let g = vec2<f32>(fbm(q + vec2<f32>(e, 0.0), 4) - n, fbm(q + vec2<f32>(0.0, e), 4) - n) / e;
    let rim = clamp(1.0 - body * 2.2, 0.0, 1.0);
    let N = normalize(vec3<f32>(xy * 0.9 - g * 0.12, sqrt(max(1.0 - min(r * r, 1.0), 0.04))));
    let L = normalize(vec3<f32>(0.25, 0.85, 0.45));
    let lit = 0.45 + 0.55 * dot(N, L);
    let hollow = smoothstep(0.15, -0.25, n - 0.5 + 0.3 * (1.0 - r));
    let cream = mix(tone(ARMS_C, hue), vec3<f32>(1.0, 0.9, 0.76), 0.62);
    let warm = mix(tone(ARMS_C, hue), tone(ORGANS, hue), 0.35);
    // Rounded: darker toward its outline, deepest where it tucks under the
    // billows round it (the creases between them).
    // (Milky: the creases are not dark but warm, the light inside showing
    // through where it is thin.)
    let round = smoothstep(0.1, 1.0, rim);
    let shade = clamp(lit - 0.35 * hollow - 0.45 * round, 0.0, 1.0);
    var col = mix(warm * 0.75, cream, shade) * (0.26 + 0.4 * shade);
    // The light from inside, warmest near the bell.
    col += warm * (0.04 + 0.06 * round) * (0.6 + 0.6 * (1.0 - u));
    return vec4<f32>(col * P.body, 0.95 * edge);
}

fn puff_paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    let J = jelly_at(u32(f.data.w) % 16u);
    let m = puff_look(J, f.data.xy, f.data.z, f.colour.y, f.colour.z);
    let c = water(shown(L_ARMS, m.rgb), f.colour.x, eye_near(), natural_bodies());
    let cover = m.a * water_keep(f.colour.x, eye_near()) * select(0.0, 1.0, layer_on(L_ARMS));
    // (Solid, from a half, it was drawn in the cover pass; its edge is light.)
    if (cover_pass() && !covering() && cover >= 0.5) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(c * cover * P.glow, cover);
}
