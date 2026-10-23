// The lids (views Dark and Macro): skin over a shell a little larger than
// the eyeball (clear of the cornea), opened in an almond, the palpebral
// fissure, in the head's frame (x right, y up, toward the viewer), not the
// eye's: lids belong to the face. The upper lid rides with the gaze's
// height and closes on a blink; the opening's edges are the lid margins,
// rolled in, pink and wet, the lower one with a line of tears; the inner
// corner holds the caruncle; the lids shade the eye under them; lashes
// grow from the margins; the skin fades into the dark away from the eye.
// Measures in eyeball radii, as directions from the eye's centre.

const SHELL: f32 = 1.13;
const FISSURE_W: f32 = 0.8;

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

// The skin at direction d (head frame) on eye k's shell: lit, with pores
// and fine creases, the upper lid's crease, and darker away from the eye.
fn skin_colour(k: u32, d: vec3<f32>, n: vec3<f32>, rd: vec3<f32>, up_gap: f32, lo_gap: f32, pw: f32) -> vec3<f32> {
    let P = params();
    let salt = f32(k) * 3.1;
    // The upper lid's crease, a fold above the margin, and the lid's bulge.
    let crease_y = 0.3 + 0.08 * (1.0 - d.x * d.x);
    let crease = exp(-pow((d.y - crease_y) / 0.035, 2.0)) * step(0.0, d.y);
    var nn = normalize(n + vec3<f32>(0.0, 0.25, 0.0) * crease);
    // Pores and creases, fading when finer than a pixel.
    let fine = 1.0 - smoothstep(0.004, 0.012, pw);
    let pores = vnoise3(d * 140.0 + salt) * fine;
    let lines = vnoise3(vec3<f32>(d.x * 9.0, d.y * 70.0, salt)) * fine * 0.35;
    nn = normalize(nn + vec3<f32>(pores * 0.08, lines * 0.12 + pores * 0.08, 0.0));
    var base = skin_tone() * (0.92 + 0.08 * vnoise3(d * 18.0 + salt));
    // Redder and thinner near the margins.
    let near = exp(-min(-up_gap, -lo_gap) * 9.0);
    base = mix(base, base * vec3<f32>(1.15, 0.8, 0.8), clamp(near, 0.0, 1.0) * 0.5);
    let brow = smoothstep(crease_y + 0.02, crease_y + 0.3, d.y);
    let under = smoothstep(-0.5, -0.8, d.y);
    base = mix(base, base * vec3<f32>(0.75, 0.62, 0.72), under * 0.6);
    var col = base * lit(nn, 0.3) * (1.0 - 0.45 * crease) * (1.0 - 0.55 * brow) * (1.0 - 0.3 * under);
    // A faint sheen.
    col += room(reflect(rd, nn), 0.35) * fresnel(dot(-rd, nn), 0.03) * 0.5;
    // Into the dark away from the eye (more so in Dark).
    let away = length(vec2<f32>(d.x / 1.05, d.y / 0.7)) + 0.07 * vnoise3(vec3<f32>(d.xy * 3.0, f32(k)));
    let reach = select(0.62, 0.5, u32(P.view) == 0u);
    let fall = max(away - reach, 0.0);
    return col * exp(-fall * fall * 40.0);
}

// The lid margin: a rounded, wet, pink rim rolling in toward the eye. `g`
// is how far into the rim (0 at the skin's edge, 1 where it meets the eye).
fn margin_colour(d: vec3<f32>, rd: vec3<f32>, g: f32, upper: bool) -> vec3<f32> {
    let roll = select(1.0, -1.0, upper);
    let n = normalize(vec3<f32>(0.0, roll * (0.3 + g), 0.85 - 0.4 * g));
    let base = mix(skin_tone(), vec3<f32>(0.5, 0.2, 0.18), 0.6 + 0.3 * g);
    var col = base * lit(n, 0.4) * (1.0 - 0.5 * g);
    col += room(reflect(rd, n), 0.05) * fresnel(dot(-rd, n), 0.03) * 1.5 * params().wet;
    return col;
}

// Lashes at direction d over eye k: (coverage, how far out along the
// lash, 0 root .. 1 tip). They grow from the margins in front of
// everything: upper ones long, curving up and out, lower ones short and
// sparse.
fn lash_cover(k: u32, d: vec2<f32>, m: f32, mg: vec2<f32>, open: f32, fw: f32) -> f32 {
    let P = params();
    if (P.lashes <= 0.0) {
        return 0.0;
    }
    let spacing = 0.014;
    let base = floor(d.x / spacing);
    var cover = 0.0;
    for (var j = -6; j <= 6; j++) {
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
        let len = P.lashes * (0.07 + 0.07 * (1.0 - s * s) + 0.04 * outer) * (0.7 + 0.6 * rand(id, 5.1));
        let mrg = margins(xr, m, open, 0.0);
        let root = vec2<f32>(xr, mrg.x - 0.004);
        // Fanning toward the corners, first out over the eye, then curling
        // up: three pieces.
        let fan = xr * 0.45 + clump * 0.05 + (rand(id, 2.2) - 0.5) * 0.03;
        let p1 = root + vec2<f32>(fan * 0.15, -0.02 * len / 0.1);
        let p2 = root + vec2<f32>(fan * 0.55, len * 0.45);
        let p3 = root + vec2<f32>(fan * 1.0 + clump * 0.02, len);
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
        let w0 = 0.008;
        let w = w0 * (1.0 - 0.85 * t) + fw * 0.6;
        cover = max(cover, (1.0 - smoothstep(w * 0.45, w, dist)) * min(w0 * (1.0 - 0.85 * t) / w * 3.0, 1.0));
    }
    // A sparser, shorter row below.
    let lbase = floor(d.x / (spacing * 2.2));
    for (var j = -2; j <= 2; j++) {
        let i = lbase + f32(j);
        let h = rand(u32(i + 700.0) + k * 1000u, 8.3);
        let xr = (i + h) * spacing * 2.2;
        let s = abs(xr) / FISSURE_W;
        if (s > 0.85) {
            continue;
        }
        let len = P.lashes * 0.06 * (0.6 + 0.6 * rand(u32(i + 1300.0), 1.9));
        let mrg = margins(xr, m, open, 0.0);
        let root = vec2<f32>(xr, mrg.y);
        let tip = root + vec2<f32>(xr * 0.25, -len);
        let a = d - root;
        let b = tip - root;
        let t = clamp(dot(a, b) / dot(b, b), 0.0, 1.0);
        let dist = length(a - b * t);
        let w = 0.003 * (1.0 - 0.7 * t) + fw * 0.5;
        cover = max(cover, (1.0 - smoothstep(w * 0.5, w, dist)) * min(0.003 / w * 2.0, 1.0) * 0.8);
    }
    return cover;
}

// What a ray sees of eye k with its lids, given where it met the shell.
fn lidded_colour(k: u32, ro: vec3<f32>, rd: vec3<f32>, t_shell: f32) -> vec3<f32> {
    let f = frame_of(k);
    let p = ro + rd * t_shell;
    let d = (p - f.c) / (f.r * SHELL);
    let pw = pixel() * t_shell / FOCAL / (f.r * SHELL);
    let m = medial(k);
    let open = lids_open(k);
    let mg = margins(d.x, m, open, f.g.y);
    let up_gap = mg.x - d.y;
    let lo_gap = d.y - mg.y;
    let rim = 0.045;
    var col: vec3<f32>;
    if (abs(d.x) < FISSURE_W && up_gap > 0.0 && lo_gap > 0.0) {
        // Through the opening: the eye, shaded by the lids.
        let h = eye_hit(f, ro, rd);
        if (h.t > 0.0) {
            col = eye_colour(f, k, ro, rd, h);
        } else {
            col = vec3<f32>(0.0);
        }
        let shadow = smoothstep(0.0, 0.3, up_gap) * smoothstep(0.0, 0.07, lo_gap);
        let corners = 1.0 - 0.55 * smoothstep(0.35, 0.8, abs(d.x) / FISSURE_W);
        col *= mix(0.05, 1.0, shadow * shadow) * corners;
        // The caruncle, pink and wet in the inner corner.
        let inner = smoothstep(FISSURE_W * 0.72, FISSURE_W * 0.9, d.x * m);
        if (inner > 0.0) {
            let cn = normalize(vec3<f32>(d.x * 0.5, d.y * 2.0, 1.0));
            var car = vec3<f32>(0.55, 0.2, 0.2) * lit(cn, 0.4) * (0.8 + 0.2 * vnoise3(vec3<f32>(d.xy * 60.0, 1.0)));
            car += room(reflect(rd, cn), 0.08) * fresnel(dot(-rd, cn), 0.03) * 1.5 * params().wet;
            col = mix(col, car, inner);
        }
        // The line of tears along the lower margin.
        let tear = exp(-pow(lo_gap / 0.006, 2.0));
        col += room(reflect(rd, vec3<f32>(0.0, 0.6, 0.8)), 0.0) * tear * 0.08 * params().wet;
    } else if (up_gap > -rim && d.y > 0.0 && abs(d.x) < FISSURE_W) {
        col = margin_colour(vec3<f32>(d), rd, clamp(-up_gap / rim, 0.0, 1.0), true);
        col = mix(col, skin_colour(k, d, normalize(p - f.c), rd, up_gap, lo_gap, pw), smoothstep(0.6, 1.0, -up_gap / rim));
    } else if (lo_gap > -rim && d.y < 0.0 && abs(d.x) < FISSURE_W) {
        col = margin_colour(vec3<f32>(d), rd, clamp(-lo_gap / rim, 0.0, 1.0), false);
        col = mix(col, skin_colour(k, d, normalize(p - f.c), rd, up_gap, lo_gap, pw), smoothstep(0.6, 1.0, -lo_gap / rim));
    } else {
        col = skin_colour(k, d, normalize(p - f.c), rd, up_gap, lo_gap, pw);
    }
    // The lashes over it all, dark, with a little sheen.
    let lash = lash_cover(k, d.xy, m, mg, open, pw);
    let lash_col = vec3<f32>(0.006, 0.004, 0.003) + lit(vec3<f32>(0.0, 0.3, 1.0), 0.0) * 0.008;
    return mix(col, lash_col, lash);
}
