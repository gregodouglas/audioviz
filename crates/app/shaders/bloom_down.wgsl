// Dual-filter (Kawase) downsample: 5 taps, half resolution per level.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> P: Params;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(src));
    let o = texel;
    var c = textureSample(src, samp, in.uv).rgb * 4.0;
    c += textureSample(src, samp, in.uv + vec2<f32>(-o.x, -o.y)).rgb;
    c += textureSample(src, samp, in.uv + vec2<f32>( o.x, -o.y)).rgb;
    c += textureSample(src, samp, in.uv + vec2<f32>(-o.x,  o.y)).rgb;
    c += textureSample(src, samp, in.uv + vec2<f32>( o.x,  o.y)).rgb;
    return vec4<f32>(c / 8.0, 1.0);
}
