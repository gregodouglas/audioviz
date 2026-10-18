// Ripples uniform (scenes/ripples.rs::Uniform). In the prelude so ripples.wgsl sees it.
struct RipplesParams {
    resolution: vec2<f32>,
    time: f32,
    print: u32,           // 0 contours, 1 dots, 2 grid
    height: f32,          // hill height, world units (bass-swelled)
    scale: f32,           // noise frequency
    flow: f32,            // accumulated flow toward the viewer
    density: f32,         // contour lines / dots / grid lines
    width: f32,           // line width or dot radius, pixels
    tilt: f32,            // camera pitch 0..1
    fade: f32,            // distance darkening
    grid: u32,            // mesh cells per side
    tint: vec4<f32>,
    palette_mix: f32,
    palette_shift: f32,
    _pad0: f32,
    _pad1: f32,
    rings: array<vec4<f32>, 4>,   // (x, z, birth time, strength)
    palette: array<vec4<f32>, 32>,
};

fn ripples_pal(rp: RipplesParams, t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(rp.palette[i].rgb, rp.palette[(i + 1u) % 32u].rgb, fract(x));
}
