// The lids and the skin round the eye (views Dark and Macro), as real
// geometry: a signed distance field marched along each ray, in the head's
// frame (x right, y up, z toward the viewer; lids belong to the face, not
// to the eye), in eyeball radii from the eye's centre.
//
// The lids are a thick shell over the eyeball (inside 1.015, outside about
// 1.115: a real lid margin is about 1.5 mm thick where it meets the eye),
// cut open in an almond, the
// palpebral fissure, its edges rounded: those rounded edges are the lid
// margins, with real thickness. Round it the surface rises into the brow
// above, folds in at the upper lid's crease, rolls under the lower lid and
// swells into the cheek below. The upper lid rides with the gaze's height
// and closes on a blink. Everything is lit from its own shape: the lids
// cast soft shadows on the eye, folds darken by occlusion, the margins are
// pink and wet with a grey line and gland openings, tears gather along the
// lower lid, the caruncle fills the inner corner, lashes grow from the
// margins and shade the eye, and the skin fades into the dark.

const SHELL: f32 = 1.6;
const FISSURE_W: f32 = 0.8;
const LID_IN: f32 = 1.015;
const LID_OUT: f32 = 1.115;

fn has_lids() -> bool {
    return u32(params().view) != 1u;
}

// Which side eye k's inner corner is on (-1 left, 1 right).
fn medial(k: u32) -> f32 {
    if (macro_view()) {
        return -1.0;
    }
    return select(-1.0, 1.0, k % 2u == 0u);
}

// How open eye k's lids are: the droop of quiet passages times a blink
// (shut in about 80 ms, held, open again in about 200).
fn lids_open(k: u32) -> f32 {
    let since = V.time - item(E0 + k).x;
    let shut = smoothstep(0.0, 0.08, since) * (1.0 - smoothstep(0.12, 0.32, since));
    return item(2u).z * (1.0 - shut);
}

// The lid margins' heights at x (a direction's x across the eye, `m` the
// inner corner's side): (upper, lower). The upper lid's arch peaks a
// little toward the inner corner and follows the gaze's height; closing,
// both meet a little below the middle.
fn margins(x: f32, m: f32, open: f32, look_y: f32) -> vec2<f32> {
    let s = clamp(abs(x) / FISSURE_W, 0.0, 1.0);
    let shape_u = pow(1.0 - pow(s, 2.2), 0.8) * (1.0 + 0.12 * x * m);
    let shape_l = pow(1.0 - pow(s, 2.0), 1.0) * (1.0 - 0.08 * x * m);
    let up_open = 0.4 * shape_u + 0.3 * look_y * shape_u;
    let lo_open = -0.4 * shape_l + 0.12 * look_y * shape_l;
    let shut = -0.12 * shape_l;
    let a = clamp(open, 0.0, 1.0);
    return vec2<f32>(mix(shut, up_open, a), mix(shut - 0.01, lo_open, pow(a, 0.35)));
}

fn skin_tone() -> vec3<f32> {
    return mix(vec3<f32>(0.55, 0.36, 0.27), vec3<f32>(0.16, 0.085, 0.055), params().skin);
}

// ---------------------------------------------------------------- the shape

struct Lids {
    m: f32,
    open: f32,
    look: f32,
    salt: f32,
};

fn smax(a: f32, b: f32, k: f32) -> f32 {
    let h = max(k - abs(a - b), 0.0) / k;
    return max(a, b) + h * h * k * 0.25;
}

// How far inside the opening a direction is (negative inside), roughly as
// an angle.
fn opening(dir: vec3<f32>, L: Lids) -> f32 {
    if (dir.z < 0.05) {
        return 1.0;
    }
    // Beyond a corner, the distance grows from the corner itself (a margin
    // of height zero there would cut a groove out to the temple).
    let cx = abs(dir.x) - FISSURE_W;
    if (cx > 0.0) {
        return length(vec2<f32>(cx, dir.y));
    }
    let mg = margins(dir.x, L.m, L.open, L.look);
    return max(dir.y - mg.x, mg.y - dir.y);
}

// The outer surface's radius in a direction: the brow above, the upper
// lid's crease, the roll under the lower lid, the cheek below.
fn outer_radius(dir: vec3<f32>, L: Lids) -> f32 {
    let mg = margins(dir.x, L.m, 1.0, 0.0);
    let crease_y = mg.x + 0.22 + 0.04 * (1.0 - dir.x * dir.x);
    // The crease and the roll fade out past the corners.
    let within = 1.0 - smoothstep(FISSURE_W * 0.85, FISSURE_W * 1.2, abs(dir.x));
    var r = LID_OUT;
    r += 0.3 * smoothstep(crease_y + 0.02, crease_y + 0.4, dir.y);
    r -= 0.035 * exp(-pow((dir.y - crease_y) / 0.05, 2.0)) * within;
    r += 0.035 * exp(-pow((dir.y - mg.y + 0.09) / 0.05, 2.0)) * step(dir.y, 0.0) * within;
    r += 0.16 * smoothstep(mg.y - 0.25, mg.y - 0.7, dir.y);
    return r;
}

fn lid_sdf(q: vec3<f32>, L: Lids) -> f32 {
    let len = length(q);
    let dir = q / max(len, 1e-4);
    let shell = max(len - outer_radius(dir, L), LID_IN - len);
    return smax(shell, -opening(dir, L) * len * 0.8, 0.06);
}

fn lid_normal(q: vec3<f32>, L: Lids) -> vec3<f32> {
    let e = 0.0015;
    let a = vec3<f32>(1.0, -1.0, -1.0);
    let b = vec3<f32>(-1.0, -1.0, 1.0);
    let c = vec3<f32>(-1.0, 1.0, -1.0);
    let d = vec3<f32>(1.0, 1.0, 1.0);
    return normalize(a * lid_sdf(q + a * e, L) + b * lid_sdf(q + b * e, L) + c * lid_sdf(q + c * e, L) + d * lid_sdf(q + d * e, L));
}

// Soft shadow toward the key light, from the lids alone.
fn lid_shadow(q: vec3<f32>, L: Lids) -> f32 {
    let dir = normalize(KEY);
    var res = 1.0;
    var t = 0.02;
    for (var i = 0; i < 22; i++) {
        let h = lid_sdf(q + dir * t, L);
        res = min(res, 9.0 * h / t);
        if (res < 0.01 || t > 1.2) {
            break;
        }
        t += clamp(h, 0.012, 0.12);
    }
    return clamp(res, 0.0, 1.0);
}

fn lid_occlusion(q: vec3<f32>, n: vec3<f32>, L: Lids) -> f32 {
    var occ = 0.0;
    var w = 1.0;
    for (var i = 1; i <= 3; i++) {
        let h = 0.03 * f32(i);
        occ += (h - lid_sdf(q + n * h, L)) * w;
        w *= 0.6;
    }
    return clamp(1.0 - occ * 6.0, 0.0, 1.0);
}

// ---------------------------------------------------------------- the skin

// The skin at q (local), facing n: subsurface-soft light, pores and fine
// lines in its normal, an oily sheen, redder near the margins, the margin
// itself pink and wet; all fading into the dark away from the eye.
fn skin_colour(k: u32, q: vec3<f32>, n0: vec3<f32>, rd: vec3<f32>, L: Lids, pw: f32) -> vec3<f32> {
    let P = params();
    let dir = normalize(q);
    let salt = L.salt;
    let fine = 1.0 - smoothstep(0.003, 0.01, pw);
    // Pores, fine lines (across the lids, fanning at the outer corner: crow's
    // feet) and a little unevenness, in the normal.
    let pores = vnoise3(q * 160.0 + salt) * fine;
    let lines = vnoise3(vec3<f32>(dir.x * 7.0, dir.y * 60.0, salt)) * fine;
    let outer = smoothstep(0.55, 0.95, -dir.x * L.m);
    let crow = vnoise3(vec3<f32>(atan2(dir.y, -dir.x * L.m) * 30.0, length(dir.xy) * 4.0, salt + 3.0)) * outer * fine;
    let pore_pit = smoothstep(0.35, 0.8, vnoise3(q * 210.0 + salt + 1.0)) * fine;
    var n = normalize(n0 + vec3<f32>(pores * 0.16 + crow * 0.06, lines * 0.1 + pores * 0.16 + crow * 0.1, 0.0));
    let am = opening(dir, L);
    // The margin: the rounded edge within a little of the opening.
    let rim = smoothstep(-0.0, 0.07, am) * (1.0 - smoothstep(0.07, 0.13, am));
    let wet_edge = 1.0 - smoothstep(0.0, 0.02, am);
    var base = skin_tone() * (0.9 + 0.1 * vnoise3(q * 14.0 + salt));
    base *= 1.0 - 0.22 * pore_pit;
    let blotch = smoothstep(0.1, 0.7, vnoise3(q * 5.0 + salt + 9.0));
    base = mix(base, base * vec3<f32>(1.12, 0.86, 0.84), blotch * 0.5);
    let pigment = exp(-max(am, 0.0) * 3.0) * smoothstep(0.3, 0.9, -dir.y * 2.0 + 0.6);
    base = mix(base, base * vec3<f32>(0.8, 0.7, 0.72), pigment * 0.35);
    let veinlet = smoothstep(0.92, 1.0, 1.0 - abs(vnoise3(vec3<f32>(dir.x * 18.0, dir.y * 8.0, salt + 4.0)))) * exp(-max(am, 0.0) * 6.0) * fine;
    base = mix(base, base * vec3<f32>(0.75, 0.78, 0.95), veinlet * 0.4);
    // Thin, flushed skin near the eye; a cooler, darker hollow under it.
    base = mix(base, base * vec3<f32>(1.18, 0.82, 0.82), exp(-max(am, 0.0) * 8.0) * 0.6);
    let under = smoothstep(-0.5, -0.8, dir.y) * smoothstep(0.9, 0.5, abs(dir.x));
    base = mix(base, base * vec3<f32>(0.78, 0.66, 0.76), under * 0.5);
    // The margin: pink mucosa at its inner edge, the grey line, gland
    // openings, and skin outside.
    let mucosa = vec3<f32>(0.42, 0.22, 0.2);
    base = mix(base, mucosa, wet_edge * 0.7);
    let grey = exp(-pow((am - 0.022) / 0.006, 2.0)) * fine;
    base *= 1.0 - 0.35 * grey;
    let glands = smoothstep(0.75, 0.95, vnoise3(vec3<f32>(dir.x * 220.0, am * 40.0, salt))) * exp(-pow((am - 0.025) / 0.01, 2.0)) * fine;
    base *= 1.0 - 0.5 * glands;
    // Light: wrapped (light scattering under skin), red where it scatters
    // round into shadow, occlusion in the folds, the lids' own shadows.
    let sh = lid_shadow(q + n0 * 0.01, L);
    let ao = lid_occlusion(q, n0, L);
    let key = max((dot(n, normalize(KEY)) + 0.25) / 1.25, 0.0) * sh;
    let strip = max(dot(n, normalize(STRIP)), 0.0);
    var col = base * (light_tint(0.15) * key * 1.3 + light_tint(0.6) * strip * 0.25 + vec3<f32>(0.03)) * ao;
    col += base * vec3<f32>(0.22, 0.04, 0.03) * smoothstep(0.35, -0.1, dot(n, normalize(KEY))) * ao;
    // An oily sheen, sharper and stronger on the wet margin.
    let gloss = mix(0.4, 0.03, wet_edge);
    col += room(reflect(rd, n), gloss) * fresnel(dot(-rd, n), mix(0.028, 0.03, wet_edge)) * mix(0.12, 0.6 * P.wet, wet_edge) * mix(0.5 + 0.5 * sh, 1.0, wet_edge);
    // Into the dark away from the eye (more so in Dark), its edge uneven.
    let away = length(vec2<f32>(dir.x / 1.05, dir.y / 0.72)) + 0.07 * vnoise3(vec3<f32>(dir.xy * 3.0, salt));
    let reach = select(0.62, 0.5, u32(P.view) == 0u);
    let fall = max(away - reach, 0.0);
    return col * exp(-fall * fall * 40.0) * (1.0 - 0.0 * rim);
}

// ---------------------------------------------------------------- lashes

// Lashes at direction d (on a sphere of the lids' outer radius) over eye
// k: coverage. They grow from the margins in front of everything: upper
// ones long, clumped, curving up and out in three pieces, thicker at the
// root; lower ones short and sparse.
fn lash_cover(k: u32, d: vec2<f32>, m: f32, open: f32, fw: f32) -> f32 {
    let P = params();
    if (P.lashes <= 0.0 || !layer_on(6u)) {
        return 0.0;
    }
    let spacing = 0.0095;
    let base = floor(d.x / spacing);
    var cover = 0.0;
    for (var j = -9; j <= 9; j++) {
        let i = base + f32(j);
        let id = u32(i + 500.0) + k * 1000u;
        let xr = (i + rand(id, 3.7)) * spacing;
        let s = abs(xr) / FISSURE_W;
        if (s > 0.95) {
            continue;
        }
        // Clumps: neighbours lean together.
        let clump = vnoise3(vec3<f32>(xr * 45.0, f32(k), 2.0));
        let outer = clamp(-xr * m / FISSURE_W, 0.0, 1.0);
        let long = select(1.0, 1.5, rand(id, 6.6) > 0.85);
        let len = P.lashes * (0.05 + 0.055 * (1.0 - s * s) + 0.035 * outer) * (0.6 + 0.8 * rand(id, 5.1)) * long;
        let mrg = margins(xr, m, open, 0.0);
        let root = vec2<f32>(xr, mrg.x - 0.004);
        let fan = xr * 0.45 + clump * 0.06 + (rand(id, 2.2) - 0.5) * 0.07;
        let p1 = root + vec2<f32>(fan * 0.15, -0.02 * len / 0.1);
        let p2 = root + vec2<f32>(fan * 0.55, len * 0.45);
        let curl = 0.6 + 0.5 * rand(id, 8.8);
        let p3 = root + vec2<f32>(fan * (1.0 + curl * 0.3) + clump * 0.02, len * curl);
        var dist = 1e9;
        var t = 0.0;
        let pts = array<vec2<f32>, 4>(root, p1, p2, p3);
        for (var q = 0; q < 3; q++) {
            let a = d - pts[q];
            let b = pts[q + 1] - pts[q];
            let tt = clamp(dot(a, b) / dot(b, b), 0.0, 1.0);
            let dd = length(a - b * tt);
            if (dd < dist) {
                dist = dd;
                t = (f32(q) + tt) / 3.0;
            }
        }
        let w0 = 0.0085;
        let w = w0 * (1.0 - 0.92 * t) + fw * 0.6;
        cover = max(cover, (1.0 - smoothstep(w * 0.3, w, dist)) * min(w0 * (1.0 - 0.92 * t) / w * 4.0, 1.0) * (1.0 - 0.25 * t));
    }
    // The lash line: their roots crowded along the upper margin.
    let mg0 = margins(d.x, m, open, 0.0);
    let line = exp(-pow((d.y - mg0.x - 0.006) / (0.006 + fw), 2.0)) * (1.0 - smoothstep(0.8, 0.98, abs(d.x) / FISSURE_W));
    cover = max(cover, line * 0.9 * P.lashes / max(P.lashes, 1.0));
    let lbase = floor(d.x / (spacing * 2.2));
    for (var j = -2; j <= 2; j++) {
        let i = lbase + f32(j);
        let h = rand(u32(i + 700.0) + k * 1000u, 8.3);
        let xr = (i + h) * spacing * 2.2;
        let s = abs(xr) / FISSURE_W;
        if (s > 0.85) {
            continue;
        }
        let len = P.lashes * 0.035 * (0.5 + 0.8 * rand(u32(i + 1300.0), 1.9));
        let mrg = margins(xr, m, open, 0.0);
        let root = vec2<f32>(xr, mrg.y + 0.035);
        let tip = root + vec2<f32>(xr * 0.25, -len);
        let a = d - root;
        let b = tip - root;
        let t = clamp(dot(a, b) / dot(b, b), 0.0, 1.0);
        let dist = length(a - b * t);
        let w = 0.0025 * (1.0 - 0.85 * t) + fw * 0.5;
        cover = max(cover, (1.0 - smoothstep(w * 0.45, w, dist)) * min(0.003 / w * 2.0, 1.0) * 0.55 * step(0.4, rand(u32(i + 1700.0), 4.4)));
    }
    return cover;
}

// ---------------------------------------------------------------- together

// What a ray sees of eye k with its lids: marched from where it enters the
// eye's region; the eye where the ray goes through the opening, shaded by
// the lids' shadow and occlusion and the lashes'; the lashes over all.
fn lidded_colour(k: u32, ro: vec3<f32>, rd: vec3<f32>, t_enter: f32) -> vec3<f32> {
    let f = frame_of(k);
    var L: Lids;
    L.m = medial(k);
    L.open = lids_open(k);
    L.look = f.g.y;
    L.salt = f32(k) * 3.1;
    let lo = (ro - f.c) / f.r;
    let eh = eye_hit(f, ro, rd);
    let t_eye = select(1e9, eh.t / f.r, eh.t > 0.0);
    var t = t_enter / f.r;
    var hit = false;
    for (var i = 0; i < 110; i++) {
        let q = lo + rd * t;
        let d = lid_sdf(q, L);
        if (d < 0.0004 * t) {
            hit = true;
            break;
        }
        t += d * 0.6;
        if (t > t_eye || t > t_enter / f.r + 2.0 * SHELL) {
            break;
        }
    }
    let pw = pixel() * t / FOCAL;
    var col = vec3<f32>(0.0);
    if (hit && t < t_eye) {
        if (layer_on(5u)) {
            let q = lo + rd * t;
            col = skin_colour(k, q, lid_normal(q, L), rd, L, pw);
        }
    } else if (eh.t > 0.0) {
        col = eye_colour(f, k, ro, rd, eh);
        let q = lo + rd * t_eye;
        let dir = normalize(q);
        // The lids' shadow and occlusion on the eye, and the lashes'
        // shadow just under the upper lid.
        let sh = lid_shadow(q * 1.01, L);
        let mg_e = margins(dir.x, L.m, L.open, L.look);
        let from_top = mg_e.x - dir.y;
        let from_bottom = dir.y - mg_e.y;
        let ao = smoothstep(0.0, 0.32, from_top) * smoothstep(0.0, 0.06, from_bottom) * (1.0 - 0.4 * smoothstep(0.45, 0.95, abs(dir.x) / FISSURE_W));
        let lash_sh = lash_cover(k, dir.xy + vec2<f32>(0.025, 0.045), L.m, L.open, 0.01);
        col *= mix(0.15, 1.0, sh) * mix(0.18, 1.0, ao * ao) * (1.0 - 0.45 * lash_sh);
        // The caruncle, pink and wet in the inner corner, with the fold
        // beside it (the plica).
        let inner = smoothstep(FISSURE_W * 0.7, FISSURE_W * 0.9, dir.x * L.m);
        if (inner > 0.0) {
            let bump = vec3<f32>(vnoise3(q * 26.0) + 0.5 * vnoise3(q * 55.0), vnoise3(q * 26.0 + 3.0) + 0.5 * vnoise3(q * 55.0 + 3.0), 0.0);
            let cn = normalize(vec3<f32>(dir.x * 0.6, dir.y * 2.0, 1.0) + bump * 0.12);
            var car = vec3<f32>(0.55, 0.19, 0.18) * lit(cn, 0.4) * (0.85 + 0.15 * vnoise3(q * 30.0)) * mix(0.3, 1.0, sh);
            car += room(reflect(rd, cn), 0.12) * fresnel(dot(-rd, cn), 0.03) * 0.4 * params().wet;
            let plica = exp(-pow((dir.x * L.m - FISSURE_W * 0.66) / 0.02, 2.0));
            col = mix(col, car, inner);
            col = mix(col, col * vec3<f32>(1.0, 0.7, 0.7), plica * 0.6);
        }
        // Tears gathered along the lower lid: a bright thread.
        let mg = margins(dir.x, L.m, L.open, L.look);
        let tear = exp(-pow((dir.y - mg.y) / 0.007, 2.0)) * (1.0 - smoothstep(0.7, 0.9, abs(dir.x) / FISSURE_W));
        col += room(reflect(rd, normalize(vec3<f32>(0.0, 0.7, 0.7))), 0.02) * tear * 0.05 * params().wet;
    }
    // The lashes over it all.
    let ts = sphere_hit(lo, rd, vec3<f32>(0.0), LID_OUT);
    if (ts > 0.0) {
        let dir = normalize(lo + rd * ts);
        let lash = lash_cover(k, dir.xy, L.m, L.open, pixel() * ts / FOCAL);
        let fade = exp(-pow(max(length(vec2<f32>(dir.x / 1.05, dir.y / 0.72)) - 0.62, 0.0), 2.0) * 40.0);
        col = mix(col, vec3<f32>(0.006, 0.004, 0.003), min(lash * fade * 1.5, 1.0));
    }
    return col;
}
