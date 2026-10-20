// How every part looks: paint, per pixel of the triangles. What a bell, an
// arm and whatever else a variant draws look like is the variant's
// (bell_shade, arm_paint, extra_paint); here is what they share: the pixel's
// light from the skin's frame (Look), its wet relief, the glow in the water,
// and all of it dimmed and blued by the water between it and the eye. In a
// shared space part of each species' own light and of the stroke's light
// diffuses: it spreads softly through the body round where it is, instead
// of all of it sitting sharp on the skin.

// How tight the wet glint on a bell is (glint_at).
const GLINT_SHARP: f32 = 350.0;
// What paint knows about a pixel of a bell.
struct Look {
    s: f32,          // crown 0 to margin 1
    th: f32,         // round the bell, 0..1
    plan: vec2<f32>, // s * (cos, sin) of the angle round: clean to interpolate
    face: f32,       // how face on (1) or edge on (0) the eye sees it
    top: f32,        // how much it faces the light from above
    sheen: f32,      // the light above on its wet skin: glint, and glow through it
    sky: f32,        // the bright water above as the skin mirrors it (sky_reflection)
    inner: bool,     // the subumbrella, under the jelly
    squeeze: f32,    // the stroke here
    fs: f32,         // one pixel in s
    fp: f32,         // one pixel in plan
};

// The part of this pixel's light that diffuses (see diffuse()).
var<private> diffused: vec3<f32>;

// Light c, of which `part` diffuses through the body in a shared space and
// the rest stays sharp (what it returns). `scale` is the control the light
// is under (Bioluminescence, Translucency), which the caller applies to
// what stays sharp. Outside a shared space all of it stays sharp.
fn diffuse(c: vec3<f32>, part: f32, scale: f32) -> vec3<f32> {
    if (!in_space()) {
        return c;
    }
    diffused += c * part * scale;
    return c * (1.0 - part);
}

fn paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    // Derivatives first, while every pixel is still on the same path.
    let s = length(f.colour.zw);
    let fs = fwidth(s);
    let fp = length(fwidth(f.colour.zw));
    // One pixel along colour.y (an arm's, or the lion's mass's, way down).
    let fy = fwidth(f.colour.y);
    let id = f.data.w;
    if (id > PART_EXTRA) {
        return extra_paint(f);
    }
    if (id > PART_GLOW) {
        // Its glow in the water, close round the bell: faint, so it reads as
        // light in the water, not fog.
        let a = item(HALO0 + 2u * (u32(id) % 16u));
        let b = item(HALO0 + 2u * (u32(id) % 16u) + 1u);
        let d = length(f.data.xy);
        let fade = 1.0 - smoothstep(GLOW_REACH * 0.6, GLOW_REACH, d);
        return vec4<f32>(shown(L_GLOW, b.rgb * a.w * halo(d, 0.8) * fade * P.halo * 0.012), 0.0);
    }
    let J = jelly_at(u32(id) % 16u);
    let k = kind();
    var c = vec3<f32>(0.0);
    var cover = 0.0;
    if (id > PART_ARM) {
        let m = arm_paint(J, k, f, fp, fy);
        c = shown(L_ARMS, m.rgb);
        cover = m.a * select(0.0, 1.0, layer_on(L_ARMS));
    } else {
        var L: Look;
        L.s = s;
        L.th = fract(atan2(f.colour.w, f.colour.z + 1e-7) / TAU);
        L.plan = f.colour.zw;
        // The light, per pixel: the skin's normal from the profile and this
        // pixel's own ray, so nothing is smeared across a quad or banded
        // ring by ring. The wet skin is not mirror smooth: a fine relief
        // breaks its glint into a scatter of sparks.
        let ray = pixel_ray(the_camera(), f.pixel);
        let F = skin_frame(J, u32(id) % 16u, s, L.th);
        L.inner = id > PART_INNER;
        var wet = F.n;
        if (!L.inner) {
            wet = wet_normal(F, L.plan, s, fp, J.hue);
        }
        L.face = abs(dot(F.n, ray));
        L.top = key_light(F.n, ray).x;
        L.sky = sky_reflection(wet, ray);
        L.sheen = glint_at(glint_cos(wet, ray), GLINT_SHARP) + through_light(ray) * fresnel(L.face, 1.5) * 0.6;
        L.squeeze = f.colour.y;
        L.fs = fs;
        L.fp = fp;
        // The bell's look is the variant's (bell_shade): light, and how much
        // of what is behind it it covers.
        let b = bell_shade(J, k, L);
        c = b.rgb;
        cover = b.a * select(0.0, 1.0, layer_on(select(L_GLASS, L_INNER, L.inner)));
    }
    c = water(c, f.colour.x, eye_near(), natural_bodies());
    write_glow(water(diffused, f.colour.x, eye_near(), natural_bodies()) * P.glow);
    // What it covers fades into the water with distance, as its light does.
    cover *= water_keep(f.colour.x, eye_near());
    // With a cover pass (a variant that covers, in a shared space) what is
    // solid, from a half, was drawn there; drawn again here it would add.
    if (cover_pass() && !covering() && cover >= 0.5) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(c * P.glow, cover);
}

// How much of something dist from the eye is left by the water between
// (as water() dims its light, in grey).
fn water_keep(dist: f32, near: f32) -> f32 {
    return exp(-max(dist - near, 0.0) * 0.3);
}

// ---------------------------------------------------------------- the shared look

// The skin's normal with its fine wet relief: a gentle, uneven swell of
// the jelly, as a height over the plan tilting the normal along the skin's
// frame. The height is a few waves at odd angles and sizes, its slope
// worked out exactly: value noise showed its grid as blocks in a tight
// glint. Each wave fades where it would be finer than a pixel.
fn wet_normal(F: SkinFrame, plan: vec2<f32>, s: f32, fp: f32, seed: f32) -> vec3<f32> {
    var g = vec2<f32>(0.0);
    for (var i = 0u; i < 8u; i++) {
        let fi = f32(i);
        let a = fi * 2.399 + seed * 6.0;
        let k = 9.0 * pow(1.6, fi);
        let d = vec2<f32>(cos(a), sin(a)) * k;
        let fade = keep_detail(fp * k / TAU * 14.0);
        if (fade <= 0.0) {
            break;
        }
        g += d * cos(dot(plan, d) + fi * 1.7 + seed * 11.0) / k * (0.5 + 0.12 * fi) * fade;
    }
    let out = plan / max(s, 1e-3);
    let gr = dot(g, out);
    let ga = dot(g, vec2<f32>(-out.y, out.x));
    return normalize(F.n - (F.out * gr + F.round * ga) * 0.07);
}

// A light level that swells and ebbs: x near 0 unchanged, loud ones
// brightening less and less (so a run of loud onsets never blows it out).
fn soft(x: f32) -> f32 {
    return x / (1.0 + 0.45 * max(x, 0.0));
}

// ---------------------------------------------------------------- each species' own light

// How much of each light diffuses in a shared space: small bright points
// most (they bloom), broad patterns less (they would smear), comb rows
// least (their rainbows are sharp).
const STROKE_DIFFUSES: f32 = 0.5;
const ORGANS_DIFFUSE: f32 = 0.5;
const POINTS_DIFFUSE: f32 = 0.6;
const PATTERN_DIFFUSES: f32 = 0.3;
const COMBS_DIFFUSE: f32 = 0.25;
