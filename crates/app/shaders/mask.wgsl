// Modifier: mask. Keeps the picture inside (or outside) a soft shape, so two
// layers can share the frame by place instead of only by adding.
//   p0 = (shape, radius, softness, invert)   shape: 0 circle, 1 band, 2 hexagon

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn hex_dist(p: vec2<f32>) -> f32 {
    let q = abs(p);
    return max(q.y, dot(q, vec2<f32>(0.8660254, 0.5)));
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let aspect = M.resolution.x / M.resolution.y;
    let s = (in.uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0);
    let shape = u32(M.p0.x + 0.5);
    var d = length(s);
    if (shape == 1u) { d = abs(s.y); }
    if (shape == 2u) { d = hex_dist(s); }
    let soft = max(M.p0.z, 0.002);
    var m = 1.0 - smoothstep(M.p0.y - soft, M.p0.y + soft, d);
    if (M.p0.w > 0.5) { m = 1.0 - m; }
    return vec4<f32>(textureSample(src, samp, in.uv).rgb * m, 1.0);
}
