// Luminous: how translucent, light-giving living things look. A visual
// opts in with "uses": ["luminous"] (the "jelly" library brings it too).
// See docs/building-visuals.md, "Creatures".
//
// The model. A jellyfish, a comb jelly or a glowing plankton does not
// reflect much light; what the eye sees is light from inside it: its own
// (bioluminescence, or the fluorescence that makes aquarium photographs
// glow) and light scattered in its jelly. So everything here is emission:
//   glass      more light where the eye looks through more jelly (the
//              thickness over how face-on the skin is): a soft body that
//              brightens toward its silhouette by itself
//   edge       the thin skin seen edge on (Fresnel)
//   key light  light from the surface above: the top of a body a little
//              brighter (its shape), a glow when the eye looks up through
//              it toward the light (forward scattering), and the glint of
//              that light on its wet skin (what makes it read as wet)
//   patterns   organs, canals, stripes, photophores: emission painted over
//              the surface with the textures below, faded where they would
//              be finer than a pixel
//   water      far things dim and lose their reds first
// Textures are procedural (resolution-free, sharp in a close-up, no files).

// ---------------------------------------------------------------- glass and light

// Edge glow: 1 seen edge on, 0 face on. `facing_amount` is how much the
// surface faces the eye (1 face on, 0 edge on).
fn fresnel(facing_amount: f32, power: f32) -> f32 {
    return pow(1.0 - clamp(facing_amount, 0.0, 1.0), power);
}

// How much jelly the eye looks through at a skin `thickness` thick (in the
// creature's own units) seen `facing_amount` face on: 1 for a unit of jelly
// face on, more toward the silhouette, capped where a ray would graze it.
fn glass(facing_amount: f32, thickness: f32) -> f32 {
    return min((thickness + 0.03) / max(facing_amount, 0.15), 2.5);
}

// The light from the surface above, for a body seen from `view` (unit
// direction from the eye to the point) with world normal `n`: x is how much
// the surface faces the light (0..1, wrapped, so the underside is not black),
// y the glow when the eye looks up through a thin body toward the light.
fn key_light(n: vec3<f32>, view: vec3<f32>) -> vec2<f32> {
    let l = normalize(vec3<f32>(0.25, 1.0, 0.15));
    let top = 0.5 + 0.5 * dot(n, l);
    let through = pow(max(dot(view, l), 0.0), 3.0);
    return vec2<f32>(top * top, through);
}

// The glint of the light above on wet skin: a soft highlight where the
// skin mirrors the light toward the eye, either side of the skin. 0..1.
fn glint(n: vec3<f32>, view: vec3<f32>) -> f32 {
    let l = normalize(vec3<f32>(0.25, 1.0, 0.15));
    let m = n * select(1.0, -1.0, dot(n, view) > 0.0);
    let r = reflect(view, m);
    let d = max(dot(r, l), 0.0);
    return pow(d, 28.0) + 0.15 * pow(d, 5.0);
}

// The light above as a jelly's skin shows it, for a pixel facing the eye
// `facing_amount`: the wet glint, and the glow of thin skin when the eye
// looks up through it (key_light's y).
fn sheen(n: vec3<f32>, view: vec3<f32>, facing_amount: f32) -> f32 {
    return glint(n, view) + key_light(n, view).y * fresnel(facing_amount, 1.5) * 0.6;
}

// Water between the eye and a thing `dist` beyond `near`: dimmer with
// distance, the reds going first (what makes the deep blue). `natural` 0
// dims only (for palette colours), 1 shifts the colour too.
fn water(c: vec3<f32>, dist: f32, near: f32, natural: f32) -> vec3<f32> {
    let d = max(dist - near, 0.0);
    let grey = exp(-d * 0.3);
    let blue = exp(-d * vec3<f32>(0.55, 0.28, 0.2));
    return c * mix(vec3<f32>(grey), blue, natural);
}

// A soft glow round a light at distance d, r its size: 1 at the light,
// 1/2 at r, falling off like light in murky water.
fn halo(d: f32, r: f32) -> f32 {
    let q = d / max(r, 1e-4);
    return 1.0 / (1.0 + q * q * (1.0 + 0.5 * q));
}

// A rainbow: hue t (0..1, wraps) as a saturated colour, for iridescence.
fn spectrum(t: f32) -> vec3<f32> {
    return clamp(abs(fract(t + vec3<f32>(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, vec3<f32>(0.0), vec3<f32>(1.0));
}

// Thin-film colour: a skin `thickness` thick seen `facing_amount` face on
// shows interference colours that slide as the angle changes (a soap
// bubble; a comb jelly's cilia diffract light in the same rainbow).
fn thin_film(facing_amount: f32, thickness: f32) -> vec3<f32> {
    let path = thickness / max(facing_amount, 0.25);
    return 0.5 + 0.5 * cos(TAU * (path + vec3<f32>(0.0, 0.33, 0.67)));
}

// ---------------------------------------------------------------- lines and points

// A line of half-width w at distance d, widened to at least a pixel (fw)
// and dimmed to match, so fine lines fade instead of shimmering.
fn thin_line(d: f32, w: f32, fw: f32) -> f32 {
    let ww = max(w, fw);
    return smoothstep(ww, 0.0, d) * (w / ww);
}

// Photophores: light organs scattered over a surface, one per cell of a grid
// `scale` across coordinates p, each lit with a chance `share`, twinkling on
// its own rhythm (`t`, seconds); `fw` is one pixel in p. 0..1.
fn photophores(p: vec2<f32>, scale: f32, share: f32, t: f32, fw: f32) -> f32 {
    let q = p * scale;
    let cell = floor(q);
    let h = hash22(cell + 17.0);
    if (h.x > share) {
        return 0.0;
    }
    let at = cell + 0.25 + 0.5 * hash22(cell + 41.0);
    let d = length(q - at);
    let r = 0.12 * (0.6 + 0.8 * h.y);
    let twinkle = 0.55 + 0.45 * sin(t * (0.7 + 1.6 * h.y) + h.x * 60.0);
    return thin_line(d, r, fw * scale) * twinkle;
}

// ---------------------------------------------------------------- textures
//
// Coordinates are whatever the caller maps across the surface.

// Fractal noise: `octaves` (1..6) layers of value noise, each twice as fine
// and half as strong; 0..1, about 0.5 on average.
fn fbm(p: vec2<f32>, octaves: u32) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    var norm = 0.0;
    for (var o = 0u; o < min(octaves, 6u); o++) {
        sum += noise(q) * amp;
        norm += amp;
        q = q * 2.03 + vec2<f32>(17.1, 9.3);
        amp *= 0.5;
    }
    return sum / norm;
}

// Cells (Voronoi): x the distance to the nearest cell centre, y to the
// second nearest, z a 0..1 id of the nearest cell. y - x is small along the
// walls between cells: tissue, scales, foam. `jitter` 0 is a grid, 1 fully
// irregular; `t` moves the centres slowly.
fn cells(p: vec2<f32>, jitter: f32, t: f32) -> vec3<f32> {
    let base = floor(p);
    let f = p - base;
    var d1 = 8.0;
    var d2 = 8.0;
    var id = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let o = vec2<f32>(f32(x), f32(y));
            let h = hash22(base + o);
            let c = o + 0.5 + (h - 0.5) * jitter + 0.15 * jitter * sin(t + h * 6.2831);
            let d = length(f - c);
            if (d < d1) {
                d2 = d1;
                d1 = d;
                id = h.x;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    return vec3<f32>(d1, d2, id);
}

// Fine texture that would only shimmer at this size fades to its average:
// `detail` is the texture's frequency times one pixel in its coordinates
// (fwidth); 1 at and below about a cycle per two pixels, 0 when finer.
fn keep_detail(detail: f32) -> f32 {
    return 1.0 - smoothstep(0.25, 0.6, detail);
}
