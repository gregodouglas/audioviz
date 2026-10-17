// Shared prelude, prepended to every shader by shaders.rs.

struct FsIn {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Fullscreen triangle.
fn fullscreen_vertex(vi: u32) -> FsIn {
    var out: FsIn;
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}

// Interleaved gradient noise (Jimenez 2014): cheap, well distributed.
fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(0.06711056 * p.x + 0.00583715 * p.y));
}

// Hash helpers.
fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q += dot(q, q + 45.32);
    return fract(q.x * q.y);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
    let h = hash21(p);
    return vec2<f32>(h, hash21(p + h + 7.13));
}

// Post / tonemap params (post.rs::PostParams).
struct PostParams {
    exposure: f32,
    bloom: f32,
    grain: f32,
    time: f32,
    dt: f32,
    flags: u32,       // bit0 (1): auto exposure, bit1 (2): reduce flashing
    flash_rise: f32,  // reduce flashing: max brightness rise factor per second is 1 + flash_rise
    _pad0: u32,
};

// Layer compositor / meter params (director.rs::LayerParams).
struct LayerParams {
    opacity: f32,
    dt: f32,
    flags: u32,       // bit0 (1): apply the layer's levelling gain
    _pad0: f32,
};

// Kaleidoscope fold params (director.rs::FoldParams, 32 bytes).
struct FoldParams {
    resolution: vec2<f32>,
    segments: f32,
    angle: f32,
    zoom: f32,
    mode: u32,        // 0 radial (two mirrors), 1 tiled (three mirrors)
    size: f32,        // tiled: triangle inradius in image units
    _pad0: f32,
};
