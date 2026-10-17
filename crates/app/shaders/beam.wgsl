// The electron beam: one instanced quad per consecutive sample pair, drawn
// additively. Brightness per segment ~ 1 / length (time spent per pixel), with
// a Gaussian cross-section.

struct Params {
    resolution: vec2<f32>,
    gain: f32,
    rotation: f32,
    beam_width: f32,
    beam_intensity: f32,
    decay: f32,
    exposure: f32,
    tint: vec4<f32>,
    point_count: u32,
    _pad: vec3<u32>,
};

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read> points: array<vec2<f32>>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,   // (along, across) in pixels
    @location(1) seg_len: f32,       // segment length in pixels
};

// Map an (L, R) sample to pixel coordinates: gain, rotation, square aspect.
fn to_px(s: vec2<f32>) -> vec2<f32> {
    let c = cos(P.rotation);
    let sn = sin(P.rotation);
    let r = vec2<f32>(c * s.x - sn * s.y, sn * s.x + c * s.y) * P.gain;
    let side = min(P.resolution.x, P.resolution.y) * 0.5;
    return P.resolution * 0.5 + vec2<f32>(r.x, -r.y) * side;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let a = to_px(points[ii]);
    let b = to_px(points[ii + 1u]);
    var d = b - a;
    let len = length(d);
    if (len < 1e-4) {
        d = vec2<f32>(1.0, 0.0);
    } else {
        d = d / len;
    }
    let n = vec2<f32>(-d.y, d.x);
    // Half-width plus feather, plus round caps extending past both ends.
    let w = P.beam_width * 2.5 + 1.0;

    // 6 vertices -> 2 triangles covering [-w, len+w] x [-w, w].
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
    let px = a + d * along + n * across;
    let ndc = vec2<f32>(px.x / P.resolution.x * 2.0 - 1.0, 1.0 - px.y / P.resolution.y * 2.0);

    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.local = vec2<f32>(along, across);
    out.seg_len = len;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Distance from the fragment to the segment [0, seg_len] on the along axis.
    let along = clamp(in.local.x, 0.0, in.seg_len);
    let dx = in.local.x - along;
    let dist = length(vec2<f32>(dx, in.local.y));
    let sigma = max(P.beam_width, 0.3);
    let falloff = exp(-(dist * dist) / (2.0 * sigma * sigma));
    // Energy per pixel: one sample interval spread over the segment's length.
    let energy = P.beam_intensity / (in.seg_len + 1.0);
    let v = falloff * energy;
    return vec4<f32>(v, v, v, 1.0);
}
