// How every part looks: paint, per pixel of the triangles.
//
// One look for every bell (the luminous model): light from the jelly itself,
// more where the eye looks through more of it (glass) and where the skin is
// seen edge on, shaped by the light from above and glowing when the eye
// looks up through it; the inner skin lights as the swimming muscle
// squeezes, the light rising from the crown to the margin with the stroke.
// On top, each species' own organs and light (the *_light functions), and
// all of it dimmed and blued by the water between it and the eye. In a
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
    let id = f.data.w;
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
    if (id > PART_ARM) {
        c = shown(L_ARMS, arm_look(J, k, f.colour.y, f.colour.z, f.data.z, f.data.x, fp));
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
    }
    c = water(c, f.colour.x, eye_near(), natural_bodies());
    write_glow(water(diffused, f.colour.x, eye_near(), natural_bodies()) * P.glow);
    return vec4<f32>(c * P.glow, 0.0);
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
    // A little dimmer through the thick middle, brighter toward the rim.
    let body = tone(BODY, hue) * mix(0.85, 1.05, smoothstep(0.2, 1.0, L.s));
    let lit = 0.55 + 0.45 * L.top;
    let seed = vec2<f32>(J.hue * 13.0, J.hue * 7.0);
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
        c = shown(L_GLASS, body * sp.glass * scatter(L.face, bell_thickness(b, L.s), sp.density) * tissue * lit) + shown(L_EDGES, edge);
        // The light above on the wet skin: a glint sliding over the dome as it
        // turns, and the thin skin glowing when the eye looks up through it.
        c += shown(L_SHEEN, mix(body, vec3<f32>(1.0), 0.55) * L.sheen * 0.45);
        if (k != COMB) {
            // The thin margin glows, brightest on the lobes.
            let lobe = select(1.0, 0.5 - 0.5 * cos(fract(L.th * b.lobes) * TAU), b.lobes > 0.0);
            c += shown(L_EDGES, body * thin_line(1.0 - L.s, 0.012, L.fs) * (0.5 + 0.5 * lobe) * 0.9);
        }
    } else {
        c = shown(L_INNER, body * (0.012 + 0.08 * fresnel(L.face, 2.5) + 0.012 * fbm(L.plan * 10.0 + seed, 3)) * lit);
        c += shown(L_SHEEN, mix(body, vec3<f32>(1.0), 0.4) * L.sheen * 0.06);
        // The stroke's light: the swimming muscle in the outer bell glows as it squeezes.
        let zone = smoothstep(0.3, 0.45, L.s) * smoothstep(0.97, 0.88, L.s);
        let stroke = shown(L_STROKE, tone(LIGHT, hue) * zone * pow(max(L.squeeze, 0.0), 1.5) * sp.stroke_light * 0.22);
        c += diffuse(stroke, STROKE_DIFFUSES, P.body);
    }
    var own = vec3<f32>(0.0);
    // The moon jelly splits its own light over several layers itself; the
    // others' is their organs inside and their marks outside.
    let own_layer = select(L_MARKS, L_ORGANS, L.inner);
    switch (select(k, 6u, k != MOON && !layer_on(own_layer))) {
        case 6u: {}
        case 1u: { own = nettle_light(J, L); }
        case 2u: { own = crystal_light(J, L); }
        case 3u: { own = atolla_light(J, L); }
        case 4u: { own = comb_light(J, L); }
        case 5u: { own = lion_light(J, L); }
        default: { own = moon_light(J, L); }
    }
    return c * P.body + own * P.light;
}

// An oral arm at u (root 0 to tip 1), x across (-1..1): clear jelly, the
// light in its crenulated lips and a groove down its fold.
fn arm_look(J: Jelly, k: u32, u: f32, x: f32, face: f32, top: f32, fu: f32) -> vec3<f32> {
    let P = params();
    let hue = hue_of(J);
    let c = mix(tone(ARMS_C, hue), tone(ORGANS, hue), 0.15 * (1.0 - u));
    let rim = fresnel(face, 2.0);
    // The lips: crinkled, irregular edges (each side its own), catching the
    // light where they fold; a lace of fine folds in from them; a milky
    // groove down the middle where the arm folds.
    let side = select(0.0, 9.0, x > 0.0);
    let fine = keep_detail(fu * 8.0);
    let crinkle = 0.65 * noise(vec2<f32>(u * 55.0, side + J.hue * 5.0)) + 0.35 * noise(vec2<f32>(u * 130.0, side + 3.0));
    let edge = 1.0 - abs(x) - 0.1 * crinkle * fine;
    let lip = smoothstep(0.13, 0.0, edge) * (0.45 + 0.8 * crinkle);
    let beads = mix(1.0, 0.6 + 0.6 * noise(vec2<f32>(u * 110.0, side + x * 3.0)), fine);
    // (The lace only on the moon jelly: a lion's mane's curtains overlap
    // many times over, and every arm pixel pays for it.)
    var lace = 0.0;
    if (k == MOON) {
        lace = smoothstep(0.55, 0.05, edge) * (0.5 + 0.5 * noise(vec2<f32>(u * 70.0, x * 9.0 + side))) * 0.18 * fine;
    }
    let groove = thin_line(abs(x), 0.08, fu * 4.0) * 0.22 + lace;
    let tissue = 0.7 + 0.6 * fbm(vec2<f32>(u * 14.0, x * 2.5 + J.hue * 9.0), 3);
    var lips = 0.4;
    switch (k) {
        case 1u: { lips = 0.5; }
        case 2u: { lips = 0.25; }
        case 5u: { lips = 0.35; }
        default: {}
    }
    let light = ((0.014 + 0.08 * rim) * tissue + lip * beads * lips + groove) * (0.55 + 0.45 * top);
    return c * light * (1.0 - 0.6 * u) * (0.6 + 0.5 * J.glow) * P.body;
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

// Moon jelly. Outer skin: warts (clusters of stinging cells) and the eight
// rhopalia, bright beads in the notches that sparkle on the hi-hats. Inner
// skin, what shows through the glass: the stomach, four horseshoe gonads of
// lumpy folded tissue (breathing with the bass), sixteen radial canals (the
// eight under the rhopalia forking three times on the way out) joining the
// ring canal, and the fine striation of the swimming muscle.
fn moon_light(J: Jelly, L: Look) -> vec3<f32> {
    let hue = hue_of(J);
    let body = tone(BODY, hue);
    let seed = vec2<f32>(J.hue * 13.0, J.hue * 7.0);
    let s = L.s;
    let ang = L.th * TAU;
    // Luminescence swells and ebbs: loud music brightens it less and less.
    let glow = soft(J.glow);
    let flash = soft(J.flash);
    if (!L.inner) {
        // The warts (clusters of stinging cells): tiny, uneven in size and
        // light, few near the crown and thicker toward the margin.
        let w = cells(L.plan * 70.0 + seed, 1.0, 0.0);
        let size = 0.06 + 0.08 * fract(w.z * 7.3);
        let many = mix(0.85, 0.35, smoothstep(0.2, 0.9, s));
        let wart = smoothstep(size, size * 0.3, w.x) * step(many, w.z) * (0.4 + 0.6 * fract(w.z * 13.1)) * keep_detail(L.fp * 70.0);
        var c = shown(L_MARKS, mix(body, vec3<f32>(1.0), 0.3) * wart * 0.07);
        let x = fract(L.th * 8.0);
        let d = vec2<f32>(min(x, 1.0 - x) / 8.0 * TAU * s, 1.0 - s - 0.02);
        let rh = max(exp(-dot(d, d) / (0.012 * 0.012)) - 0.02, 0.0);
        // Each rhopalium's pigment spot (warm, small) and the hood of jelly
        // over its notch, a faint arc just inside it.
        let spot = exp(-dot(d - vec2<f32>(0.0, 0.006), d - vec2<f32>(0.0, 0.006)) / (0.004 * 0.004));
        c += shown(L_MARKS, vec3<f32>(1.0, 0.55, 0.25) * spot * 0.25 * (1.0 + glow));
        c += shown(L_MARKS, body * thin_line(abs(length(d) - 0.03), 0.003, L.fp) * smoothstep(0.0, 0.01, d.y + 0.01) * 0.12);
        c += diffuse(shown(L_MARKS, mix(tone(LIGHT, hue), vec3<f32>(1.0), 0.5) * rh * (1.0 + 1.2 * glow + 2.5 * flash)), POINTS_DIFFUSE, params().light);
        return c;
    }
    let gon = tone(ORGANS, hue);
    let canal = mix(body, gon, 0.2) + vec3<f32>(0.1);
    let q = L.plan;
    let fq = L.fp;
    var c = vec3<f32>(0.0);
    // The stomach in the middle: four lobes (the gastric pouches) reaching
    // toward the gonads, a faint milky fill inside a clearer outline.
    let lobe = 0.13 * (1.0 + 0.35 * cos(ang * 4.0 - 3.14159));
    let sd = s - lobe;
    c += shown(L_ORGANS, mix(body, gon, 0.4) * (smoothstep(0.0, -0.06, sd) * 0.05 + thin_line(abs(sd), 0.004, fq) * 0.16));
    // The gonads: four horseshoes round the stomach, open toward the middle.
    // Each is a thick ribbon of milky tissue folded on itself: a soft tube
    // across (fullest along its middle, a fine rim at each edge), its inner
    // edge ruffled into a frill, lumpy along its length; under each, the
    // round mouth of its subgenital pit.
    var g = 0.0;
    var rims = 0.0;
    var fil = 0.0;
    var pits = 0.0;
    for (var n = 0u; n < 4u; n++) {
        let a = (f32(n) + 0.5) * 1.5708;
        let dir = vec2<f32>(cos(a), sin(a));
        let off = q - dir * 0.3;
        let l = length(off);
        let round = atan2(off.y, off.x);
        let open = smoothstep(-0.8, -0.45, dot(off / max(l, 1e-4), dir));
        let ring = 0.105 + 0.02 * (noise(vec2<f32>(round * 1.5, f32(n) * 7.0)) - 0.5);
        let frill = 0.008 * sin(round * 34.0 + 6.0 * noise(off * 22.0 + f32(n))) * (0.4 + 0.6 * noise(off * 30.0 + 3.0));
        let half = select(0.032, 0.032 + frill, l < ring);
        let t = (l - ring) / half;
        let aa = fq / half;
        let band = smoothstep(1.0 + aa, 1.0 - aa, abs(t));
        let tube = sqrt(max(1.0 - t * t, 0.0));
        g += band * (0.3 + 0.7 * tube) * open;
        // The gastric filaments: a row of fine beads along the inner side.
        fil += smoothstep(0.3, 0.0, abs(t + 0.55)) * pow(0.5 + 0.5 * sin(round * 44.0 + f32(n) * 2.0), 6.0) * open * keep_detail(fq * 120.0);
        rims += thin_line(abs(abs(t) - 0.9) * half, 0.003, fq) * open;
        pits += thin_line(abs(length(off) - 0.05), 0.004, fq);
    }
    c += shown(L_ORGANS, mix(body, vec3<f32>(1.0), 0.3) * pits * 0.1);
    let lumps = cells(q * 70.0 + seed, 1.0, V.time * 0.2);
    let lumpy = mix(0.65, 1.15, smoothstep(0.0, 0.18, lumps.y - lumps.x));
    let grain = mix(1.0, lumpy * (0.75 + 0.5 * fbm(q * 40.0, 3)), keep_detail(fq * 70.0));
    let milky = mix(gon, vec3<f32>(1.0), 0.35);
    c += diffuse(shown(L_ORGANS, (milky * g * grain * 0.5 + gon * rims * 0.4 + mix(gon, vec3<f32>(1.0), 0.6) * fil * 0.35) * (0.55 + glow * 1.3)), ORGANS_DIFFUSE, params().light);
    // Sixteen radial canals from the stomach out to the ring canal. The
    // eight under the rhopalia and between them (per- and interradial)
    // branch again and again on the way out, each branch wandering its own
    // way, into a fan of fine twigs that join the ring canal; the eight
    // between those (adradial) run out unbranched.
    let span = smoothstep(0.14, 0.2, s) * smoothstep(0.95, 0.92, s);
    let u8 = L.th * 8.0;
    let k8 = floor(u8 + 0.5);
    var x = (u8 - k8) / 8.0 * TAU * s + (noise(vec2<f32>(s * 9.0, k8 * 3.7)) - 0.5) * 0.02;
    var w = 0.0055;
    var branch = 0.0;
    var starts = array<f32, 4>(0.36, 0.52, 0.66, 0.78);
    var spreads = array<f32, 4>(0.1, 0.055, 0.03, 0.016);
    for (var lvl = 0u; lvl < 4u; lvl++) {
        let grow = smoothstep(starts[lvl], starts[lvl] + 0.12, s);
        let jitter = 0.75 + 0.5 * hash21(vec2<f32>(k8 * 7.0 + branch, f32(lvl)));
        branch = branch * 2.0 + select(0.0, 1.0, x > 0.0);
        x = abs(x) - spreads[lvl] * jitter * grow;
        x += (noise(vec2<f32>(s * 5.0, branch * 3.1 + k8 * 11.0)) - 0.5) * 0.006 * grow;
        w *= mix(1.0, 0.8, grow);
    }
    var canals = thin_line(abs(x), w, fq) * span;
    let xa = (fract(u8) - 0.5) / 8.0 * TAU * s + (noise(vec2<f32>(s * 9.0, k8 * 5.3 + 2.0)) - 0.5) * 0.02;
    canals += thin_line(abs(xa), 0.005, fq) * span * mix(1.0, 0.7, s);
    canals += thin_line(abs(s - 0.93), 0.005, L.fs) * 0.8;
    c += shown(L_CANALS, canal * canals * (0.2 + 0.15 * glow));
    // The fringe's roots: a bead where each of the hundred and sixty
    // tentacles leaves the margin.
    let bx = abs(fract(L.th * 160.0 + 0.5) - 0.5) / 160.0 * TAU * s;
    let bulbs = thin_line(length(vec2<f32>(bx, 1.0 - s - 0.012)), 0.006, fq);
    c += shown(L_MARKS, tone(STRANDS_C, hue) * bulbs * 0.25 * (0.4 + 0.6 * hash21(vec2<f32>(floor(L.th * 160.0 + 0.5), J.hue * 3.0))));
    // The swimming muscle: a fine, broken striation in the outer bell, its
    // fibres short and wavering, so it reads as tissue, not as rings. It
    // brightens only a little on the squeeze (the stroke's light is the
    // soft glow in bell_look); lit hoops looked like a grid.
    let freq = 90.0;
    let warp = (noise(vec2<f32>(L.th * 40.0, s * 6.0) + seed) - 0.5) * 0.012;
    let fibres = smoothstep(0.35, 0.75, noise(vec2<f32>(L.th * 260.0, s * 30.0) + seed));
    let even = 0.4 + 1.2 * fbm(vec2<f32>(s * 25.0, L.th * 9.0) + seed, 2);
    let bands = mix(pow(0.5 + 0.5 * cos((s + warp) * TAU * freq), 6.0) * even * fibres, 0.12, clamp(L.fs * freq * 1.5, 0.0, 1.0));
    let zone = smoothstep(0.4, 0.5, s) * smoothstep(0.92, 0.85, s);
    c += shown(L_STROKE, body * bands * zone * (0.01 + 0.025 * max(L.squeeze, 0.0)));
    return c;
}

// Sea nettle. Outer skin: sixteen dark stripes radiating from the crown
// (they pulse with the mids and flash on its onsets), a speckle of
// photophores, the scalloped lappets lit. Inner: four frilled gonads.
fn nettle_light(J: Jelly, L: Look) -> vec3<f32> {
    let hue = hue_of(J);
    let s = L.s;
    if (!L.inner) {
        let a = L.th * TAU + 0.07 * (noise(vec2<f32>(s * 5.0, J.hue * 9.0 + L.th * 16.0)) - 0.5);
        let width = mix(0.6, 1.0, s);
        let stripes = pow(0.5 + 0.5 * cos(a * 16.0), 5.0 / width) * smoothstep(0.08, 0.35, s) * (0.6 + 0.4 * s);
        let aa = mix(1.0, 0.35, clamp(L.fp * 16.0 * 1.5, 0.0, 1.0));
        var c = diffuse(tone(ORGANS, hue) * mix(0.35, stripes, aa) * (0.4 + 1.6 * J.glow + 1.2 * J.flash), PATTERN_DIFFUSES, params().light);
        c += diffuse(tone(LIGHT, hue) * photophores(L.plan, 70.0, 0.3, V.time, L.fp) * 0.3 * (0.5 + J.glow), POINTS_DIFFUSE, params().light);
        let lappet = thin_line(1.0 - s, 0.02, L.fs) * (0.5 + 0.5 * cos(L.th * TAU * 32.0));
        c += tone(BODY, hue) * lappet * 0.5;
        return c;
    }
    var g = 0.0;
    for (var n = 0u; n < 4u; n++) {
        let a = (f32(n) + 0.5) * 1.5708;
        let off = L.plan - vec2<f32>(cos(a), sin(a)) * 0.34;
        let r = 0.13 + 0.03 * (noise(off * 18.0 + f32(n) * 5.0) - 0.5);
        g += smoothstep(r, r - 0.05, length(off));
    }
    return diffuse(tone(ORGANS, hue) * g * 0.15 * (0.5 + J.glow), ORGANS_DIFFUSE, params().light);
}

// Crystal jelly. Inner skin: a hundred hair-thin radial canals, thicker
// along the gonads in the outer half. Outer: a ring of green photocytes
// round the rim that flash on strong low onsets, the flash running round.
fn crystal_light(J: Jelly, L: Look) -> vec3<f32> {
    let hue = hue_of(J);
    let s = L.s;
    if (L.inner) {
        let u = L.th * 96.0;
        let dx = abs(fract(u + 0.5) - 0.5) / 96.0 * TAU * s;
        let canals = thin_line(dx, 0.0035, L.fp) * smoothstep(0.15, 0.25, s) * smoothstep(0.98, 0.94, s);
        let gonads = thin_line(dx, 0.009, L.fp) * smoothstep(0.45, 0.55, s) * smoothstep(0.88, 0.8, s);
        return tone(ORGANS, hue) * canals * 0.25 * (0.4 + J.glow) + diffuse(tone(ORGANS, hue) * gonads * 0.3 * (0.4 + J.glow), ORGANS_DIFFUSE, params().light);
    }
    let rim = smoothstep(0.88, 0.99, s);
    let dots = pow(0.5 + 0.5 * cos(L.th * TAU * 48.0), 4.0) * rim;
    let run = exp(-fract(L.th - J.pa) * 6.0) * J.flash;
    return diffuse(tone(LIGHT, hue) * dots * (1.5 + 2.0 * J.glow + 8.0 * J.flash + 6.0 * run), POINTS_DIFFUSE, params().light);
}

// Atolla. Outer skin: the coronal groove's edges and twenty lappet dots; on
// a drop the blue alarm pinwheel spins round the rim. Inner: the red cross
// of the stomach.
fn atolla_light(J: Jelly, L: Look) -> vec3<f32> {
    let hue = hue_of(J);
    let s = L.s;
    let b = bell_of(ATOLLA);
    if (L.inner) {
        let st = smoothstep(0.35, 0.1, s * (1.0 + 0.4 * cos(L.th * TAU * 4.0)));
        return diffuse(tone(ORGANS, hue) * st * 0.3 * (0.5 + J.glow), ORGANS_DIFFUSE, params().light);
    }
    let groove = thin_line(abs(abs(s - b.groove_at) - 0.04), 0.008, L.fs) * 0.4;
    let rim = smoothstep(0.88, 0.99, s);
    let dots = pow(0.5 + 0.5 * cos(L.th * TAU * 20.0), 8.0) * rim * 0.5;
    let wheel = exp(-fract(L.th - J.pa) * 8.0) * rim * J.flash * 8.0;
    return tone(BODY, hue) * groove + diffuse(tone(LIGHT, hue) * (dots + wheel), POINTS_DIFFUSE, params().light);
}

// Comb jelly. Outer skin: eight comb rows from pole to pole whose plates of
// cilia break the light into rainbows racing along them with the highs,
// over a faint thin-film shimmer of the whole body.
fn comb_light(J: Jelly, L: Look) -> vec3<f32> {
    let hue = hue_of(J);
    let s = L.s;
    if (L.inner) {
        return tone(ORGANS, hue) * fresnel(L.face, 2.0) * 0.05;
    }
    let m = abs(fract(L.th * 8.0 + 0.5) - 0.5);
    let row = exp(-m / 0.05) * smoothstep(0.1, 0.2, s) * smoothstep(0.9, 0.8, s);
    let plates = pow(0.5 + 0.5 * cos(s * TAU * 22.0 - J.pa * TAU * 1.5), 3.0);
    let natural = params().colours > 0.5 && params().colours < 1.5;
    let irid = select(colour(hue + s - J.pa * 0.2), spectrum(s * 2.0 - J.pa * 0.5 + L.th) * 0.7 + 0.3 * thin_film(L.face, 2.0 + s * 3.0 + J.pa * 0.2), natural);
    let sheen = thin_film(L.face, 1.5 + s) * fresnel(L.face, 2.0) * 0.04;
    return diffuse(irid * row * plates * (0.8 + J.glow * 2.5 + J.flash * 2.0), COMBS_DIFFUSE, params().light) + sheen * tone(BODY, hue);
}

// Lion's mane. Outer skin: thirty-two radial folds over the lobes; inner:
// the glowing heart of folded tissue in the middle.
fn lion_light(J: Jelly, L: Look) -> vec3<f32> {
    let hue = hue_of(J);
    let s = L.s;
    if (L.inner) {
        let heart = exp(-s * 3.0) * (0.7 + 0.6 * fbm(L.plan * 9.0 + J.hue * 5.0, 3));
        return diffuse(tone(ORGANS, hue) * heart * 0.6 * (0.5 + J.glow * 1.5), ORGANS_DIFFUSE, params().light);
    }
    let aa = mix(1.0, 0.25, clamp(L.fp * 32.0 * 1.5, 0.0, 1.0));
    let folds = mix(0.25, pow(0.5 + 0.5 * cos(L.th * TAU * 32.0), 8.0), aa) * 0.5 * s;
    return diffuse(tone(ORGANS, hue) * folds * (0.5 + J.glow * 1.5), PATTERN_DIFFUSES, params().light);
}
