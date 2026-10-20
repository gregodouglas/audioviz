// The glass species' own light, organs and pigment: what the shared bell
// look asks of a variant. The glass is clear: it covers nothing.

fn bell_cover(k: u32, L: Look) -> f32 {
    return 0.0;
}

fn body_colour(k: u32, L: Look, body: vec3<f32>) -> vec3<f32> {
    return body;
}

// A sea nettle's stripes are pigment in the jelly: darker bands where less
// of its golden light comes through; an atolla's pedalia are thicker jelly.
fn bell_pigment(J: Jelly, k: u32, L: Look, b: Bell) -> f32 {
    if (k == NETTLE) {
        return 1.0 - 0.65 * nettle_stripes(J, L);
    }
    if (k == ATOLLA) {
        return atolla_pedalia(L, b);
    }
    return 1.0;
}

fn own_light(J: Jelly, k: u32, L: Look) -> vec3<f32> {
    switch (k) {
        case 1u: { return nettle_light(J, L); }
        case 2u: { return crystal_light(J, L); }
        case 3u: { return atolla_light(J, L); }
        case 4u: { return comb_light(J, L); }
        default: { return moon_light(J, L); }
    }
}

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
    let flash = soft(flash_of(J));
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
        // (Its own warm pigment on a natural body; the palette's light otherwise.)
        let pigment = select(tone(LIGHT, hue), vec3<f32>(1.0, 0.55, 0.25), natural_bodies() > 0.5);
        c += shown(L_MARKS, pigment * spot * 0.25 * (1.0 + glow));
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

// Sea nettle's sixteen stripes, 0..1: radiating from the crown, widening
// toward the margin, wavering a little, faded to their average where they
// would be finer than a pixel.
fn nettle_stripes(J: Jelly, L: Look) -> f32 {
    let s = L.s;
    let a = L.th * TAU + 0.07 * (noise(vec2<f32>(s * 5.0, J.hue * 9.0 + L.th * 16.0)) - 0.5);
    let width = mix(0.6, 1.0, s);
    let stripes = pow(0.5 + 0.5 * cos(a * 16.0), 5.0 / width) * smoothstep(0.08, 0.35, s) * (0.6 + 0.4 * s);
    let aa = mix(1.0, 0.35, clamp(L.fp * 16.0 * 1.5, 0.0, 1.0));
    return mix(0.35, stripes, aa);
}

// Sea nettle. Outer skin: sixteen stripes of dark pigment radiating from
// the crown (bell_look darkens the jelly under them), which glow with the
// mids and flash on its onsets, and a speckle of photophores. Inner: four
// frilled gonads.
fn nettle_light(J: Jelly, L: Look) -> vec3<f32> {
    let hue = hue_of(J);
    let s = L.s;
    if (!L.inner) {
        var c = diffuse(tone(ORGANS, hue) * nettle_stripes(J, L) * (0.06 + 0.9 * J.glow + 1.2 * flash_of(J)), PATTERN_DIFFUSES, params().light);
        c += diffuse(tone(LIGHT, hue) * photophores(L.plan, 70.0, 0.3, V.time, L.fp) * 0.3 * (0.5 + J.glow), POINTS_DIFFUSE, params().light);
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

// Crystal jelly. Inner skin: a big round stomach in the middle, and from
// its rim a hundred hair-thin radial canals, each wavering its own way, a
// wavy whitish gonad along the outer half of each. Outer: a ring of tiny
// green photocytes, one at the end of each canal, faint at rest, flashing
// on strong low onsets, the flash running round.
fn crystal_light(J: Jelly, L: Look) -> vec3<f32> {
    let hue = hue_of(J);
    let s = L.s;
    let u = L.th * 96.0;
    let k = floor(u + 0.5);
    let along = (fract(u + 0.5) - 0.5) / 96.0 * TAU * s;
    if (L.inner) {
        let dx = abs(along + (noise(vec2<f32>(s * 6.0, k * 3.1)) - 0.5) * 0.006 * s);
        let canals = thin_line(dx, 0.0028, L.fp) * smoothstep(0.32, 0.38, s) * smoothstep(0.98, 0.94, s);
        // (Each gonad its own wave: one wave for all drew a fishbone moire.)
        let wave = 0.003 * (noise(vec2<f32>(s * 30.0, k * 5.7)) - 0.5) * 2.0;
        let thick = 0.006 * (0.5 + noise(vec2<f32>(s * 25.0, k * 2.3)));
        let gonads = thin_line(abs(dx + wave * smoothstep(0.5, 0.6, s)), thick, L.fp) * smoothstep(0.5, 0.6, s) * smoothstep(0.88, 0.8, s);
        let stomach = thin_line(abs(s - 0.33), 0.006, L.fs) * 0.6 + smoothstep(0.34, 0.25, s) * 0.06;
        return tone(ORGANS, hue) * (canals * 0.22 + stomach * 0.3) * (0.4 + J.glow) + diffuse(mix(tone(ORGANS, hue), vec3<f32>(1.0), 0.4) * gonads * 0.3 * (0.4 + J.glow), ORGANS_DIFFUSE, params().light);
    }
    let d = vec2<f32>(along, 1.0 - s - 0.025);
    let dots = exp(-dot(d, d) / (0.008 * 0.008));
    let ring = smoothstep(0.9, 0.97, s) * smoothstep(1.0, 0.98, s) * 0.06;
    let flash = flash_of(J);
    let run = exp(-fract(L.th - J.pa) * 6.0) * flash;
    return diffuse(tone(LIGHT, hue) * (dots + ring) * (1.2 + 2.0 * J.glow + 5.0 * flash + 4.0 * run), POINTS_DIFFUSE, params().light);
}

// Atolla's jelly below the coronal groove: a ring of thick pedalia, one
// to each lappet, ridges of thicker jelly with furrows between (a factor on
// the light through it), evened out where they would be finer than a pixel.
fn atolla_pedalia(L: Look, b: Bell) -> f32 {
    let zone = smoothstep(b.groove_at + 0.04, b.groove_at + 0.12, L.s);
    let x = fract(L.th * b.lobes) * 2.0 - 1.0;
    let ridge = 1.0 - x * x;
    let aa = keep_detail(L.fp * b.lobes);
    return mix(1.0, mix(0.85, 0.45 + 0.8 * ridge, aa), zone);
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
    // Its lights are dark until it is stirred: a faint glimmer at rest.
    let dots = pow(0.5 + 0.5 * cos(L.th * TAU * 20.0), 8.0) * rim * 0.5 * (0.15 + 0.85 * J.glow);
    let wheel = exp(-fract(L.th - J.pa) * 8.0) * rim * flash_of(J) * 8.0;
    return tone(BODY, hue) * groove + diffuse(tone(LIGHT, hue) * (dots + wheel), POINTS_DIFFUSE, params().light);
}

// Comb jelly. Outer skin: eight comb rows from pole to pole, each a line
// of short plates of cilia across the row, faint and whitish; waves of
// beating run down each row (faster with the highs), and where the plates
// beat they break the light into a rainbow sliding along them; over it a
// faint thin-film shimmer of the whole body. Inner: the canal under each
// row, and the apical organ, a small bright statocyst at the top pole.
fn comb_light(J: Jelly, L: Look) -> vec3<f32> {
    let hue = hue_of(J);
    let s = L.s;
    let row = floor(L.th * 8.0 + 0.5);
    let dx = abs(fract(L.th * 8.0 + 0.5) - 0.5) / 8.0 * TAU * max(s, 0.05);
    let span = smoothstep(0.1, 0.18, s) * smoothstep(0.9, 0.82, s);
    if (L.inner) {
        let canal = thin_line(dx, 0.004, L.fp) * smoothstep(0.06, 0.14, s) * smoothstep(0.95, 0.88, s);
        let apical = exp(-s * s / (0.035 * 0.035));
        return tone(ORGANS, hue) * (fresnel(L.face, 2.0) * 0.05 + canal * 0.12) + mix(tone(LIGHT, hue), vec3<f32>(1.0), 0.6) * apical * 0.35;
    }
    let bar = thin_line(dx, 0.03, L.fp) * span;
    let cell = fract(s * 26.0);
    let plate = mix(smoothstep(0.0, 0.06, cell) * smoothstep(0.55, 0.4, cell), 0.4, clamp(L.fs * 26.0 * 1.5, 0.0, 1.0));
    let wave = pow(0.5 + 0.5 * cos(s * TAU * 2.5 - J.pa * TAU * 1.5 + row * 0.8), 2.0);
    let natural = params().colours > 0.5 && params().colours < 1.5;
    let irid = select(colour(hue + s - J.pa * 0.2), spectrum(s * 2.0 - J.pa * 0.5 + L.th + cell * 0.25) * 0.75 + 0.25 * thin_film(L.face, 2.0 + s * 3.0 + J.pa * 0.2), natural);
    let combs = (mix(tone(BODY, hue), vec3<f32>(1.0), 0.5) * 0.2 + irid * wave * (0.4 + J.glow * 3.2 + J.flash * 2.5)) * bar * plate;
    let base = thin_line(dx, 0.003, L.fp) * span * 0.06;
    let sheen = thin_film(L.face, 1.5 + s) * fresnel(L.face, 2.0) * 0.04;
    return diffuse(combs, COMBS_DIFFUSE, params().light) + (sheen + base) * tone(BODY, hue);
}
