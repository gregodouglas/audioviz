// How every part looks: paint, per pixel of the triangles.
//
// One look for every bell (the luminous model): light from the jelly itself,
// more where the eye looks through more of it (glass) and where the skin is
// seen edge on, shaped by the light from above and glowing when the eye
// looks up through it; the inner skin lights as the swimming muscle
// squeezes, the light rising from the crown to the margin with the stroke.
// On top, each species' own organs and light (each variant's own_light),
// and all of it dimmed and blued by the water between it and the eye. In a
// shared space part of each species' own light and of the stroke's light
// diffuses: it spreads softly through the body round where it is, instead
// of all of it sitting sharp on the skin.

// How tight the wet glint on a bell is (glint_at).
const GLINT_SHARP: f32 = 350.0;
// How deep the lappets cut into the margin (in s) where they are painted:
// a bell of sixteen or more, too many for the mesh's sectors to round.
const LAPPET_DEPTH: f32 = 0.05;

// The painted lappets of species k: how many round the margin, and how deep
// (0: none). A lion's mane's eight lobes are each scalloped into four.
fn lappets(k: u32) -> vec2<f32> {
    let b = bell_of(k);
    if (k == LION) {
        return vec2<f32>(32.0, 0.025);
    }
    return select(vec2<f32>(0.0), vec2<f32>(b.lobes, LAPPET_DEPTH), b.lobes >= 16.0);
}

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
        let m = arm_paint(J, k, f, fp);
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
        c = bell_look(J, k, L);
        cover = bell_cover(k, L) * select(0.0, 1.0, layer_on(select(L_GLASS, L_INNER, L.inner)));
    }
    c = water(c, f.colour.x, eye_near(), natural_bodies());
    write_glow(water(diffused, f.colour.x, eye_near(), natural_bodies()) * P.glow);
    // What it covers fades into the water with distance, as its light does.
    cover *= water_keep(f.colour.x, eye_near());
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

fn bell_look(J: Jelly, k: u32, L: Look) -> vec3<f32> {
    let P = params();
    let sp = species_of(k);
    let b = bell_of(k);
    let hue = hue_of(J);
    // A little dimmer through the thick middle, brighter toward the rim
    // (and as the variant colours it: body_colour).
    let body = body_colour(k, L, tone(BODY, hue) * mix(0.85, 1.05, smoothstep(0.2, 1.0, L.s)));
    let lit = 0.55 + 0.45 * L.top;
    let seed = vec2<f32>(J.hue * 13.0, J.hue * 7.0);
    // Lappets finer than the mesh can shape are cut here instead: rounded
    // scallops meeting in notches, the pixels beyond them left out (with
    // two or three sectors each the mesh drew them as saw teeth).
    var rim_s = 1.0;
    var cut = 1.0;
    let lp = lappets(k);
    if (lp.y > 0.0) {
        let x = fract(L.th * lp.x) * 2.0 - 1.0;
        rim_s = 1.0 - lp.y * (1.0 - sqrt(max(1.0 - x * x, 0.0)));
        cut = smoothstep(rim_s + L.fs, rim_s - L.fs, L.s);
    }
    var c = vec3<f32>(0.0);
    if (!L.inner) {
        // The jelly is not quite even: a soft mottling through it, and faint
        // fibres running out from the crown (noise of the direction out, so
        // it has no seam).
        let out = L.plan / max(L.s, 1e-3);
        let fibres = 0.9 + 0.15 * noise(out * 26.0 + vec2<f32>(L.s * 3.0, J.hue * 11.0)) * keep_detail(L.fp * 26.0 / max(L.s, 0.05));
        let tissue = (0.75 + 0.5 * fbm(L.plan * 6.0 + seed, 3)) * fibres;
        let rimzone = select(smoothstep(0.1, 0.7, L.s), 1.0, k == COMB);
        // The jelly scatters the light through it (more, and whiter, where
        // the eye looks through more of it); its wet skin, seen edge on,
        // mirrors the bright water above where the reflection looks up and
        // stays dark where it looks down.
        let edge = mix(body, vec3<f32>(1.0), 0.3 * L.sky) * sp.edge * fresnel(L.face, 4.0) * rimzone * (0.35 * lit + 0.9 * L.sky);
        // Pigment in the jelly, the variant's (bell_pigment): less of its
        // light comes through there.
        let pigment = bell_pigment(J, k, L, b);
        c = shown(L_GLASS, body * sp.glass * scatter(L.face, bell_thickness(b, L.s), sp.density) * tissue * lit * pigment) + shown(L_EDGES, edge);
        // The light above on the wet skin: a glint sliding over the dome as it
        // turns, and the thin skin glowing when the eye looks up through it.
        c += shown(L_SHEEN, mix(body, vec3<f32>(1.0), 0.55) * L.sheen * 0.45);
        if (k != COMB) {
            // The thin margin glows, brightest on the lobes.
            let lobe = select(1.0, 0.5 - 0.5 * cos(fract(L.th * b.lobes) * TAU), b.lobes > 0.0);
            c += shown(L_EDGES, body * thin_line(abs(rim_s - L.s), 0.012, L.fs) * (0.5 + 0.5 * lobe) * 0.9);
        }
    } else {
        c = shown(L_INNER, body * (0.012 + 0.08 * fresnel(L.face, 2.5) + 0.012 * fbm(L.plan * 10.0 + seed, 3)) * lit);
        c += shown(L_SHEEN, mix(body, vec3<f32>(1.0), 0.4) * L.sheen * 0.06);
        // The stroke's light: the swimming muscle in the outer bell glows as it squeezes.
        let zone = smoothstep(0.3, 0.45, L.s) * smoothstep(0.97, 0.88, L.s);
        let stroke = shown(L_STROKE, tone(LIGHT, hue) * zone * pow(max(L.squeeze, 0.0), 1.5) * sp.stroke_light * 0.22);
        c += diffuse(stroke, STROKE_DIFFUSES, P.body);
    }
    // Each species' own light (the variant's: own_light). The moon jelly
    // splits its own over several layers itself; the others' is their organs
    // inside and their marks outside.
    var own = vec3<f32>(0.0);
    if (k == MOON || layer_on(select(L_MARKS, L_ORGANS, L.inner))) {
        own = own_light(J, k, L);
    }
    return (c * P.body + own * P.light) * cut;
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
