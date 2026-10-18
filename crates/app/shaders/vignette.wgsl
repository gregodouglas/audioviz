// Modifier: vignette. The frame's edges darkened toward black. Distance is a
// superellipse that runs from the frame's own rectangle (roundness 0) to a
// circle (1), scaled so the corners sit at 1 either way.
//   p0 = (amount, softness, roundness, opening)

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
    let col = textureSample(src, samp, in.uv).rgb;
    let rnd = clamp(M.p0.z, 0.0, 1.0);
    let c = abs(in.uv * 2.0 - 1.0);
    // A circle in pixels: the aspect folded in, corners kept at sqrt 2.
    let aspect = M.resolution.x / max(M.resolution.y, 1.0);
    let k = c * vec2<f32>(aspect, 1.0) * (1.41421356 / length(vec2<f32>(aspect, 1.0)));
    let q = mix(c, k, rnd);
    let n = mix(6.0, 2.0, rnd);
    let d = pow(pow(q.x, n) + pow(q.y, n), 1.0 / n) / pow(2.0, 1.0 / n);
    let soft = clamp(M.p0.y, 0.02, 1.0);
    let e1 = 1.05 + M.p0.w;
    let e0 = e1 - soft * 1.1;
    let fall = smoothstep(e0, e1, d);
    return vec4<f32>(col * (1.0 - clamp(M.p0.x, 0.0, 1.0) * fall), 1.0);
}
