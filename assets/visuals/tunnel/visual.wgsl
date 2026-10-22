// Audio tunnel: rings of light rushing toward the viewer. Every Spacing of
// travel a ring is born at the far end with the 32 bands of that moment
// (mirrored round it, the lows at the top); rings ride toward the eye
// (faster with the energy and on each beat) and are reborn when they pass
// it. The tunnel's centre line winds with distance, so it curves away.
//
// Items, every one working out the same clock from last frame's:
//   0             (distance travelled, distance at the last birth, births, 0)
//   1             (last beat pulse, beat kick, 1 once started, 0)
//   BANDS0 + 8s + g   ring slot s's bands 4g .. 4g+3, as born
//   META0 + s         ring slot s: (distance at its birth, 0, 0, 0)
// The line: ring r is points r * PER .. r * PER + 127, then a break.

const MAX_RINGS: u32 = 64u;
const BANDS0: u32 = 2u;
const META0: u32 = 514u;
const PER: u32 = 129u;
const Z_NEAR: f32 = 0.25;
const FOCAL: f32 = 0.9;

fn ring_count() -> u32 {
    return clamp(u32(params().rings), 8u, MAX_RINGS);
}

fn far() -> f32 {
    return Z_NEAR + f32(ring_count()) * params().spacing;
}

// This frame's clock: (distance, distance at the last birth, births, the
// slot reborn this frame or MAX_RINGS for none), and the beat kick.
struct Clock {
    d: f32,
    last: f32,
    births: f32,
    reborn: u32,
    kick: f32,
};

fn clock() -> Clock {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let c0 = item(0u);
    let c1 = item(1u);
    var k: Clock;
    k.kick = c1.y * exp(-dt / 0.35);
    if (V.beat > c1.x + 0.4) {
        k.kick = 1.0;
    }
    let speed = P.speed * 0.6 * (0.5 + 0.5 * V.energy) * (1.0 + 1.5 * P.kick * k.kick);
    k.d = c0.x + speed * dt;
    k.last = c0.y;
    k.births = c0.z;
    k.reborn = MAX_RINGS;
    if (k.d - k.last >= P.spacing) {
        k.reborn = u32(k.births) % ring_count();
        k.births = k.births + 1.0;
        k.last = k.d;
    }
    return k;
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let k = clock();
    let started = item(1u).z > 0.5;
    if (i == 0u) {
        return vec4<f32>(k.d, k.last, k.births % 65536.0, 0.0);
    }
    if (i == 1u) {
        return vec4<f32>(V.beat, k.kick, 1.0, 0.0);
    }
    if (i >= META0) {
        let s = i - META0;
        if (!started) {
            // At the start the tunnel is already full of quiet rings.
            return vec4<f32>(-f32(ring_count() - 1u - min(s, ring_count() - 1u)) * P.spacing, 0.0, 0.0, 0.0);
        }
        return select(item(i), vec4<f32>(k.d, 0.0, 0.0, 0.0), s == k.reborn);
    }
    let s = (i - BANDS0) / 8u;
    let g = (i - BANDS0) % 8u;
    if (!started) {
        return vec4<f32>(0.0);
    }
    if (s == k.reborn) {
        return vec4<f32>(band(4u * g), band(4u * g + 1u), band(4u * g + 2u), band(4u * g + 3u));
    }
    return item(i);
}

// Ring slot s's band at t 0..1 (low to high), interpolated.
fn ring_band(s: u32, t: f32) -> f32 {
    let x = clamp(t, 0.0, 1.0) * 31.0;
    let a = u32(floor(x));
    let b = min(a + 1u, 31u);
    let va = item(BANDS0 + s * 8u + a / 4u)[a % 4u];
    let vb = item(BANDS0 + s * 8u + b / 4u)[b % 4u];
    return mix(va, vb, fract(x));
}

// The tunnel's centre line at distance s along it.
fn centre_at(s: f32) -> vec2<f32> {
    return params().wind * vec2<f32>(1.4 * sin(s * 0.8), 0.9 * sin(s * 0.53 + 1.0));
}

fn point(i: u32) -> LinePoint {
    let P = params();
    var q: LinePoint;
    q.pos = vec2<f32>(0.0);
    q.width = 0.0;
    q.colour = vec3<f32>(0.0);
    let r = i / PER;
    let j = i % PER;
    if (r >= ring_count() || j == PER - 1u) {
        return q;
    }
    let d = item(0u).x;
    let born = item(META0 + r).x;
    let z = far() - (d - born);
    if (z <= Z_NEAR) {
        return q;
    }
    // Round the ring from the top, the spectrum mirrored down both sides.
    // The last point lands on the first, closing the ring.
    let a = f32(j) / f32(PER - 2u) * TAU;
    let t = select(a / 3.14159265, (TAU - a) / 3.14159265, a > 3.14159265);
    let v = ring_band(r, t);
    let s_ring = born + far() - Z_NEAR;
    let turn = a + P.twist * s_ring * 0.5;
    let radius = 1.0 + P.shape * 0.5 * v;
    let off = centre_at(s_ring) - centre_at(d + 0.6);
    q.pos = (off + vec2<f32>(sin(turn), cos(turn)) * radius) * FOCAL / z;
    q.width = P.width * clamp(1.2 / z, 0.4, 3.0);
    var c = colour(fract(s_ring * 0.12));
    if (P.colouring > 1.5) {
        c = colour(0.15 + 0.8 * v);
    } else if (P.colouring > 0.5) {
        c = colour(t);
    }
    let fog = exp(-P.fog * (z - Z_NEAR) * 0.5);
    let birth = smoothstep(far(), far() - 2.0 * P.spacing, z);
    let near = smoothstep(Z_NEAR, Z_NEAR + 0.25, z);
    q.colour = c * (0.35 + 1.2 * v) * fog * birth * near;
    return q;
}
