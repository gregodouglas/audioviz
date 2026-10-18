// Wheel uniform (scenes/wheel.rs::Uniform). In the prelude so wheel.wgsl sees it.
struct WheelParams {
    resolution: vec2<f32>,
    count: f32,           // wedges
    spin: f32,            // rotation, radians
    radius: f32,          // disc radius, fraction of the short half-side
    soft: f32,            // wedge edge softness, fraction of a wedge
    rays: f32,            // how far wedges continue outside the disc as fading rays
    curve: f32,           // brightness = level ^ curve
    range: f32,           // fraction of the palette the wheel spans
    hole: f32,            // dark centre radius, fraction of the disc
    palette_mix: f32,
    palette_shift: f32,
    tint: vec4<f32>,
    levels: array<vec4<f32>, 16>,   // 64 wedge levels
    palette: array<vec4<f32>, 32>,
};

fn wheel_level(wp: WheelParams, i: u32) -> f32 {
    let v = wp.levels[(i / 4u) % 16u];
    switch (i % 4u) {
        case 0u: { return v.x; }
        case 1u: { return v.y; }
        case 2u: { return v.z; }
        default: { return v.w; }
    }
}

fn wheel_pal(wp: WheelParams, t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(wp.palette[i].rgb, wp.palette[(i + 1u) % 32u].rgb, fract(x));
}
