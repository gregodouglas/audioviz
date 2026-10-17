// Modifier: ripple. Beats launch rings from the centre that bend the picture
// radially as they pass; a standing sine warp underneath if asked.
//   p0 = (amount, wavelength, speed, ring width)   p1 = ages of the last four rings (s)
//   p2 = (sine amount, sine frequency, phase, 0)

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;          // mirror-repeat
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

const PI: f32 = 3.14159265;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let aspect = M.resolution.x / M.resolution.y;
    let s = (in.uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0);
    let r = length(s);
    let dir = s / max(r, 1e-4);
    var d = 0.0;
    let ages = array<f32, 4>(M.p1.x, M.p1.y, M.p1.z, M.p1.w);
    for (var k = 0; k < 4; k++) {
        let age = ages[k];
        if (age < 0.0 || age > 6.0) { continue; }
        let front = age * M.p0.z;
        let x = r - front;
        let env = exp(-(x * x) / (2.0 * M.p0.w * M.p0.w)) * exp(-age * 0.6);
        d += M.p0.x * env * sin(2.0 * PI * x / max(M.p0.y, 0.01));
    }
    d += M.p2.x * sin(r * M.p2.y - M.p2.z);
    let q = s + dir * d;
    let suv = (q * vec2<f32>(1.0 / aspect, -1.0) + 1.0) * 0.5;
    return vec4<f32>(textureSample(src, samp, suv).rgb, 1.0);
}
