// Modifier: scanlines. Fine stripes over the picture: a multiplicative cut
// (the broadcast texture) plus a faint additive glow in the tint so the
// stripes also sit over black. The phase crawls with the music.
//   p0 = (pitch px, angle rad, cut, glow)   p1 = (tint r, g, b, phase)

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
    let px = in.uv * M.resolution;
    let a = M.p0.y;
    let d = px.x * sin(a) + px.y * cos(a);
    let stripe = 0.5 + 0.5 * sin(d * 6.2831853 / max(M.p0.x, 1.0) + M.p1.w);
    let col = textureSample(src, samp, in.uv).rgb;
    let cut = 1.0 - M.p0.z * stripe;
    let out = col * cut + M.p1.rgb * M.p0.w * stripe;
    return vec4<f32>(out, 1.0);
}
