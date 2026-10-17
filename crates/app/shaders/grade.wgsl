// Modifier: palette grade. Recolours the layer's picture through its palette:
//   luma mode = a gradient map (dark -> first colours, bright -> last colours),
//   hue mode  = the picture's own hue picks the palette position.
// Brightness stays the picture's; the palette supplies the colour.
//   p0 = (mode, amount, shift, react)

struct GradeParams {
    m: ModParams,
    pal: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> G: GradeParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn pal(t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    let f = fract(x);
    return mix(G.pal[i].rgb, G.pal[(i + 1u) % 32u].rgb, f);
}

fn hue_of(c: vec3<f32>) -> f32 {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    let d = mx - mn;
    if (d < 1e-5) { return 0.0; }
    var h = 0.0;
    if (mx == c.r) { h = (c.g - c.b) / d; }
    else if (mx == c.g) { h = 2.0 + (c.b - c.r) / d; }
    else { h = 4.0 + (c.r - c.g) / d; }
    return fract(h / 6.0);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let c = textureSample(src, samp, in.uv).rgb;
    let lum = max(c.r, max(c.g, c.b));
    var t: f32;
    if (G.m.p0.x > 0.5) {
        t = hue_of(c);
    } else {
        // Soft map of scene-referred brightness onto the palette.
        t = 1.0 - exp(-lum * 1.6);
    }
    let p = pal(t + G.m.p0.z);
    let pl = max(dot(p, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.12);
    let graded = p * (lum / pl) * 0.9;
    let col = mix(c, graded, clamp(G.m.p0.y, 0.0, 1.0));
    return vec4<f32>(col, 1.0);
}
