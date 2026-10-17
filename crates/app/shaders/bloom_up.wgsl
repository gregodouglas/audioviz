// 3x3 tent upsample, blended additively into the next larger level so the
// chain accumulates a wide, smooth glow.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> P: PostParams;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let o = 1.0 / vec2<f32>(textureDimensions(src));
    var c = textureSample(src, samp, in.uv).rgb * 4.0;
    c += textureSample(src, samp, in.uv + vec2<f32>(-o.x, 0.0)).rgb * 2.0;
    c += textureSample(src, samp, in.uv + vec2<f32>( o.x, 0.0)).rgb * 2.0;
    c += textureSample(src, samp, in.uv + vec2<f32>(0.0, -o.y)).rgb * 2.0;
    c += textureSample(src, samp, in.uv + vec2<f32>(0.0,  o.y)).rgb * 2.0;
    c += textureSample(src, samp, in.uv + vec2<f32>(-o.x, -o.y)).rgb;
    c += textureSample(src, samp, in.uv + vec2<f32>( o.x, -o.y)).rgb;
    c += textureSample(src, samp, in.uv + vec2<f32>(-o.x,  o.y)).rgb;
    c += textureSample(src, samp, in.uv + vec2<f32>( o.x,  o.y)).rgb;
    // Attenuate per level so the widest (haziest) levels contribute least:
    // the glow hugs the lines instead of lifting the whole background.
    return vec4<f32>(c / 16.0 * 0.6, 1.0);
}
