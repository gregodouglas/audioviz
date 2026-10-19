// A shared space's diffusion (ADR-0002): the glow its visuals wrote with
// write_glow, blurred at two sizes, added back into the space's picture
// (the pipeline blends by adding). The blurs come from glow_blur.wgsl, run
// over the light target: binding 0 is the narrow one (quarter size),
// binding 3 the wide one (eighth size).
//   p0 = (narrow weight, wide weight, 0, 0)

@group(0) @binding(0) var narrow: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var wide: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let a = textureSampleLevel(narrow, samp, in.uv, 0.0).rgb;
    let b = textureSampleLevel(wide, samp, in.uv, 0.0).rgb;
    return vec4<f32>(a * M.p0.x + b * M.p0.y, 0.0);
}
