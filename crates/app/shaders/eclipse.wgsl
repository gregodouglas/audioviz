// Eclipse: a black disc ringed by spectrum dots, with a corona at the rim.
// One fragment pass; every dot is evaluated (their halos overlap).

@group(0) @binding(0) var<uniform> E: EclipseParams;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

// Colour at a place round the ring (0..1 from the bottom, mirrored when on).
fn ring_colour(t: f32) -> vec3<f32> {
    var u = t;
    if (E.mirror > 0.5) { u = 1.0 - abs(1.0 - 2.0 * t); }
    let pal = eclipse_pal(E, u * E.range + E.palette_shift) * 1.2;
    return mix(E.tint.rgb, pal, E.palette_mix);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let half = 0.5 * min(E.resolution.x, E.resolution.y);
    let px = in.uv * E.resolution;
    let p = vec2<f32>(px.x - 0.5 * E.resolution.x, 0.5 * E.resolution.y - px.y) / half;
    let r = length(p);
    let aa = 1.5 / half;
    let n = u32(clamp(E.count, 1.0, 64.0));
    let step_a = 6.2831853 / f32(n);
    var col = vec3<f32>(0.0);
    for (var d = 0u; d < n; d++) {
        let lvl = pow(clamp(eclipse_level(E, d), 0.0, 4.0), max(E.curve, 0.1));
        let a = -1.5707963 + (f32(d) + 0.5) * step_a + E.spin;
        let rr = E.radius * (1.0 + 0.06 * E.react * min(lvl, 2.0));
        let c = vec2<f32>(cos(a), sin(a)) * rr;
        let s = max(E.size * (0.55 + 0.6 * E.react * min(lvl, 1.5)), 0.002);
        let dist = length(p - c);
        let core = 1.0 - smoothstep(s - aa, s + aa, dist);
        // Halo: a soft Lorentzian tail, stronger for loud dots.
        let halo = E.glow * s * s / (dist * dist + s * s) * (0.25 + 0.75 * min(lvl, 2.0));
        let bright = 0.15 + 1.6 * lvl;
        col += ring_colour((f32(d) + 0.5) / f32(n)) * (core * bright + halo * bright * 0.6);
    }
    // Corona: a bright thin rim and a soft outward glow that flares on the bass.
    let rd = E.radius * E.disc;
    let ang = atan2(p.y, select(p.x, 1e-6, abs(p.x) < 1e-6));
    let t = fract((ang + 1.5707963 - E.spin) / 6.2831853);
    let outside = max(r - rd, 0.0);
    let rim = exp(-outside / max(0.004, 0.006 * (1.0 + E.flare)));
    let soft = exp(-outside / (0.06 * (1.0 + 0.8 * E.flare)));
    col += ring_colour(t) * E.corona * (0.6 * rim + 0.35 * soft) * (0.4 + E.flare);
    // The disc is black: it masks everything behind it, dots included.
    let mask = smoothstep(rd - aa, rd + aa, r);
    return vec4<f32>(col * mask, 1.0);
}
