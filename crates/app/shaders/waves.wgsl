// Waves: a landscape of waveform (or spectrum) rows in perspective. Rows are
// drawn far to near; each row first fills black from its curve down to its
// baseline (hiding what is behind it), then draws its line. One instanced
// draw: instance = row * (2 * segments) + part * segments + segment.
// Premultiplied alpha: the fill covers, the line is anti-aliased.

@group(0) @binding(0) var<uniform> W: WavesParams;
@group(0) @binding(1) var<storage, read> rows: array<f32>;
@group(0) @binding(2) var<storage, read> levels: array<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,        // (along, across) in pixels, line part only
    @location(1) seg_len: f32,
    @location(2) @interpolate(flat) part: u32,
    @location(3) colour: vec3<f32>,
};

fn to_ndc(px: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(px.x / W.resolution.x * 2.0 - 1.0, 1.0 - px.y / W.resolution.y * 2.0, 0.0, 1.0);
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let segs = W.cols - 1u;
    let per_row = segs * 2u;
    let row = ii / per_row;                 // 0 = farthest, drawn first
    let within = ii % per_row;
    let part = within / segs;               // 0 fill, 1 line
    let seg = within % segs;
    let age = W.rows - 1u - row;            // 0 = newest
    let t = f32(age) / f32(max(W.rows - 1u, 1u));
    // Foreshortening: rows bunch toward the horizon as perspective rises.
    let k = max(1.0 - W.persp, 0.03);
    let tp = t * (1.0 + k) / (t + k);
    let far_scale = mix(1.0, 0.22, W.persp);
    let scale = mix(1.0, far_scale, tp);
    let base_y = mix(W.near_y, W.far_y, tp) * 2.0 - 1.0;
    let data = ((W.head + W.rows - age) % W.rows) * W.cols;

    let i0 = seg;
    let i1 = seg + 1u;
    let x0 = (-1.0 + 2.0 * f32(i0) / f32(segs)) * W.spread * scale;
    let x1 = (-1.0 + 2.0 * f32(i1) / f32(segs)) * W.spread * scale;
    let v0 = rows[data + i0];
    let v1 = rows[data + i1];
    let amp = W.height * scale;
    let a = vec2<f32>(x0, base_y + v0 * amp);
    let b = vec2<f32>(x1, base_y + v1 * amp);
    let a_base = vec2<f32>(x0, base_y);
    let b_base = vec2<f32>(x1, base_y);

    var out: VsOut;
    out.part = part;
    out.seg_len = 0.0;
    out.local = vec2<f32>(0.0);
    // Colour: through the palette by age (far = later colours) or by the
    // row's loudness; without a palette, the tint.
    let lvl = levels[(W.head + W.rows - age) % W.rows];
    var pos_t = t;
    if (W.colour == 1u) { pos_t = clamp(lvl, 0.0, 1.0); }
    let pal = waves_pal(W, pos_t + W.palette_shift) * 1.2;
    let bright = mix(1.0, 1.0 - W.fade, tp);
    out.colour = mix(W.tint.rgb, pal, W.palette_mix) * bright;

    if (part == 0u) {
        var p: vec2<f32>;
        switch (vi) {
            case 0u: { p = a; }
            case 1u: { p = b; }
            case 2u: { p = a_base; }
            case 3u: { p = a_base; }
            case 4u: { p = b; }
            default: { p = b_base; }
        }
        out.pos = vec4<f32>(p, 0.0, 1.0);
        return out;
    }

    // Line quad in pixels, like the scope beam.
    let pa = vec2<f32>((a.x + 1.0) * 0.5 * W.resolution.x, (1.0 - a.y) * 0.5 * W.resolution.y);
    let pb = vec2<f32>((b.x + 1.0) * 0.5 * W.resolution.x, (1.0 - b.y) * 0.5 * W.resolution.y);
    var d = pb - pa;
    let len = length(d);
    if (len < 1e-4) { d = vec2<f32>(1.0, 0.0); } else { d = d / len; }
    let n = vec2<f32>(-d.y, d.x);
    let w = W.width * 0.5 + 1.5;
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
    out.pos = to_ndc(pa + d * along + n * across);
    out.local = vec2<f32>(along, across);
    out.seg_len = len;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    if (in.part == 0u) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let along = clamp(in.local.x, 0.0, in.seg_len);
    let dist = length(vec2<f32>(in.local.x - along, in.local.y));
    let half = max(W.width, 0.5) * 0.5;
    let cov = 1.0 - smoothstep(half - 0.5, half + 0.8, dist);
    return vec4<f32>(in.colour * cov, cov);
}
