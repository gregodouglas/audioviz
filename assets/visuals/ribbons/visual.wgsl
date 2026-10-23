// Flow-field ribbons: soft strokes of light carried by a flow the music
// stirs, into a trail that fades by Persistence. Path picks the variant:
//   free/  particles advected through a curl-noise field plus twelve
//          vortices, one per pitch class on a circle-of-fifths wheel, each
//          as strong as its note (flow.wgsl).
//   fall/  long strands bound to a path in 3D: a column dropping from
//          above, bending round an arc and fanning out along a floor toward
//          the camera, the curl noise their meander (fall.wgsl; after
//          vizz.fm's Light Fall).
// This file holds what both share: the noise, the colours and the trail.
// Coordinates: y in [-1, 1], x in [-aspect, aspect] (the height is the
// unit, as in the compiled-in Ribbons).

fn aspect() -> f32 {
    return V.resolution.x / V.resolution.y;
}

fn seed() -> f32 {
    return V.instance * 100.0;
}

// ---------------------------------------------------------------- the noise

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn vnoise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash31(i + vec3<f32>(0.0, 0.0, 0.0)), hash31(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let b = mix(hash31(i + vec3<f32>(0.0, 1.0, 0.0)), hash31(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let c = mix(hash31(i + vec3<f32>(0.0, 0.0, 1.0)), hash31(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let d = mix(hash31(i + vec3<f32>(0.0, 1.0, 1.0)), hash31(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z) * 2.0 - 1.0;
}

fn potential(p: vec2<f32>, t: f32) -> f32 {
    var s = 0.0;
    s += vnoise3(vec3<f32>(p, t)) * 1.0;
    s += vnoise3(vec3<f32>(p * 2.1 + 17.0, t * 1.3)) * 0.5;
    s += vnoise3(vec3<f32>(p * 4.3 + 31.0, t * 1.7)) * 0.25;
    return s;
}

// ---------------------------------------------------------------- colour

fn hsv2rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let k = vec3<f32>(0.0, 2.0, 4.0) + h * 6.0;
    let f = abs(fract(k / 6.0) * 6.0 - 3.0) - 1.0;
    return v * mix(vec3<f32>(1.0), clamp(f, vec3<f32>(0.0), vec3<f32>(1.0)), s);
}

fn palette_at(t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
}

// A stroke's colour: its pitch class's hue on the circle of fifths pulled
// toward the key's. With a palette, the chord sits round the palette's
// moving position, each class offset by its distance from the key times
// the spread; `sec` is the section's offset, `jitter` a stroke's own
// (-0.5..0.5), `along` a further move along the palette.
fn stroke_colour(pitch: f32, sec: f32, jitter: f32, along: f32) -> vec3<f32> {
    let P = params();
    let key_hue = f32((u32(V.key) * 7u) % 12u) / 12.0;
    let class_hue = f32((u32(pitch) * 7u) % 12u) / 12.0;
    let hue = fract(mix(key_hue, class_hue, 0.6) + sec + along);
    let own = mix(V.tint.rgb, hsv2rgb(hue, P.saturation, 1.0), 0.85);
    var off = class_hue - key_hue;
    off = off - round(off);
    let pal = palette_at(V.palette_shift + sec + along + off * P.spread + jitter * 0.04 * P.spread) * 1.3;
    return mix(own, pal, V.palette_mix);
}

// ---------------------------------------------------------------- the trail

// Last frame fading by Persistence per second, with a tiny floor taken off
// so it reaches black.
fn shade(uv: vec2<f32>) -> vec3<f32> {
    let decay = pow(max(params().persistence, 1e-6), V.dt);
    return max(prev(uv) * decay - vec3<f32>(0.0005), vec3<f32>(0.0));
}
