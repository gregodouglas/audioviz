// Modifier: kaleidoscope fold. Radial = two mirrors (N-fold symmetry around
// an apex); pinwheel = rotation only (no mirror, spinning blades); tiled =
// three mirrors at 60 degrees (a triangular tiling of one cell). The apex can
// sit off the source's centre (p1.yz), which is what makes a real
// kaleidoscope's patterns rich. Four rotated-grid taps soften the seams.
// Zoomed out, `src` is a mip pyramid of the input and the taps read at the
// zoom's lod, which keeps them in cache and stops the shimmer.
//   p0 = (segments, angle, zoom, mode)   p1 = (cell size, offset x, offset y, lod)

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;          // mirror-repeat
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var unused_tex: texture_2d<f32>;

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

fn fold_radial(p: vec2<f32>, mirror: bool) -> vec2<f32> {
    let r = length(p);
    var a = atan2(p.y, p.x) + M.p0.y;
    let seg = 2.0 * PI / max(M.p0.x, 1.0);
    a = a - seg * floor(a / seg);
    if (mirror && a > seg * 0.5) { a = seg - a; }
    return vec2<f32>(cos(a), sin(a)) * r;
}

fn fold_tiled(p: vec2<f32>) -> vec2<f32> {
    var q = rot(p, M.p0.y);
    let h = M.p1.x;
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
    let mode = u32(M.p0.w + 0.5);
    if (mode == 1u) { return fold_tiled(p); }
    if (mode == 2u) { return fold_radial(p, false); }
    return fold_radial(p, true);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let res = M.resolution;
    let aspect = res.x / res.y;
    var col = vec3<f32>(0.0);
    let offs = array<vec2<f32>, 4>(vec2<f32>(0.125, 0.375), vec2<f32>(-0.375, 0.125), vec2<f32>(-0.125, -0.375), vec2<f32>(0.375, -0.125));
    for (var i = 0; i < 4; i++) {
        let uv = in.uv + offs[i] / res;
        let s = (uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0);
        let q = fold(s) / max(M.p0.z, 0.05) + M.p1.yz;
        let suv = (q * vec2<f32>(1.0 / aspect, -1.0) + 1.0) * 0.5;
        col += textureSampleLevel(src, samp, suv, M.p1.w).rgb;
    }
    return vec4<f32>(col * 0.25, 1.0);
}
