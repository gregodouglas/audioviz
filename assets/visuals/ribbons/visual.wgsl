// Flow-field ribbons: particles advected through a curl-noise field plus
// twelve vortices, one per pitch class on a circle-of-fifths wheel, each as
// strong as its note. Every particle draws its last step as a soft stroke
// into a trail that fades by Persistence. Coordinates: y in [-1, 1], x in
// [-aspect, aspect] (the height is the unit, as in the compiled-in Ribbons).
//
// Each particle is two prepare items, kept from frame to frame:
//   2k      (position x, y, previous position x, y)
//   2k + 1  (age, life, seed, pitch class)
// Both items of a pair run the same update, each keeping its half. The last
// item (SHARED) is the section's colour offset, eased.

const PARTICLES: u32 = 65536u;
const LIFETIME: f32 = 5.0;

fn aspect() -> f32 {
    return V.resolution.x / V.resolution.y;
}

fn particle_count() -> u32 {
    return u32(clamp(params().count, 256.0, f32(PARTICLES)));
}

fn seed() -> f32 {
    return V.instance * 100.0;
}

// ---------------------------------------------------------------- the field

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn vnoise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash31(i + vec3<f32>(0.0, 0.0, 0.0)), hash31(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let b = mix(hash31(i + vec3<f32>(0.0, 1.0, 0.0)), hash31(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let c = mix(hash31(i + vec3<f32>(0.0, 0.0, 1.0)), hash31(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let d = mix(hash31(i + vec3<f32>(0.0, 1.0, 1.0)), hash31(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z) * 2.0 - 1.0;
}

fn potential(p: vec2<f32>, t: f32) -> f32 {
    var s = 0.0;
    s += vnoise3(vec3<f32>(p, t)) * 1.0;
    s += vnoise3(vec3<f32>(p * 2.1 + 17.0, t * 1.3)) * 0.5;
    s += vnoise3(vec3<f32>(p * 4.3 + 31.0, t * 1.7)) * 0.25;
    return s;
}

fn field(p: vec2<f32>) -> vec2<f32> {
    let P = params();
    // Curl of a scalar potential: divergence-free, so lines never pile up.
    // The bass breathes the field's scale (eased: raw, it warped every
    // particle's path each frame), as much as the layer's Motion says.
    let s = P.scale * (1.0 + 0.3 * motion() * clamp(bass_soft() - 1.0, -0.5, 1.0));
    let t = V.time * 0.08;
    let e = 0.01;
    let dx = potential((p + vec2<f32>(e, 0.0)) * s, t) - potential((p - vec2<f32>(e, 0.0)) * s, t);
    let dy = potential((p + vec2<f32>(0.0, e)) * s, t) - potential((p - vec2<f32>(0.0, e)) * s, t);
    var v = vec2<f32>(dy, -dx) / (2.0 * e) * P.curl * 0.35;
    // Pitch-class vortices: twelve centres on a circle, strength = chroma.
    let radius = 0.62;
    let turn = V.time * 0.02;
    for (var i = 0u; i < 12u; i++) {
        let c = chroma(i);
        if (c < 0.02) { continue; }
        // Circle of fifths so harmonically close classes are neighbours.
        let ang = f32((i * 7u) % 12u) / 12.0 * TAU + turn;
        let centre = vec2<f32>(cos(ang), sin(ang)) * radius;
        let d = p - centre;
        let r = length(d) + 0.08;
        let tangent = vec2<f32>(-d.y, d.x) / r;
        // Alternate spin so adjacent vortices shear rather than co-rotate.
        let spin = select(1.0, -1.0, (i & 1u) == 1u);
        v += tangent * spin * c * c * P.vortex * 0.9 / r;
    }
    return v;
}

// ---------------------------------------------------------------- particles

struct Particle {
    pos: vec2<f32>,
    prev: vec2<f32>,
    age: f32,
    life: f32,
    seed: f32,
    pitch: f32,
};

fn respawn(k: u32, t: f32, stagger: bool) -> Particle {
    let r = hash22(vec2<f32>(f32(k) * 0.731, t + seed()));
    let r2 = hash22(vec2<f32>(f32(k) * 1.117 + 5.0, t * 0.37 + seed()));
    var p: Particle;
    p.pos = vec2<f32>((r.x * 2.0 - 1.0) * aspect() * 1.05, (r.y * 2.0 - 1.0) * 1.05);
    p.prev = p.pos;
    p.life = LIFETIME * (0.5 + r2.x);
    // The first particles start part-way through their lives, so they do not all respawn together.
    p.age = select(0.0, hash21(vec2<f32>(f32(k), 9.1 + seed())) * p.life, stagger);
    p.seed = r2.y;
    p.pitch = floor(hash21(vec2<f32>(f32(k), 3.3 + t)) * 12.0);
    return p;
}

fn particle(k: u32) -> Particle {
    let a = item(2u * k);
    let b = item(2u * k + 1u);
    var p: Particle;
    p.pos = a.xy;
    p.prev = a.zw;
    p.age = b.x;
    p.life = b.y;
    p.seed = b.z;
    p.pitch = b.w;
    return p;
}

const SHARED: u32 = 131072u;

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    if (i == SHARED) {
        return vec4<f32>(ease(item(SHARED).x, section_seed(4.0) * 0.5 * P.sections, 3.0, 3.0), 0.0, 0.0, 0.0);
    }
    let k = i / 2u;
    if (k >= particle_count()) {
        return item(i);
    }
    var p = particle(k);
    if (p.life <= 0.0) {
        p = respawn(k, V.time, true);
    } else {
        let dt = min(V.dt, 0.05);
        let speed = P.speed * (1.0 + motion() * (0.4 * (min(bass_soft(), 2.0) - 1.0) + 1.2 * onset_soft() + P.rush * build()));
        p.prev = p.pos;
        p.pos += field(p.pos) * speed * dt;
        p.age += dt;
        let edge = aspect() * 1.1;
        if (p.age > p.life || abs(p.pos.x) > edge || abs(p.pos.y) > 1.1) {
            p = respawn(k, V.time, false);
        }
    }
    if (i % 2u == 0u) {
        return vec4<f32>(p.pos, p.prev);
    }
    return vec4<f32>(p.age, p.life, p.seed, p.pitch);
}

// ---------------------------------------------------------------- drawing

fn hsv2rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let k = vec3<f32>(0.0, 2.0, 4.0) + h * 6.0;
    let f = abs(fract(k / 6.0) * 6.0 - 3.0) - 1.0;
    return v * mix(vec3<f32>(1.0), clamp(f, vec3<f32>(0.0), vec3<f32>(1.0)), s);
}

fn palette_at(t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
}

fn to_px(p: vec2<f32>) -> vec2<f32> {
    let half = V.resolution.y * 0.5;
    return V.resolution * 0.5 + vec2<f32>(p.x, -p.y) * half;
}

// Particle i / 6's last step as a quad round the segment prev -> pos.
fn vertex(i: u32) -> MeshVertex {
    let P = params();
    let k = i / 6u;
    let vi = i % 6u;
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    v.colour = vec4<f32>(0.0);
    v.data = vec4<f32>(0.0);
    if (k >= particle_count()) {
        return v;
    }
    let part = particle(k);
    let a = to_px(part.prev);
    let b = to_px(part.pos);
    var d = b - a;
    let len = length(d);
    // A respawn jump or a brand-new particle: draw nothing.
    if (len < 1e-4 || len > 60.0 || part.age < 1e-4) {
        return v;
    }
    d = d / len;
    let n = vec2<f32>(-d.y, d.x);
    let w = P.width * 2.5 + 1.0;
    var along: f32;
    var across: f32;
    switch (vi) {
        case 0u: { along = -w;       across = -w; }
        case 1u: { along = len + w;  across = -w; }
        case 2u: { along = -w;       across =  w; }
        case 3u: { along = -w;       across =  w; }
        case 4u: { along = len + w;  across = -w; }
        default: { along = len + w;  across =  w; }
    }
    let px = a + d * along + n * across;
    let ndc = vec2<f32>(px.x / V.resolution.x * 2.0 - 1.0, 1.0 - px.y / V.resolution.y * 2.0);
    let short = min(V.resolution.x, V.resolution.y);
    v.pos = vec4<f32>(ndc * V.resolution / short, 0.0, 1.0);
    // Colour: the pitch class's hue on the circle of fifths pulled toward the
    // key's, fading in and out over the particle's life. With a palette, the
    // chord sits round the palette's moving position, each class offset by
    // its distance from the key times the spread.
    let key_hue = f32((u32(V.key) * 7u) % 12u) / 12.0;
    let class_hue = f32((u32(part.pitch) * 7u) % 12u) / 12.0;
    let sec = item(SHARED).x;
    let hue = fract(mix(key_hue, class_hue, 0.6) + sec);
    let fade = smoothstep(0.0, 0.6, part.age) * (1.0 - smoothstep(part.life - 1.0, part.life, part.age));
    let own = mix(V.tint.rgb, hsv2rgb(hue, P.saturation, 1.0), 0.85);
    var off = class_hue - key_hue;
    off = off - round(off);
    let pal = palette_at(V.palette_shift + sec + off * P.spread + (part.seed - 0.5) * 0.04 * P.spread) * 1.3;
    v.colour = vec4<f32>(mix(own, pal, V.palette_mix) * fade, 0.0);
    v.data = vec4<f32>(along, across, len, 0.0);
    return v;
}

// The stroke: a Gaussian cross-section, added.
fn paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    let along = clamp(f.data.x, 0.0, f.data.z);
    let dist = length(vec2<f32>(f.data.x - along, f.data.y));
    let sigma = max(P.width, 0.3);
    let falloff = exp(-(dist * dist) / (2.0 * sigma * sigma));
    return vec4<f32>(f.colour.rgb * falloff * P.intensity * 0.004, 0.0);
}

// The trail: last frame fading by Persistence per second, with a tiny floor
// taken off so it reaches black.
fn shade(uv: vec2<f32>) -> vec3<f32> {
    let decay = pow(max(params().persistence, 1e-6), V.dt);
    return max(prev(uv) * decay - vec3<f32>(0.0005), vec3<f32>(0.0));
}
