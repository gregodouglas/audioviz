// Modifier: colorify. The picture reprinted in one ink by its luminance: the
// layer tint's hue (saturated) turned round the circle by the drift, or the palette at a
// place set by the (levelled) brightness plus the drift. Brightness stays
// the picture's. With the halftone's one ink, the colour / monochrome half of
// Uberviz's look.
//   p0 = (strength, use palette, drift turns, level on)
//   p1 = (tint r, g, b, palette shift)
// Binding 3 is the layer meter's state (r = levelling gain).

struct GradeParams {
    m: ModParams,
    pal: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> G: GradeParams;
@group(0) @binding(3) var meter: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn pal(t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(G.pal[i].rgb, G.pal[(i + 1u) % 32u].rgb, fract(x));
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Hue turn in YIQ, as director.rs::hue_rotate.
fn turn(c: vec3<f32>, a: f32) -> vec3<f32> {
    let y = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    let i = dot(c, vec3<f32>(0.596, -0.274, -0.322));
    let q = dot(c, vec3<f32>(0.211, -0.523, 0.312));
    let s = sin(a);
    let co = cos(a);
    let i2 = i * co - q * s;
    let q2 = i * s + q * co;
    return max(vec3<f32>(y + 0.956 * i2 + 0.621 * q2, y - 0.272 * i2 - 0.647 * q2, y - 1.106 * i2 + 1.703 * q2), vec3<f32>(0.0));
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let M = G.m;
    let col = textureSample(src, samp, in.uv).rgb;
    let lum = luma(col);
    var ink: vec3<f32>;
    if (M.p0.y > 0.5) {
        var gain = 1.0;
        if (M.p0.w > 0.5) {
            gain = max(textureSampleLevel(meter, samp, vec2<f32>(0.5), 0.0).r, 1e-6);
        }
        // Soft knee so a levelled lit mean lands low on the palette and hot cores high.
        let l = 1.0 - exp(-lum * gain * 4.0);
        ink = pal(l * 0.8 + M.p1.w + M.p0.z);
    } else {
        // The tint's hue at full saturation: one ink should be a colour even
        // when the tint is pale; a grey tint stays grey.
        let t = turn(M.p1.rgb, M.p0.z * 6.2831853);
        let mx = max(t.r, max(t.g, t.b));
        let mn = min(t.r, min(t.g, t.b));
        let pure = (t - vec3<f32>(mn)) / max(mx - mn, 1e-4);
        ink = mix(vec3<f32>(1.0), pure, clamp((mx - mn) / max(mx, 1e-4) * 4.0, 0.0, 1.0));
    }
    ink = ink / max(luma(ink), 0.12);
    let out = mix(col, ink * lum, clamp(M.p0.x, 0.0, 1.0));
    return vec4<f32>(out, 1.0);
}
