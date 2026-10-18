// Eclipse uniform (scenes/eclipse.rs::Uniform). In the prelude so eclipse.wgsl sees it.
struct EclipseParams {
    resolution: vec2<f32>,
    count: f32,           // dots
    spin: f32,            // rotation, radians
    radius: f32,          // ring radius, fraction of the short half-side
    disc: f32,            // black disc radius, fraction of the ring
    size: f32,            // dot radius, fraction of the short half-side
    glow: f32,            // halo strength
    corona: f32,          // rim glow
    flare: f32,           // corona flare from the bass, 0..~2
    curve: f32,           // brightness = level ^ curve
    range: f32,           // fraction of the palette the ring spans
    react: f32,           // loud dots swell and push out
    mirror: f32,          // 1 = bands mirrored up both sides
    palette_mix: f32,
    palette_shift: f32,
    tint: vec4<f32>,
    levels: array<vec4<f32>, 16>,   // 64 dot levels
    palette: array<vec4<f32>, 32>,
};

fn eclipse_level(ep: EclipseParams, i: u32) -> f32 {
    let v = ep.levels[(i / 4u) % 16u];
    switch (i % 4u) {
        case 0u: { return v.x; }
        case 1u: { return v.y; }
        case 2u: { return v.z; }
        default: { return v.w; }
    }
}

fn eclipse_pal(ep: EclipseParams, t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(ep.palette[i].rgb, ep.palette[(i + 1u) % 32u].rgb, fract(x));
}
