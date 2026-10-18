// Spiral: one instanced draw, instance i = the segment from point i to i+1.
// Points are stepped round by S.step: an even-density spiral (radius by the
// square root of the index) or a rose r = cos(k theta) joined in step order
// (a Maurer rose). The waveform pushes each point along its radius.

@group(0) @binding(0) var<uniform> S: SpiralParams;
@group(0) @binding(1) var<storage, read> wave: array<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) seg_len: f32,
    @location(2) colour: vec3<f32>,
};

fn point_at(i: u32) -> vec2<f32> {
    let n = f32(max(S.points, 2u));
    let t = f32(i) / (n - 1.0);
    // The step angle is applied in f32 per index; wrap it first so a large
    // index keeps its precision.
    let th = fract(f32(i) * (S.step / 6.2831853)) * 6.2831853;
    var r: f32;
    if (S.shape == 1u) {
        r = cos(S.petals * th);
    } else {
        r = sqrt(t);
    }
    let w = wave[min(u32(t * f32(S.wave - 1u)), S.wave - 1u)];
    r = r * S.radius + w * S.amp;
    let a = th + S.spin;
    let half = 0.5 * min(S.resolution.x, S.resolution.y);
    return S.resolution * 0.5 + vec2<f32>(cos(a), sin(a)) * r * half;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let pa = point_at(ii);
    let pb = point_at(ii + 1u);
    let t = f32(ii) / f32(max(S.points - 1u, 1u));
    let pal = spiral_pal(S, t + S.palette_shift) * 1.2;
    var out: VsOut;
    out.colour = mix(S.tint.rgb, pal, S.palette_mix) * S.intensity;
    var d = pb - pa;
    let len = length(d);
    if (len < 1e-4) { d = vec2<f32>(1.0, 0.0); } else { d = d / len; }
    let n = vec2<f32>(-d.y, d.x);
    let w = S.width * 0.5 + 1.5;
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
    out.pos = vec4<f32>(px.x / S.resolution.x * 2.0 - 1.0, 1.0 - px.y / S.resolution.y * 2.0, 0.0, 1.0);
    out.local = vec2<f32>(along, across);
    out.seg_len = len;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let along = clamp(in.local.x, 0.0, in.seg_len);
    let dist = length(vec2<f32>(in.local.x - along, in.local.y));
    let half = max(S.width, 0.5) * 0.5;
    let cov = 1.0 - smoothstep(half - 0.5, half + 0.8, dist);
    let cap = select(0.5, 1.0, in.local.x >= 0.0 && in.local.x <= in.seg_len);
    return vec4<f32>(in.colour * cov * cap, 1.0);
}
