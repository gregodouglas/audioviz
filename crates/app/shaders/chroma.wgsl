// Modifier: chromatic split. Red and blue are read a little away from green,
// radially from the centre or sideways, so edges fringe like a cheap lens.
//   p0 = (amount, radial, 0, 0)

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let aspect = M.resolution.x / M.resolution.y;
    let s = (in.uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0);
    var dir = vec2<f32>(1.0, 0.0);
    if (M.p0.y > 0.5) {
        dir = s / max(length(s), 1e-4) * min(length(s), 1.0);
    }
    let off = dir * M.p0.x * vec2<f32>(1.0 / aspect, -1.0) * 0.5;
    let r = textureSample(src, samp, in.uv + off).r;
    let g = textureSample(src, samp, in.uv).g;
    let b = textureSample(src, samp, in.uv - off).b;
    return vec4<f32>(r, g, b, 1.0);
}
