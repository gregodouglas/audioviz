// A starter visual, made by New visual. Save this file and it reloads.
// The guide: docs/building-visuals.md ("The music" and "Choosing a
// signal"). The API: docs/visuals.md.
//
// shade() runs once per pixel and returns linear light (above 1 is fine).
// params() holds the controls from visual.json by their ids; V and the
// music functions hold the music: V.swell breathes with the bass,
// broad_attack(BASS) is the kick, bar_phase() runs through each bar,
// level_at(t) is the spectrum in balance; colour(t) is the layer's palette.

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let p = centred(uv);                       // origin in the middle, y up, short side -1..1
    let r = P.radius * (1.0 + 0.2 * P.swell * V.swell + 0.05 * P.swell * broad_attack(BASS));
    // A slow sway, once a bar, only as much as the beat is sure.
    let sway = 0.15 * P.sway * sin(TAU * bar_phase()) * V.confidence;
    let q = rotate(p, -sway);
    let d = abs(length(q) - r);                // distance to the ring
    let half = 0.5 * P.width * pixel();
    let line = clamp((half - d) / pixel() + 0.5, 0.0, 1.0);
    let glow = 0.4 * max(exp(-d / 0.05) - 0.002, 0.0);
    let t = atan2(q.x, q.y) / TAU + 0.5;       // 0..1 round the ring
    return colour(t) * (2.0 * line + glow) * (0.3 + level_at(abs(2.0 * t - 1.0)));
}
