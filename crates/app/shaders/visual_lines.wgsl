// Lines for loaded visuals (shader_visual.rs). A visual's point(i) wrote two
// vec4 per point (centred x, centred y, width px, depth) and (colour, 0); this
// draws instance i as the segment from point i to i + 1: a quad around it
// with a soft edge and round caps, added over the visual's picture. A point
// with width 0 breaks the line there. Caps are half bright so joins do not
// double up. In a shared space, fs_space also writes the line's light depth
// (the point's view depth, 0 meaning far) into the space's light target.

struct LineIn {
    resolution: vec2<f32>,
    count: u32,
    style: u32,   // 0 line, 1 beam (width is a Gaussian sigma, brightness falls with segment length)
};

@group(0) @binding(0) var<uniform> L: LineIn;
@group(0) @binding(1) var<storage, read> pts: array<vec4<f32>>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) seg_len: f32,
    @location(2) half_w: f32,
    @location(3) colour: vec3<f32>,
    @location(4) depth: f32,
};

fn to_px(p: vec2<f32>) -> vec2<f32> {
    let half = 0.5 * min(L.resolution.x, L.resolution.y);
    return L.resolution * 0.5 + vec2<f32>(p.x, -p.y) * half;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(0.0);
    out.local = vec2<f32>(0.0);
    out.seg_len = 0.0;
    out.half_w = 0.0;
    out.colour = vec3<f32>(0.0);
    out.depth = 0.0;
    let a = pts[2u * ii];
    let b = pts[2u * ii + 2u];
    // A zero width ends the stroke: a degenerate quad draws nothing.
    if (ii + 1u >= L.count || !(a.z > 0.0) || !(b.z > 0.0)) {
        return out;
    }
    let pa = to_px(a.xy);
    let pb = to_px(b.xy);
    var d = pb - pa;
    let len = length(d);
    if (len < 1e-4) { d = vec2<f32>(1.0, 0.0); } else { d = d / len; }
    let n = vec2<f32>(-d.y, d.x);
    let half_w = 0.25 * (a.z + b.z);
    var w = half_w + 1.5;
    if (L.style == 1u) {
        w = 2.0 * half_w * 2.5 + 1.0;
    }
    var along: f32;
    var across: f32;
    switch (vi) {
        case 0u: { along = -w;       across = -w; }
        case 1u: { along = len + w;  across = -w; }
        case 2u: { along = -w;       across =  w; }
        case 3u: { along = -w;       across =  w; }
        case 4u: { along = len + w;  across = -w; }
        default: { along = len + w;  across =  w; }
    }
    let px = pa + d * along + n * across;
    out.pos = vec4<f32>(px.x / L.resolution.x * 2.0 - 1.0, 1.0 - px.y / L.resolution.y * 2.0, 0.0, 1.0);
    out.local = vec2<f32>(along, across);
    out.seg_len = len;
    out.half_w = half_w;
    out.colour = select(pts[2u * ii + 3u].rgb, pts[2u * ii + 1u].rgb, along < len * 0.5);
    out.depth = select(b.w, a.w, along < len * 0.5);
    return out;
}

fn line_light(in: VsOut) -> vec3<f32> {
    let along = clamp(in.local.x, 0.0, in.seg_len);
    let dist = length(vec2<f32>(in.local.x - along, in.local.y));
    var c: vec3<f32>;
    if (L.style == 1u) {
        // The beam: a Gaussian cross-section, dimmer the faster it moved
        // (the time it spent on each pixel).
        let sigma = max(2.0 * in.half_w, 0.3);
        c = in.colour * exp(-(dist * dist) / (2.0 * sigma * sigma)) / (in.seg_len + 1.0);
    } else {
        let half = max(in.half_w, 0.5);
        let cov = 1.0 - smoothstep(half - 0.5, half + 0.8, dist);
        let cap = select(0.5, 1.0, in.local.x >= 0.0 && in.local.x <= in.seg_len);
        c = in.colour * cov * cap;
    }
    // A NaN from a visual's point() must not reach the levelling.
    return select(vec3<f32>(0.0), max(c, vec3<f32>(0.0)), all(c == c));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(line_light(in), 1.0);
}

// The far plane of the light depth (LIGHT_FAR in the visual API).
const LIGHT_FAR: f32 = 50.0;

struct SpaceOut {
    @location(0) colour: vec4<f32>,
    @location(1) light: vec4<f32>,
};

@fragment
fn fs_space(in: VsOut) -> SpaceOut {
    let c = line_light(in);
    var o: SpaceOut;
    o.colour = vec4<f32>(c, 1.0);
    // A pixel the line does not light keeps the far plane.
    let lit = dot(c, vec3<f32>(1.0)) > 0.0 && in.depth > 0.0;
    o.light = vec4<f32>(0.0, 0.0, 0.0, select(LIGHT_FAR, in.depth, lit));
    return o;
}
