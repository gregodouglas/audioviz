// Wheel: a disc of spectrum wedges, each wedge's brightness its band's level
// raised to a power, colours running round the palette; outside the disc
// the wedges carry on as fading rays. One fragment pass.
// Inspired by the Wheel in Felix Turner's Uberviz (Lantern); written fresh.

@group(0) @binding(0) var<uniform> W: WheelParams;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn wedge_colour(i: u32, n: f32) -> vec3<f32> {
    let lvl = clamp(wheel_level(W, i), 0.0, 1.5);
    let bright = pow(lvl, max(W.curve, 0.1));
    let pal = wheel_pal(W, f32(i) / n * W.range + W.palette_shift) * 1.2;
    return mix(W.tint.rgb, pal, W.palette_mix) * bright;
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let half = min(W.resolution.x, W.resolution.y) * 0.5;
    let p = (in.uv - 0.5) * W.resolution / half;
    let r = length(p);
    let n = max(W.count, 1.0);
    let ang = atan2(p.y, p.x) / 6.2831853 + 0.5 + W.spin;
    let w = fract(ang) * n;
    let i = u32(floor(w)) % u32(n);
    let f = fract(w);
    // Soft edge between a wedge and its neighbours.
    let soft = max(W.soft, 1e-3);
    var col = wedge_colour(i, n);
    if (f < soft) {
        let j = (i + u32(n) - 1u) % u32(n);
        col = mix(wedge_colour(j, n), col, 0.5 + 0.5 * f / soft);
    } else if (f > 1.0 - soft) {
        let j = (i + 1u) % u32(n);
        col = mix(col, wedge_colour(j, n), 0.5 * (f - (1.0 - soft)) / soft);
    }
    // The disc, a dark centre, and rays fading out beyond the rim.
    let rim = W.radius;
    var shape = 1.0 - smoothstep(rim - 0.01, rim + 0.01, r);
    if (W.rays > 0.0) {
        let beyond = max(r - rim, 0.0) / max(W.rays * 2.0, 1e-3);
        shape = max(shape, 0.55 * exp(-beyond * beyond * 2.0) * step(rim, r));
    }
    let hole = rim * W.hole;
    shape *= smoothstep(hole, hole + 0.02, r);
    return vec4<f32>(col * shape, 1.0);
}
