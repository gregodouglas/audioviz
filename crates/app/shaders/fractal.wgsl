// Modifier: fractal. Two recursions over the picture.
//   kifs  = kaleidoscopic IFS: fold the plane (abs), rotate, scale and shift,
//           sampling the picture at every step so one cell recurs at every scale.
//   julia = the plane is iterated through z^2 + c and the picture is read at
//           each z: the source gets pulled into Julia-set filaments.
//   p0 = (mode, iterations, scale, angle)   p1 = (offset, zoom, c.x, c.y)   p2 = (falloff, 0, 0, 0)

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;          // mirror-repeat
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn rot(p: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
}

fn read(q: vec2<f32>, aspect: f32) -> vec3<f32> {
    let suv = (q * vec2<f32>(1.0 / aspect, -1.0) + 1.0) * 0.5;
    return textureSampleLevel(src, samp, suv, 0.0).rgb;
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let aspect = M.resolution.x / M.resolution.y;
    let s = (in.uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0) / max(M.p1.y, 0.05);
    let iters = i32(clamp(M.p0.y, 1.0, 10.0));
    let mode = u32(M.p0.x + 0.5);
    var col = read(s, aspect);
    var wsum = 1.0;
    var w = 1.0;
    var p = s;
    if (mode == 1u) {
        let c = M.p1.zw;
        for (var i = 0; i < iters; i++) {
            p = vec2<f32>(p.x * p.x - p.y * p.y, 2.0 * p.x * p.y) + c;
            // Keep the sample inside a sane range; the set's escape is the look.
            let r = length(p);
            let q = select(p, p / r * (1.0 + log(r)), r > 1.0);
            w *= M.p2.x;
            col += w * read(q, aspect);
            wsum += w;
        }
    } else {
        for (var i = 0; i < iters; i++) {
            p = abs(p);
            p = rot(p, M.p0.w);
            p = p * M.p0.z - vec2<f32>(M.p1.x, M.p1.x * 0.6);
            w *= M.p2.x;
            col += w * read(p, aspect);
            wsum += w;
        }
    }
    return vec4<f32>(col / wsum, 1.0);
}
