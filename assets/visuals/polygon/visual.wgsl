// Polygon: one glowing regular polygon. Example 1 of docs/building-visuals.md:
// a shape from a distance function, an antialiased line, a glow that ends at
// zero, and colour from the layer's palette.

const PI: f32 = 3.14159265;

// Signed distance to a regular n-gon with circumradius r, a vertex at the
// top (negative inside). After Inigo Quilez's sdRegularPolygon (MIT).
fn polygon(p: vec2<f32>, r: f32, n: f32) -> f32 {
    let an = PI / n;
    let a = atan2(p.x, p.y);
    // The angle within one sector, -an..an.
    let b = a - 2.0 * an * floor(a / (2.0 * an)) - an;
    var q = length(p) * vec2<f32>(cos(b), abs(sin(b)));
    q -= r * vec2<f32>(cos(an), sin(an));
    q.y += clamp(-q.y, 0.0, r * sin(an));
    return length(q) * sign(q.x);
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    // Turning the plane one way turns the shape the other.
    let p = rotate(centred(uv), -V.time * P.spin * TAU);
    let d = abs(polygon(p, P.radius, P.sides));

    // The line: full inside half its width, fading over one pixel.
    let half = 0.5 * P.width * pixel();
    let line = clamp((half - d) / pixel() + 0.5, 0.0, 1.0);
    // The glow: an exponential tail, lowered so it reaches exactly zero
    // (a tail that never ends counts as lit everywhere for the levelling).
    let glow = P.glow * max(exp(-d / P.reach) - 0.002, 0.0);

    // Colour runs once round the outline, so a palette wraps the shape.
    let t = atan2(p.x, p.y) / TAU + 0.5;
    return colour(t) * (2.0 * line + glow);
}
