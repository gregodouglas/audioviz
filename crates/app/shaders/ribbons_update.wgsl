// Advect particles through a curl-noise field plus twelve chroma-driven
// vortices on a circle-of-fifths wheel. Coordinates: y in [-1, 1], x in
// [-aspect, aspect].

@group(0) @binding(0) var<uniform> P: RibbonParams;
@group(0) @binding(1) var<storage, read_write> particles: array<Particle>;

fn aspect() -> f32 {
    return P.resolution.x / P.resolution.y;
}

fn respawn(i: u32, p: ptr<function, Particle>, t: f32) {
    let r = hash22(vec2<f32>(f32(i) * 0.731, t + P.seed));
    let r2 = hash22(vec2<f32>(f32(i) * 1.117 + 5.0, t * 0.37 + P.seed));
    (*p).pos = vec2<f32>((r.x * 2.0 - 1.0) * aspect() * 1.05, (r.y * 2.0 - 1.0) * 1.05);
    (*p).prev = (*p).pos;
    (*p).age = 0.0;
    (*p).life = P.lifetime * (0.5 + r2.x);
    (*p).seed = r2.y;
    (*p).pitch = floor(hash21(vec2<f32>(f32(i), 3.3 + t)) * 12.0);
}

fn field(p: vec2<f32>) -> vec2<f32> {
    // Curl of a scalar potential: divergence-free, so lines never pile up.
    let s = P.field_scale;
    let t = P.time * 0.08;
    let e = 0.01;
    let dx = potential((p + vec2<f32>(e, 0.0)) * s, t) - potential((p - vec2<f32>(e, 0.0)) * s, t);
    let dy = potential((p + vec2<f32>(0.0, e)) * s, t) - potential((p - vec2<f32>(0.0, e)) * s, t);
    var v = vec2<f32>(dy, -dx) / (2.0 * e) * P.curl * 0.35;

    // Pitch-class vortices: twelve centers on a circle, strength = chroma.
    let radius = 0.62;
    for (var i = 0u; i < 12u; i++) {
        let c = chroma_at(P, i);
        if (c < 0.02) { continue; }
        // Circle of fifths so harmonically close classes are neighbours.
        let ang = f32((i * 7u) % 12u) / 12.0 * 6.2831853 + P.rotation;
        let center = vec2<f32>(cos(ang), sin(ang)) * radius;
        let d = p - center;
        let r = length(d) + 0.08;
        let tangent = vec2<f32>(-d.y, d.x) / r;
        // Alternate spin so adjacent vortices shear rather than co-rotate.
        let spin = select(1.0, -1.0, (i & 1u) == 1u);
        v += tangent * spin * c * c * P.vortex * 0.9 / r;
    }
    return v;
}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= P.point_count) { return; }
    var p = particles[i];

    let speed = P.speed * (0.6 + 0.4 * P.bass) * (1.0 + 1.5 * P.onset);
    let v = field(p.pos);
    p.prev = p.pos;
    p.pos += v * speed * P.dt;
    p.age += P.dt;

    let a = aspect() * 1.1;
    if (p.age > p.life || abs(p.pos.x) > a || abs(p.pos.y) > 1.1) {
        respawn(i, &p, P.time);
    }
    particles[i] = p;
}
