// Loop: concentric waveform rings, one instanced draw of line segments.
// instance = ring * cols + segment; ring 0 is the newest (innermost). Each
// segment is a pixel-space quad round the line (as the scope beam), drawn
// additively with an anti-aliased core.

@group(0) @binding(0) var<uniform> L: LoopParams;
@group(0) @binding(1) var<storage, read> rings: array<f32>;
@group(0) @binding(2) var<storage, read> levels: array<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) seg_len: f32,
    @location(2) colour: vec3<f32>,
};

fn ring_point(data: u32, i: u32, r: f32, turn: f32) -> vec2<f32> {
    let k = i % L.cols;
    let a = 6.2831853 * f32(k) / f32(L.cols) + turn;
    let rr = max(r + rings[data + k] * L.amp, 0.0);
    let half = 0.5 * min(L.resolution.x, L.resolution.y);
    return L.resolution * 0.5 + vec2<f32>(cos(a), sin(a)) * rr * half;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let age = ii / L.cols;                  // 0 = newest
    let seg = ii % L.cols;
    let t = f32(age) / f32(max(L.rings - 1u, 1u));
    let slot = (L.head + L.rings - age) % L.rings;
    let data = slot * L.cols;
    let r = mix(L.inner, L.outer, t);
    let turn = L.spin + L.twist * f32(age);
    let pa = ring_point(data, seg, r, turn);
    let pb = ring_point(data, seg + 1u, r, turn);

    // Brightness: the ring's own loudness, dimmed with age.
    let lvl = clamp(levels[slot], 0.0, 1.0);
    let bright = mix(1.0, 0.15 + 1.6 * lvl, L.level) * mix(1.0, 1.0 - L.fade, t);
    var pos_t = t;
    if (L.colour == 1u) { pos_t = lvl; }
    let pal = loop_pal(L, pos_t + L.palette_shift) * 1.2;

    var out: VsOut;
    out.colour = mix(L.tint.rgb, pal, L.palette_mix) * bright;
    var d = pb - pa;
    let len = length(d);
    if (len < 1e-4) { d = vec2<f32>(1.0, 0.0); } else { d = d / len; }
    let n = vec2<f32>(-d.y, d.x);
    let w = L.width * 0.5 + 1.5;
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
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Segments meet end to end; clamp along so the joins do not double up
    // beyond the half pixel of overlap.
    let along = clamp(in.local.x, 0.0, in.seg_len);
    let dist = length(vec2<f32>(in.local.x - along, in.local.y));
    let half = max(L.width, 0.5) * 0.5;
    let cov = 1.0 - smoothstep(half - 0.5, half + 0.8, dist);
    // Round caps overlap at joints; only the segment's own span counts fully.
    let cap = select(0.5, 1.0, in.local.x >= 0.0 && in.local.x <= in.seg_len);
    return vec4<f32>(in.colour * cov * cap, 1.0);
}
