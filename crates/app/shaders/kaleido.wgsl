// Kaleidoscope object cell: a window into a picture (album art or one of
// your photos) that drifts, turns and breathes with the music. The layer's
// fold turns it into the kaleidoscope; with the fold off it is a slideshow.

@group(0) @binding(0) var tex_a: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;          // mirror-repeat
@group(0) @binding(2) var<uniform> K: CellParams;
@group(0) @binding(3) var tex_b: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn window(s: vec2<f32>, scale: vec2<f32>, angle: f32, center: vec2<f32>) -> vec2<f32> {
    let c = cos(angle);
    let sn = sin(angle);
    let r = vec2<f32>(c * s.x - sn * s.y, sn * s.x + c * s.y);
    return center + vec2<f32>(r.x, -r.y) * scale;
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let aspect = K.resolution.x / K.resolution.y;
    let s = (in.uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0);
    let a = textureSample(tex_a, samp, window(s, K.scale_a, K.angle_a, K.center_a)).rgb;
    let b = textureSample(tex_b, samp, window(s, K.scale_b, K.angle_b, K.center_b)).rgb;
    var col = mix(a, b, K.mix_t);
    // Pull toward the tint when asked (own colour / album colour on a photo).
    let lum = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
    col = mix(col, K.tint.rgb * lum * 1.6, K.tint.w);
    // Breathe with energy; a brief flash on onsets.
    col *= K.gain * (0.75 + 0.25 * K.energy + 0.25 * K.onset);
    return vec4<f32>(col, 1.0);
}
