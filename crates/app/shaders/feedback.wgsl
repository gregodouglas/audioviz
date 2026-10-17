// Modifier: feedback / echo. This frame plus last frame's result, re-sampled
// with a zoom and a spin about the centre and faded. The classic warp: an
// infinite tunnel out of anything. The aux texture is the layer's history.
//   p0 = (decay, zoom factor this frame, spin this frame, 0)   p1 = (drift x, drift y, 0, 0)

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var hist: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let aspect = M.resolution.x / M.resolution.y;
    let s = (in.uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0);
    // Where this pixel was last frame: undo the zoom and spin.
    let a = -M.p0.z;
    let c = cos(a);
    let sn = sin(a);
    let r = vec2<f32>(c * s.x - sn * s.y, sn * s.x + c * s.y) / max(M.p0.y, 0.2) - M.p1.xy;
    let huv = (r * vec2<f32>(1.0 / aspect, -1.0) + 1.0) * 0.5;
    let inside = step(0.0, huv.x) * step(huv.x, 1.0) * step(0.0, huv.y) * step(huv.y, 1.0);
    let prev = textureSampleLevel(hist, samp, huv, 0.0).rgb * inside;
    let cur = textureSample(src, samp, in.uv).rgb;
    // Bounded so a long decay cannot run away.
    let col = min(cur + prev * M.p0.x, vec3<f32>(12.0));
    return vec4<f32>(col, 1.0);
}
