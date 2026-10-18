// Glow, the sub-passes before the composite (glow.wgsl). Rendered into the
// slot's two quarter-resolution textures.
//   mode 0, prefilter: a 4x4 box down from the full picture (four bilinear
//     taps), keeping only what clears the threshold with a soft knee. The
//     threshold is measured on the levelled picture (binding 3 is the layer
//     meter's state, r = gain) so it means the same for a dim or a hot layer.
//   mode 1, blur: a 25-tap Gaussian along `dir` (texels of this texture).
//   p0 = (mode, dir x, dir y, sigma texels)   p1 = (threshold, level on, 0, 0)

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var meter: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(src));
    if (M.p0.x < 0.5) {
        var c = textureSampleLevel(src, samp, in.uv + vec2<f32>(-texel.x, -texel.y), 0.0).rgb;
        c += textureSampleLevel(src, samp, in.uv + vec2<f32>(texel.x, -texel.y), 0.0).rgb;
        c += textureSampleLevel(src, samp, in.uv + vec2<f32>(-texel.x, texel.y), 0.0).rgb;
        c += textureSampleLevel(src, samp, in.uv + vec2<f32>(texel.x, texel.y), 0.0).rgb;
        c *= 0.25;
        var gain = 1.0;
        if (M.p1.y > 0.5) {
            gain = max(textureSampleLevel(meter, samp, vec2<f32>(0.5), 0.0).r, 1e-6);
        }
        let l = luma(c) * gain;
        let t = max(M.p1.x, 0.0);
        let knee = max(t * 0.5, 1e-4);
        let soft = clamp(l - t + knee, 0.0, 2.0 * knee);
        let keep = max(soft * soft / (4.0 * knee), l - t) / max(l, 1e-6);
        return vec4<f32>(c * clamp(keep, 0.0, 1.0), 1.0);
    }
    let sigma = max(M.p0.w, 0.5);
    let stride = M.p0.yz * texel * (sigma / 4.0);
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var i = -12; i <= 12; i++) {
        // Taps a quarter sigma apart cover three sigma either side.
        let x = f32(i) * 0.25;
        let w = exp(-0.5 * x * x);
        sum += textureSampleLevel(src, samp, in.uv + stride * f32(i), 0.0).rgb * w;
        wsum += w;
    }
    return vec4<f32>(sum / wsum, 1.0);
}
