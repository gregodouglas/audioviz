// Modifier: edge. Sobel outline of the picture, in its own colours, the
// layer tint, or the layer's palette (position by the edge's direction, turned
// by the palette shift), with an optional share of the original kept underneath.
//   p0 = (strength, keep, colour 0 picture | 1 tint | 2 palette, palette shift)
//   p1 = (tint r, g, b, 0)

struct GradeParams {
    m: ModParams,
    pal: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> G: GradeParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

fn pal(t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    let f = fract(x);
    return mix(G.pal[i].rgb, G.pal[(i + 1u) % 32u].rgb, f);
}

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
    let M = G.m;
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
    var hue = c / lum;                       // the picture's own hue at unit brightness
    if (M.p0.z > 1.5) {
        // Direction of the gradient picks the palette colour, so each edge
        // keeps its colour as it moves and a curve runs through the palette.
        // Flat areas have no direction; keep atan2 off (0, 0) so no NaN
        // reaches the levelling meter.
        let ang = select(0.0, atan2(gy, gx) / 6.2831853, g > 1e-6);
        let p = pal(ang + M.p0.w);
        hue = p / max(dot(p, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.12);
    } else if (M.p0.z > 0.5) {
        hue = M.p1.rgb;
    }
    let col = hue * g * M.p0.x + c * M.p0.y;
    return vec4<f32>(col, 1.0);
}
