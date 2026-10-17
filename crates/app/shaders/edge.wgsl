// Modifier: edge. Sobel outline of the picture, in its own colours or the
// layer tint, with an optional share of the original kept underneath.
//   p0 = (strength, keep, tinted, 0)   p1 = (tint r, g, b, 0)

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn lum_at(uv: vec2<f32>) -> f32 {
    let c = textureSampleLevel(src, samp, uv, 0.0).rgb;
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let px = 1.0 / M.resolution;
    let tl = lum_at(in.uv + px * vec2<f32>(-1.0, -1.0));
    let t = lum_at(in.uv + px * vec2<f32>(0.0, -1.0));
    let tr = lum_at(in.uv + px * vec2<f32>(1.0, -1.0));
    let l = lum_at(in.uv + px * vec2<f32>(-1.0, 0.0));
    let rr = lum_at(in.uv + px * vec2<f32>(1.0, 0.0));
    let bl = lum_at(in.uv + px * vec2<f32>(-1.0, 1.0));
    let b = lum_at(in.uv + px * vec2<f32>(0.0, 1.0));
    let br = lum_at(in.uv + px * vec2<f32>(1.0, 1.0));
    let gx = (tr + 2.0 * rr + br) - (tl + 2.0 * l + bl);
    let gy = (bl + 2.0 * b + br) - (tl + 2.0 * t + tr);
    let g = sqrt(gx * gx + gy * gy);
    let c = textureSample(src, samp, in.uv).rgb;
    let lum = max(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)), 1e-4);
    let own = c / lum;                       // the picture's own hue at unit brightness
    let hue = mix(own, M.p1.rgb, M.p0.z);
    let col = hue * g * M.p0.x + c * M.p0.y;
    return vec4<f32>(col, 1.0);
}
