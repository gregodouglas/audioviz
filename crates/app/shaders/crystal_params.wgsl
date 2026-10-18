// Crystal uniform and shard record (scenes/crystal.rs). In the prelude so
// crystal.wgsl sees them.
struct CrystalParams {
    view_proj: mat4x4<f32>,
    resolution: vec2<f32>,
    width: f32,           // edge line width, pixels
    time: f32,
    tint: vec4<f32>,
    palette_mix: f32,
    palette_shift: f32,
    facets: f32,          // facet opacity gain
    edges: f32,           // edge brightness gain
    palette: array<vec4<f32>, 32>,
};

struct Shard {
    model: mat4x4<f32>,
    colour: vec4<f32>,    // rgb + facet opacity
    extra: vec4<f32>,     // (edge gain, palette position, 0, 0)
};

fn crystal_pal(cp: CrystalParams, t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(cp.palette[i].rgb, cp.palette[(i + 1u) % 32u].rgb, fract(x));
}
