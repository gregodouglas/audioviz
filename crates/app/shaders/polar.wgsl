// Modifier: polar wrap. Wrap = the source's x becomes angle and y becomes
// radius (a scope becomes a ring, ribbons a vortex). Unwrap = the inverse
// (a ring becomes a strip).
//   p0 = (unwrap, radius, spin, 0)

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
    let radius = max(M.p0.y, 0.05);
    var suv: vec2<f32>;
    if (M.p0.x > 0.5) {
        // Unwrap: this pixel's x is an angle, its y a radius; read the source there.
        let ang = (in.uv.x + M.p0.z / (2.0 * PI)) * 2.0 * PI;
        let r = (1.0 - in.uv.y) * radius;
        let q = vec2<f32>(cos(ang), sin(ang)) * r;
        suv = (q * vec2<f32>(1.0 / aspect, -1.0) + 1.0) * 0.5;
    } else {
        // Wrap: this pixel's angle and radius pick a column and row of the source.
        let r = length(s) / radius;
        let ang = atan2(s.y, s.x) + M.p0.z;
        suv = vec2<f32>(fract(ang / (2.0 * PI)), 1.0 - r);
    }
    let col = textureSample(src, samp, suv).rgb;
    // Fade the far edge of a wrap so the tiling does not show as a hard ring.
    let fade = select(1.0, 1.0 - smoothstep(0.95, 1.05, length(s) / radius), M.p0.x <= 0.5);
    return vec4<f32>(col * fade, 1.0);
}
