// Draw each particle's step (prev -> pos) as an anti-aliased segment, colored
// by pitch class around the current key, into the persistence buffer.

@group(0) @binding(0) var<uniform> P: RibbonParams;
@group(0) @binding(1) var<storage, read> particles: array<Particle>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) seg_len: f32,
    @location(2) color: vec3<f32>,
};

fn to_px(p: vec2<f32>) -> vec2<f32> {
    let half = P.resolution.y * 0.5;
    return P.resolution * 0.5 + vec2<f32>(p.x, -p.y) * half;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let part = particles[ii];
    let a = to_px(part.prev);
    let b = to_px(part.pos);
    var d = b - a;
    var len = length(d);
    // A respawn jump or a brand-new particle: draw nothing.
    if (len < 1e-4 || len > 60.0 || part.age < 1e-4) {
        len = 0.0;
        d = vec2<f32>(1.0, 0.0);
    } else {
        d = d / len;
    }
    let n = vec2<f32>(-d.y, d.x);
    let w = P.beam_width * 2.5 + 1.0;

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
    var px = a + d * along + n * across;
    if (len == 0.0) { px = a; }
    let ndc = vec2<f32>(px.x / P.resolution.x * 2.0 - 1.0, 1.0 - px.y / P.resolution.y * 2.0);

    // Color: pitch-class hue (circle of fifths) pulled toward the key hue;
    // fade in/out over the particle's life.
    let class_hue = f32((u32(part.pitch) * 7u) % 12u) / 12.0;
    let hue = fract(mix(P.key_hue, class_hue, 0.6));
    let fade = smoothstep(0.0, 0.6, part.age) * (1.0 - smoothstep(part.life - 1.0, part.life, part.age));
    let col = mix(P.tint.rgb, hsv2rgb(hue, P.saturation, 1.0), 0.85) * fade;

    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.local = vec2<f32>(along, across);
    out.seg_len = len;
    out.color = col;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let along = clamp(in.local.x, 0.0, in.seg_len);
    let dx = in.local.x - along;
    let dist = length(vec2<f32>(dx, in.local.y));
    let sigma = max(P.beam_width, 0.3);
    let falloff = exp(-(dist * dist) / (2.0 * sigma * sigma));
    return vec4<f32>(in.color * falloff * P.beam_intensity, 1.0);
}
