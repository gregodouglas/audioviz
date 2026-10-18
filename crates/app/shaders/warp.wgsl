// Modifier: warp. The picture is pulled through a slowly flowing field of
// smooth noise: liquid, heat-haze, or a slow churn depending on scale and
// amount. The bass swells the displacement; onsets kick the flow.
//   p0 = (amount px, scale cycles per screen, flow time, twist)
//   p1 = (octaves, 0, 0, 0)

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;          // mirror-repeat
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

// Integer lattice hash. A float hash of `i + 1.0` can round differently
// from the neighbour's `i`, which tears the field along cell edges; integers
// cannot.
fn ihash(p: vec2<i32>) -> f32 {
    var h = u32(p.x) * 374761393u + u32(p.y) * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    h = h ^ (h >> 16u);
    return f32(h & 0x00ffffffu) / 16777216.0;
}

// Smooth value noise, 0..1.
fn vnoise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = ihash(i);
    let b = ihash(i + vec2<i32>(1, 0));
    let c = ihash(i + vec2<i32>(0, 1));
    let d = ihash(i + vec2<i32>(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>, octaves: i32) -> f32 {
    var v = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < octaves; i++) {
        v += amp * vnoise(q);
        q = q * 2.03 + vec2<f32>(17.1, 9.7);
        amp *= 0.5;
    }
    return v;
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let aspect = M.resolution.x / M.resolution.y;
    let p = vec2<f32>(in.uv.x * aspect, in.uv.y) * M.p0.y;
    let t = M.p0.z;
    let oct = i32(clamp(M.p1.x, 1.0, 5.0));
    // Two independent fields give a vector; a third turns it for the twist.
    let nx = fbm(p + vec2<f32>(t * 0.7, 0.0), oct) - 0.5;
    let ny = fbm(p + vec2<f32>(31.4, t * 0.55), oct) - 0.5;
    let ang = (fbm(p * 0.5 + vec2<f32>(t * 0.2, 71.0), oct) - 0.5) * 6.2831853 * M.p0.w;
    let c = cos(ang);
    let s = sin(ang);
    let v = vec2<f32>(c * nx - s * ny, s * nx + c * ny);
    let off = v * 2.0 * M.p0.x / M.resolution;
    let col = textureSample(src, samp, in.uv + off).rgb;
    return vec4<f32>(col, 1.0);
}
