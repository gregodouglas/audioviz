// Kaleidoscope fold: a per-layer transform applied to a visual's output
// before it joins the mix. Radial = two mirrors (N-fold symmetry around the
// centre); tiled = three mirrors at 60 degrees (the whole frame becomes a
// triangular tiling of one cell). Four rotated-grid taps soften the seams.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;          // mirror-repeat
@group(0) @binding(2) var<uniform> F: FoldParams;

const PI: f32 = 3.14159265;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn rot(p: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
}

fn fold_radial(p: vec2<f32>) -> vec2<f32> {
    let r = length(p);
    var a = atan2(p.y, p.x) + F.angle;
    let seg = 2.0 * PI / max(F.segments, 1.0);
    a = a - seg * floor(a / seg);
    if (a > seg * 0.5) { a = seg - a; }
    return vec2<f32>(cos(a), sin(a)) * r;
}

fn fold_tiled(p: vec2<f32>) -> vec2<f32> {
    var q = rot(p, F.angle);
    let h = F.size;
    // Outward normals of an equilateral triangle centred on the origin.
    let n0 = vec2<f32>(0.0, 1.0);
    let n1 = vec2<f32>(-0.8660254, -0.5);
    let n2 = vec2<f32>(0.8660254, -0.5);
    for (var i = 0; i < 10; i++) {
        var d = dot(q, n0) - h;
        if (d > 0.0) { q -= 2.0 * d * n0; }
        d = dot(q, n1) - h;
        if (d > 0.0) { q -= 2.0 * d * n1; }
        d = dot(q, n2) - h;
        if (d > 0.0) { q -= 2.0 * d * n2; }
    }
    return q;
}

fn fold(p: vec2<f32>) -> vec2<f32> {
    if (F.mode == 1u) { return fold_tiled(p); }
    return fold_radial(p);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let res = F.resolution;
    let aspect = res.x / res.y;
    var col = vec3<f32>(0.0);
    // Rotated-grid supersample: four taps a quarter pixel apart.
    let offs = array<vec2<f32>, 4>(vec2<f32>(0.125, 0.375), vec2<f32>(-0.375, 0.125), vec2<f32>(-0.125, -0.375), vec2<f32>(0.375, -0.125));
    for (var i = 0; i < 4; i++) {
        let uv = in.uv + offs[i] / res;
        let s = (uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0);
        let q = fold(s) / max(F.zoom, 0.05);
        let suv = (q * vec2<f32>(1.0 / aspect, -1.0) + 1.0) * 0.5;
        col += textureSampleLevel(src, samp, suv, 0.0).rgb;
    }
    return vec4<f32>(col * 0.25, 1.0);
}
