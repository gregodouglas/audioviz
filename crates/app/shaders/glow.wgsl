// Modifier: glow. The picture plus its own bright parts blurred wide (the
// quarter-resolution result of glow_blur.wgsl, binding 3), the heavy halo
// frame of Uberviz's Lantern. The glow keeps the picture's colours, so a
// palette carries straight through.
//   p0 = (amount, 0, 0, 0)

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var blurred: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let col = textureSample(src, samp, in.uv).rgb;
    let g = textureSample(blurred, samp, in.uv).rgb;
    return vec4<f32>(col + g * max(M.p0.x, 0.0), 1.0);
}
