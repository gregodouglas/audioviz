// Modifier: halftone. The picture re-printed as a screen of dots (or hatch
// lines) on a rotated grid; each cell's mark grows with the brightness under
// it. Marks keep the picture's colour, or go monotone in the tint / palette;
// `mono` can drift with time and the music so the print flickers between
// colour and one ink.
//   p0 = (cell px, angle rad, shape 0 dot | 1 line | 2 cross, mono 0..1)
//   p1 = (tint r, g, b, invert)
//   p2 = (palette shift, use palette, gamma, 0)

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
    return mix(G.pal[i].rgb, G.pal[(i + 1u) % 32u].rgb, fract(x));
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let M = G.m;
    let cell = max(M.p0.x, 2.0);
    let a = M.p0.y;
    let shape = u32(M.p0.z + 0.5);
    let px = in.uv * M.resolution;
    // Rotate pixel space, find the cell and the pixel's offset from its centre.
    let c = cos(a);
    let s = sin(a);
    let r = vec2<f32>(c * px.x - s * px.y, s * px.x + c * px.y);
    let cell_id = floor(r / cell);
    let centre_r = (cell_id + 0.5) * cell;
    let d = r - centre_r;
    // Back to pixel space for the sample at the cell centre (a 2x2 box so
    // thin lines still register).
    let centre = vec2<f32>(c * centre_r.x + s * centre_r.y, -s * centre_r.x + c * centre_r.y);
    let cuv = centre / M.resolution;
    let o = 0.25 * cell / M.resolution;
    var col = textureSampleLevel(src, samp, cuv + vec2<f32>(-o.x, -o.y), 0.0).rgb;
    col += textureSampleLevel(src, samp, cuv + vec2<f32>(o.x, -o.y), 0.0).rgb;
    col += textureSampleLevel(src, samp, cuv + vec2<f32>(-o.x, o.y), 0.0).rgb;
    col += textureSampleLevel(src, samp, cuv + vec2<f32>(o.x, o.y), 0.0).rgb;
    col *= 0.25;
    // Brightness → mark size. Scene-referred, so soft-knee into 0..1.
    var l = 1.0 - exp(-luma(col) * max(M.p2.z, 0.1));
    if (M.p1.w > 0.5) { l = 1.0 - l; }
    let half = cell * 0.5;
    var dist: f32;
    switch (shape) {
        case 1u: { dist = abs(d.y); }
        case 2u: { dist = min(abs(d.x), abs(d.y)); }
        default: { dist = length(d); }
    }
    // Dots cover the cell at full brightness (radius ~0.71 cell for a square
    // of ink); lines and crosses fill the cell width.
    var size: f32;
    if (shape == 0u) { size = sqrt(l) * half * 1.42; } else { size = l * half; }
    let cov = 1.0 - smoothstep(size - 0.8, size + 0.8, dist);
    // Ink colour: the picture's own at unit brightness, or one ink.
    let lum = max(luma(col), 1e-4);
    let own = col / lum;
    var ink = M.p1.rgb;
    if (M.p2.y > 0.5) {
        ink = pal(l * 0.8 + M.p2.x);
        ink = ink / max(luma(ink), 0.12);
    }
    let hue = mix(own, ink, clamp(M.p0.w, 0.0, 1.0));
    // Keep the picture's energy: ink brightness follows the cell's luminance.
    let out = hue * lum * cov * 1.3;
    return vec4<f32>(out, 1.0);
}
