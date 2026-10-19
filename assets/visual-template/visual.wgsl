// A starter visual, made by New visual. Save this file and it reloads.
// The guide: docs/building-visuals.md. The API: docs/visuals.md.
//
// shade() runs once per pixel and returns linear light (above 1 is fine).
// params() holds the controls from visual.json by their ids; V holds the
// music (V.bass, V.beat, V.flow...); band_at(t) is the spectrum, colour(t)
// the layer's palette.

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let p = centred(uv);                       // origin in the middle, y up, short side -1..1
    let r = P.radius * (1.0 + 0.2 * P.swell * V.swell);
    let d = abs(length(p) - r);                // distance to the ring
    let half = 0.5 * P.width * pixel();
    let line = clamp((half - d) / pixel() + 0.5, 0.0, 1.0);
    let glow = 0.4 * max(exp(-d / 0.05) - 0.002, 0.0);
    let t = atan2(p.x, p.y) / TAU + 0.5;       // 0..1 round the ring
    return colour(t) * (2.0 * line + glow) * (0.5 + band_at(t));
}
