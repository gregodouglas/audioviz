// The light: a dark room with a large softbox above left (the catchlight
// every eye photograph has), a thin strip light to the right, and a dim
// fill. Colours Palette light tints them with the palette. Then the white:
// wet, translucent tissue with its veins, and the cornea.

const KEY: vec3<f32> = vec3<f32>(-0.48, 0.55, 0.68);
const STRIP: vec3<f32> = vec3<f32>(0.85, 0.05, 0.52);

fn palette_at(t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
}

// How strongly the room's lights take the palette (Palette light, or
// Palette, which colours the irises and lets the light stay white).
fn light_tint(at: f32) -> vec3<f32> {
    let lit = select(0.0, V.palette_mix, u32(params().colours) == 2u);
    return mix(vec3<f32>(1.0), palette_at(V.palette_shift + at) * 1.4, lit);
}

// A rectangle of light round direction `towards`, `w` by `h` radians,
// its edges `soft` wide.
fn panel(dir: vec3<f32>, towards: vec3<f32>, w: f32, h: f32, soft: f32) -> f32 {
    let t = normalize(towards);
    let side = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), t));
    let up = cross(t, side);
    let c = dot(dir, t);
    if (c <= 0.0) {
        return 0.0;
    }
    let a = vec2<f32>(dot(dir, side), dot(dir, up)) / c;
    let q = abs(a) - vec2<f32>(w, h);
    let inside = (1.0 - smoothstep(-soft, soft, q.x)) * (1.0 - smoothstep(-soft, soft, q.y));
    // Brighter toward its middle, as a diffuser is.
    return inside * (0.65 + 0.35 * (1.0 - length(a / vec2<f32>(w, h)) * 0.7));
}

// What a mirror facing `dir` sees; `rough` blurs the lights.
fn room(dir: vec3<f32>, rough: f32) -> vec3<f32> {
    if (!layer_on(4u)) {
        return vec3<f32>(0.0);
    }
    let key = panel(dir, KEY, 0.17, 0.13, 0.015 + rough) * 60.0;
    let frame = panel(dir, KEY, 0.2, 0.16, 0.006 + rough) - panel(dir, KEY, 0.17, 0.13, 0.006 + rough);
    let strip = panel(dir, STRIP, 0.022, 0.22, 0.006 + rough) * 16.0;
    // The window: four panes, the bars between them dark.
    let win_dir = vec3<f32>(0.55, 0.32, 0.77);
    let pane = panel(dir, win_dir, 0.1, 0.13, 0.006 + rough);
    let wt = normalize(win_dir);
    let ws = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), wt));
    let wu = cross(wt, ws);
    let wa = vec2<f32>(dot(dir, ws), dot(dir, wu)) / max(dot(dir, wt), 1e-3);
    let bars = max(1.0 - smoothstep(0.004, 0.008 + rough, abs(wa.x)), 1.0 - smoothstep(0.004, 0.008 + rough, abs(wa.y)));
    let window = pane * (1.0 - bars) * 14.0;
    // Walls lit dimly, a darker floor, a dim ceiling.
    let wall = 0.035 * (1.0 - abs(dir.y)) * smoothstep(-0.2, 0.6, dir.z + 0.3);
    let floor = 0.008 * smoothstep(0.0, -0.5, dir.y);
    let ceiling = 0.02 * smoothstep(0.3, 0.9, dir.y);
    return light_tint(0.15) * (key + frame * 0.15) + light_tint(0.6) * strip + light_tint(0.4) * window * vec3<f32>(0.85, 0.92, 1.05)
        + vec3<f32>(wall + floor + ceiling + 0.006);
}

// The light falling on a surface facing n, wrapped round its edge a
// little as light in translucent tissue is.
fn lit(n: vec3<f32>, wrap: f32) -> vec3<f32> {
    let k = max((dot(n, normalize(KEY)) + wrap) / (1.0 + wrap), 0.0);
    let s = max((dot(n, normalize(STRIP)) + wrap) / (1.0 + wrap), 0.0);
    return light_tint(0.15) * k * 1.35 + light_tint(0.6) * s * 0.3 + vec3<f32>(0.025);
}

fn fresnel(cos_t: f32, f0: f32) -> f32 {
    return f0 + (1.0 - f0) * pow(1.0 - clamp(cos_t, 0.0, 1.0), 5.0);
}

// ---------------------------------------------------------------- the white

// Blood vessels on the white, as curves: ten main conjunctival vessels
// come forward from the back toward the iris, each wandering, thinning as
// it comes and splitting in two partway; fine capillaries between them.
// `l` is the direction in the eye's frame (z toward the gaze), `pw` a
// pixel in eyeball radii.
fn vessel(phi: f32, z: f32, at: f32, wobble: f32, width: f32, start: f32, stop: f32) -> f32 {
    let ring = sqrt(max(1.0 - z * z, 0.0));
    var d = phi - at - wobble;
    d = d - TAU * round(d / TAU);
    let across = abs(d) * ring;
    // Thinner toward the iris, fading in and out at its ends.
    let w = width * (1.0 - 0.6 * smoothstep(start, stop, z));
    let along = smoothstep(start - 0.05, start + 0.05, z) * (1.0 - smoothstep(stop - 0.12, stop, z));
    return exp(-pow(across / w, 2.0)) * along;
}

fn vessels(l: vec3<f32>, pw: f32, k: u32) -> f32 {
    let phi = atan2(l.y, l.x);
    let z = l.z;
    var v = 0.0;
    let w0 = max(0.016, pw * 0.8);
    for (var m = 0u; m < 10u; m++) {
        let seed = f32(k) * 17.0 + f32(m) * 3.7;
        let at = (f32(m) + rand(m, seed)) / 10.0 * TAU;
        let wob = 0.16 * sin(z * 4.7 + seed * 1.3) + 0.08 * sin(z * 9.1 + seed * 2.9) + 0.04 * sin(z * 19.0 + seed);
        let reach = 0.74 + 0.12 * rand(m, seed + 1.0);
        let thick = w0 * (1.0 + 1.2 * rand(m, seed + 2.0));
        v = max(v, vessel(phi, z, at, wob, thick, -0.4, reach));
        // A branch splits off partway and wanders its own way.
        let split = 0.05 + 0.35 * rand(m, seed + 3.0);
        let side = select(-1.0, 1.0, rand(m, seed + 4.0) > 0.5);
        let bwob = wob + side * 0.5 * max(z - split, 0.0) + 0.05 * sin(z * 13.0 + seed * 3.7);
        v = max(v, vessel(phi, z, at, bwob, thick * 0.6, split, reach - 0.08) * 0.8);
    }
    // Capillaries: faint, fine, wandering, everywhere but nearest the iris.
    let r = 40.0 / TAU;
    let a = phi + 0.3 * vnoise3(l * 6.0 + f32(k));
    let n = vnoise3(vec3<f32>(cos(a) * r, sin(a) * r, z * 5.0) + vec3<f32>(f32(k) * 5.0, 0.0, 0.0));
    let fine = smoothstep(0.93, 1.0, 1.0 - abs(n)) * smoothstep(0.8, 0.4, z) * (1.0 - smoothstep(0.004, 0.012, pw));
    return max(v, fine * 0.45);
}

fn white_colour(f: Frame, k: u32, p: vec3<f32>, rd: vec3<f32>, pw: f32) -> vec3<f32> {
    let P = params();
    let n = normalize(p - f.c);
    let l = vec3<f32>(dot(n, f.x), dot(n, f.y), dot(n, f.g));
    // Ivory, pinker and duskier toward the back, a cool grey-blue where it
    // thins at the limbus.
    var base = vec3<f32>(0.76, 0.73, 0.69);
    base = mix(base, vec3<f32>(0.62, 0.42, 0.38), smoothstep(0.45, -0.2, l.z));
    base = mix(base, vec3<f32>(0.55, 0.57, 0.6), smoothstep(0.83, 0.873, l.z) * 0.5);
    if (layer_on(1u) && P.veins > 0.0) {
        let dense = smoothstep(0.85, 0.15, l.z) * P.veins;
        let v = vessels(l, pw / f.r, k) * P.veins * 1.4;
        base = mix(base, vec3<f32>(0.5, 0.05, 0.04), clamp(v * 1.2, 0.0, 1.0));
        // And a general flush of fine capillaries.
        base = mix(base, vec3<f32>(0.7, 0.32, 0.28), dense * 0.25);
    }
    var col = vec3<f32>(0.0);
    if (layer_on(0u)) {
        col = base * lit(n, 0.2);
        // Light through the tissue reddens where it turns from the light.
        col += base * vec3<f32>(0.12, 0.025, 0.02) * smoothstep(0.3, -0.2, dot(n, normalize(KEY)));
    }
    // The tear film: a soft mirror.
    let fr = fresnel(dot(-rd, n), 0.022) * P.wet;
    col += room(reflect(rd, n), 0.035) * fr * 0.5;
    return col;
}

// ---------------------------------------------------------------- the cornea

fn cornea_colour(f: Frame, k: u32, p: vec3<f32>, rd: vec3<f32>, pw: f32) -> vec3<f32> {
    let P = params();
    let cc = f.c + f.g * (CORNEA_C * f.r);
    let n = normalize(p - cc);
    let fr = fresnel(dot(-rd, n), 0.025);
    // Through the cornea onto the iris.
    let t = refract(rd, n, 1.0 / CORNEA_N);
    let centre = f.c + f.g * (IRIS_Z * f.r);
    let along = dot(t, f.g);
    var through = vec3<f32>(0.0);
    if (along < -1e-4) {
        let q = p + t * (dot(centre - p, f.g) / along);
        let rv = q - centre;
        let rad = length(rv) / f.r;
        let ang = atan2(dot(rv, f.y), dot(rv, f.x));
        // Light reaching the iris has come through the cornea too, which
        // gathers it on the side away from the light.
        let away = max(-dot(normalize(rv + f.g * 1e-4), normalize(KEY)), 0.0);
        let light = lit(f.g, 0.0) * (0.75 + 0.6 * away * smoothstep(0.15, 0.45, rad));
        through = iris_colour(k, rad, ang, pw / (f.r * max(rad, 0.05)), pw / f.r) * light;
    }
    // Near its edge the cornea is clouded by the limbus, grading into the
    // white.
    let edge = length(p - f.c - f.g * dot(p - f.c, f.g)) / f.r;
    let limb = smoothstep(LIMBUS_R - 0.022, LIMBUS_R + 0.004, edge);
    var col = through * (1.0 - fr);
    if (limb > 0.0) {
        col = mix(col, white_colour(f, k, p, rd, pw), limb * 0.85);
    }
    if (layer_on(3u)) {
        col += room(reflect(rd, n), 0.0) * fr * (0.6 + 0.4 * P.wet) * (1.0 - limb * 0.85);
    }
    return col;
}
