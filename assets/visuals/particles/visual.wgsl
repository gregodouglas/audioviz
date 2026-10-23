// Particle dance: up to 65536 glowing particles in 3D, seen by a view going
// slowly round the middle.
//   Cloud: an ABC flow (a smooth, chaotic, divergence-free swirl) carries
//     them; each belongs to one of three points drifting on slow paths,
//     which pulls it in, so they gather into shifting clouds; a beat blows
//     each cloud apart from its point. Each lives 4 to 10 s, then is reborn
//     beside its point.
//   Galaxy: a thin two-armed spiral disc and a bulge, worked out from the
//     clock (no state), turning with the energy; a beat flashes the core.
//   Burst: a dead particle waits; on a beat a share of the waiting ones is
//     thrown out from the middle, slows, swirls a little and fades.
// Each particle is a quad facing the eye (its size falling with distance),
// painted as a bright core in a soft halo, added as light.
//
// Items:
//   0           (flow clock, last beat pulse, beats, 1 once started)
//   1           (bass, highs, view turn, core flash), eased
//   G + 2p      particle p: (position, age)
//   G + 2p + 1  particle p: (velocity, life)
// Every particle item works out the same step from last frame's items.

const G: u32 = 2u;
const MAX: u32 = 65536u;
const EYE_DIST: f32 = 4.4;
const FOCAL: f32 = 1.8;

fn count() -> u32 {
    return clamp(u32(params().count), 1000u, MAX);
}

struct Beat {
    struck: bool,
    beats: f32,
};

fn beat_now() -> Beat {
    let c0 = item(0u);
    var b: Beat;
    b.struck = V.beat > c0.y + 0.4;
    b.beats = c0.z + select(0.0, 1.0, b.struck);
    return b;
}

// The three points the cloud gathers round, drifting on slow paths.
fn attractor(k: u32, t: f32) -> vec3<f32> {
    let f = f32(k);
    return vec3<f32>(
        1.0 * sin(t * 0.21 + f * 2.09),
        0.55 * sin(t * 0.27 + f * 1.31 + 0.6),
        1.0 * cos(t * 0.19 + f * 2.09));
}

// An ABC flow, its three phases drifting with the clock.
fn swirl_at(p: vec3<f32>, t: f32) -> vec3<f32> {
    let q = p * 1.4;
    return vec3<f32>(
        sin(q.z + t) + 0.6 * cos(q.y - 0.7 * t),
        0.8 * sin(q.x + 0.9 * t) + cos(q.z),
        0.6 * sin(q.y - 1.1 * t) + 0.8 * cos(q.x));
}

// 0..1 for particle p and a salt. An integer hash: hash21 on particle
// numbers in the thousands keeps only a few bits, so particles pile up.
fn rand(p: u32, salt: f32) -> f32 {
    var n = p * 747796405u + bitcast<u32>(salt) * 2891336453u + 1u;
    n = ((n >> ((n >> 28u) + 4u)) ^ n) * 277803737u;
    n = (n >> 22u) ^ n;
    return f32(n & 0xffffffu) / 16777215.0;
}

// A random point in the unit ball.
fn in_ball(p: u32, salt: f32) -> vec3<f32> {
    let z = rand(p, salt) * 2.0 - 1.0;
    let a = rand(p, salt + 1.7) * TAU;
    let r = pow(rand(p, salt + 3.1), 0.3333);
    let s = sqrt(max(1.0 - z * z, 0.0));
    return vec3<f32>(s * cos(a), z, s * sin(a)) * r;
}

fn galaxy_point(p: u32, t: f32) -> vec3<f32> {
    let h1 = rand(p, 11.0);
    let wobble = 0.03 * vec3<f32>(sin(t * 3.0 + h1 * 50.0), cos(t * 2.3 + h1 * 70.0), sin(t * 2.7 + h1 * 30.0));
    if (h1 < 0.15) {
        return in_ball(p, 13.0) * vec3<f32>(0.35, 0.2, 0.35) + wobble;
    }
    let r = 0.25 + 2.0 * pow(rand(p, 17.0), 0.7);
    let arm = floor(rand(p, 19.0) * 2.0) * 3.14159;
    // Most stars near an arm, a fifth anywhere in the disc.
    let loose = select(1.0, 7.0, rand(p, 43.0) < 0.2);
    let spread = (rand(p, 23.0) - 0.5) * 0.9 * (0.4 + 0.6 * rand(p, 29.0)) * loose;
    let a = arm + 1.7 * log(r / 0.25) + spread + 0.5 * t;
    let y = (rand(p, 31.0) + rand(p, 37.0) - 1.0) * 0.09 * (1.2 - r / 2.25);
    return vec3<f32>(r * cos(a), y, r * sin(a)) + wobble;
}

struct Particle {
    pos: vec3<f32>,
    age: f32,
    vel: vec3<f32>,
    life: f32,
};

fn step_particle(p: u32) -> Particle {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1) * P.speed;
    let c0 = item(0u);
    let c1 = item(1u);
    let t = c0.x;
    let b = beat_now();
    let a = item(G + 2u * p);
    let v = item(G + 2u * p + 1u);
    var s: Particle;
    s.pos = a.xyz;
    s.age = a.w + dt;
    s.vel = v.xyz;
    s.life = v.w;
    let started = c0.w > 0.5;
    if (P.mode > 1.5) {
        // Burst.
        if (!started) {
            s.pos = vec3<f32>(0.0);
            s.vel = vec3<f32>(0.0);
            s.age = 1.0;
            s.life = 0.0;
            return s;
        }
        if (s.age < s.life) {
            s.vel = s.vel * exp(-dt * 1.2) + swirl_at(s.pos, t) * P.swirl * 0.3 * dt;
            s.pos = s.pos + s.vel * dt;
            return s;
        }
        let share = clamp(0.25 * P.kick * (0.6 + 0.6 * c1.x), 0.0, 1.0);
        if (b.struck && rand(p, b.beats) < share) {
            let d = normalize(in_ball(p, b.beats + 5.0) + vec3<f32>(1e-4));
            s.pos = d * 0.05;
            s.vel = d * (0.8 + 0.8 * rand(p, b.beats + 9.0)) * (1.0 + 0.6 * c1.x) * 1.6;
            s.age = 0.0;
            s.life = 1.2 + 1.8 * rand(p, b.beats + 13.0);
        }
        return s;
    }
    if (P.mode > 0.5) {
        // Galaxy.
        let at = galaxy_point(p, t);
        s.vel = (at - s.pos) / max(dt, 1e-3);
        s.pos = at;
        s.age = 1.0;
        s.life = 2.0;
        return s;
    }
    // Cloud.
    let k = p % 3u;
    let home = attractor(k, t * 0.6);
    if (!started || s.life <= 0.0 || s.age > s.life || length(s.pos) > 2.6) {
        s.pos = home + in_ball(p, t + 3.0) * 0.4;
        s.vel = vec3<f32>(0.0);
        s.age = select(rand(p, 41.0) * 4.0, 0.0, started);
        s.life = 4.0 + 6.0 * rand(p, t + 7.0);
        return s;
    }
    let pull = (home - s.pos) * P.gather * 2.0;
    s.vel = s.vel * exp(-dt * 2.0) + (swirl_at(s.pos, t) * P.swirl * 1.2 + pull) * dt;
    if (b.struck) {
        let away = normalize(s.pos - home + 0.01 * in_ball(p, b.beats));
        s.vel = s.vel + away * P.kick * motion() * (0.25 + 0.45 * c1.x) * (0.5 + rand(p, b.beats + 2.0));
    }
    s.pos = s.pos + s.vel * dt;
    return s;
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let c0 = item(0u);
    let c1 = item(1u);
    let speed_now = pace();
    if (i == 0u) {
        let b = beat_now();
        return vec4<f32>((c0.x + dt * P.speed * 0.3 * speed_now) % 10000.0, V.beat, b.beats % 65536.0, 1.0);
    }
    if (i == 1u) {
        let bass = ease(c1.x, clamp(broad_rel(BASS) - 0.5, 0.0, 2.0), 0.05, 0.3);
        let highs = ease(c1.y, clamp(0.5 * (broad_rel(PRESENCE) + broad_rel(AIR)) - 0.5, 0.0, 2.0), 0.03, 0.2);
        let turn = (c1.z + dt * P.turn * 0.12 * speed_now) % TAU;
        var flash = c1.w * exp(-dt / 0.3);
        if (beat_now().struck) {
            flash = 1.0;
        }
        return vec4<f32>(bass, highs, turn, flash);
    }
    let p = (i - G) / 2u;
    if (p >= count()) {
        return vec4<f32>(0.0);
    }
    let s = step_particle(p);
    if ((i - G) % 2u == 0u) {
        return vec4<f32>(s.pos, s.age);
    }
    return vec4<f32>(s.vel, s.life);
}

struct Eye {
    pos: vec3<f32>,
    right: vec3<f32>,
    up: vec3<f32>,
    fwd: vec3<f32>,
};

fn eye() -> Eye {
    let P = params();
    let yaw = item(1u).z;
    let pitch = mix(0.08, 1.4, clamp(P.view, 0.0, 1.0));
    var e: Eye;
    e.pos = EYE_DIST * vec3<f32>(sin(yaw) * cos(pitch), sin(pitch), -cos(yaw) * cos(pitch));
    e.fwd = normalize(-e.pos);
    e.right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), e.fwd));
    e.up = cross(e.fwd, e.right);
    return e;
}

fn vertex(i: u32) -> MeshVertex {
    let P = params();
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    v.colour = vec4<f32>(0.0);
    v.data = vec4<f32>(0.0);
    let p = i / 6u;
    if (p >= count()) {
        return v;
    }
    let a = item(G + 2u * p);
    let b = item(G + 2u * p + 1u);
    if (b.w <= 0.0 || a.w >= b.w) {
        return v;
    }
    let e = eye();
    let rel = a.xyz - e.pos;
    let vz = dot(rel, e.fwd);
    if (vz < 0.2) {
        return v;
    }
    var c = vec2<f32>(-1.0, -1.0);
    switch (i % 6u) {
        case 1u: { c = vec2<f32>(1.0, -1.0); }
        case 2u: { c = vec2<f32>(1.0, 1.0); }
        case 3u: { c = vec2<f32>(-1.0, -1.0); }
        case 4u: { c = vec2<f32>(1.0, 1.0); }
        case 5u: { c = vec2<f32>(-1.0, 1.0); }
        default: {}
    }
    let short = min(V.resolution.x, V.resolution.y);
    let hs = P.size * 2.0 / short * clamp(EYE_DIST / vz, 0.5, 2.0) * 1.6;
    let centre = vec2<f32>(dot(rel, e.right), dot(rel, e.up)) * FOCAL / vz;
    v.pos = vec4<f32>(centre + c * hs, 0.0, 1.0);
    let c1 = item(1u);
    let seed = rand(p, 53.0);
    var t = clamp(sqrt(length(b.xyz)) * 0.6, 0.0, 1.0);
    if (P.colouring > 1.5) {
        t = seed;
    } else if (P.colouring > 0.5 || (P.mode > 0.5 && P.mode < 1.5)) {
        // (The galaxy turns as one, so its speed is its place.)
        t = clamp(length(a.xyz) / 2.4, 0.0, 1.0);
    }
    // In over a quarter second, out over the last third of its life.
    var bright = smoothstep(0.0, 0.25, a.w) * (1.0 - smoothstep(0.66 * b.w, b.w, a.w));
    if (P.mode > 0.5 && P.mode < 1.5) {
        bright = 1.0 + 1.5 * P.kick * c1.w * exp(-length(a.xyz) * 3.0);
    }
    let sparkle = 1.0 + P.twinkle * c1.y * (0.5 + 0.5 * sin(V.time * 9.0 + seed * 60.0));
    let fog = clamp(1.6 - 0.15 * vz, 0.3, 1.0);
    v.colour = vec4<f32>(colour(t) * bright * sparkle * fog * P.brightness * 0.6, 0.0);
    v.data = vec4<f32>(c, 0.0, 0.0);
    return v;
}

// A bright core in a soft halo, ending at the quad's edge.
fn paint(f: MeshFragment) -> vec4<f32> {
    let r2 = dot(f.data.xy, f.data.xy);
    if (r2 >= 1.0) {
        return vec4<f32>(0.0);
    }
    let core = exp(-r2 * 9.0);
    let halo = exp(-r2 * 3.0) * 0.25 * (1.0 - r2);
    return vec4<f32>(f.colour.rgb * (core + halo), 0.0);
}
