// Ripples: a flowing noise-displaced plane seen from low over it, printed
// with contour lines, dots or a grid, darkening with distance. Beats drop
// rings into it at seeded places (stronger on the bar's first beat, a big
// one in the middle on a drop); the bass swells the hills; each section of
// the song has its own colours.
//
// After the Ripples in Felix Turner's Uberviz (https://uberviz.io). No code
// used; written fresh from a description of the effect.
//
// The plane is a 160 x 160 grid of cells, two triangles each, drawn far row
// first so near hills cover far ones (alpha 1). paint() prints each pixel
// from the height and ground position the triangles carry.
//
// Prepare items:
//   0      (flow, slewed swell, last beat pulse, rings dropped so far)
//   1      (next ring slot, drops so far, the section's colour offset (eased), 0)
//   2..5   rings: (x, z, birth time, strength)

const GRID: u32 = 160u;
const Z_NEAR: f32 = 0.15;
const Z_FAR: f32 = 9.0;
const EYE_H: f32 = 0.9;
const FOCAL: f32 = 1.732;   // 60 degree vertical field of view

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let last = item(0u);
    let dropped = drop_count() > item(1u).y && drop_sure() > 0.3;
    let struck = (V.beat > last.z + 0.4 || dropped) && P.rings > 0.0;
    if (i == 0u) {
        let flow = last.x + V.dt * P.speed * (0.5 + 0.5 * V.energy);
        // The bass swells the hills, slewed so it rolls rather than jumps.
        let want = 1.0 + P.react * 0.6 * clamp(V.bass - 1.0, -0.6, 1.5);
        let prev = select(last.y, 1.0, last.y <= 0.0);
        let swell = prev + (want - prev) * (1.0 - exp(-V.dt / 0.2));
        return vec4<f32>(flow, swell, V.beat, last.w + select(0.0, 1.0, struck));
    }
    let slot = u32(item(1u).x);
    if (i == 1u) {
        let sec = ease(item(1u).z, section_seed(5.0) * 0.5 * P.sections, 3.0, 3.0);
        return vec4<f32>(f32((slot + select(0u, 1u, struck)) % 4u), drop_count(), sec, 0.0);
    }
    // A beat drops a ring at a seeded place in the near half of the plane.
    if (struck && i - 2u == slot) {
        let seed = vec2<f32>(V.track * 997.0, last.w + 1.0);
        let x = select((hash21(seed) * 2.0 - 1.0) * 1.2, 0.0, dropped);
        let z = select(0.6 + hash21(seed + 17.3) * 1.8, 1.2, dropped);
        let first = beat_in_bar() < 0.5 && bar_sure() > 0.4;
        let strength = select(select(1.0, 1.4, first) * (0.6 + 0.4 * min(V.beat, 1.0)), 2.0, dropped);
        return vec4<f32>(x, z, V.time, P.rings * strength);
    }
    return item(i);
}

fn lattice(i: vec2<i32>) -> f32 {
    var n = u32(i.x) * 374761393u + u32(i.y) * 668265263u;
    n = (n ^ (n >> 13u)) * 1274126177u;
    n = n ^ (n >> 16u);
    return f32(n & 0xffffu) / 65535.0;
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = lattice(i);
    let b = lattice(i + vec2<i32>(1, 0));
    let c = lattice(i + vec2<i32>(0, 1));
    let d = lattice(i + vec2<i32>(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn height_scale() -> f32 {
    return params().height * item(0u).y;
}

fn surface(x: f32, z: f32) -> f32 {
    let P = params();
    let q = vec2<f32>(x, z + item(0u).x) * max(P.scale, 0.05);
    var h = (vnoise(q) * 0.65 + vnoise(q * 2.13 + vec2<f32>(7.3, 1.9)) * 0.35 - 0.5) * height_scale();
    for (var k = 0u; k < 4u; k++) {
        let ring = item(2u + k);
        let age = V.time - ring.z;
        if (ring.w > 0.0 && age >= 0.0 && age < 6.0) {
            let d = length(vec2<f32>(x, z) - ring.xy);
            let front = d - age * 0.9;
            let packet = exp(-front * front / 0.06);
            h += ring.w * 0.07 * sin(front * 16.0) * packet * exp(-age * 0.6);
        }
    }
    return h;
}

// Corner i: cell (i / 6), far row first, two triangles per cell.
fn vertex(i: u32) -> MeshVertex {
    let P = params();
    let q = i / 6u;
    let corner = i % 6u;
    let row = GRID - 1u - q / GRID;
    let col = q % GRID;
    var dx = 0u;
    var dz = 0u;
    switch (corner) {
        case 1u: { dx = 1u; }
        case 2u: { dz = 1u; }
        case 3u: { dz = 1u; }
        case 4u: { dx = 1u; }
        default: { if (corner == 5u) { dx = 1u; dz = 1u; } }
    }
    let s = f32(row + dz) / f32(GRID);
    let z = Z_NEAR + (Z_FAR - Z_NEAR) * s * s;   // cells bunch up near the eye
    let aspect = V.resolution.x / max(V.resolution.y, 1.0);
    let half_w = 0.3 + z * 1.2 * aspect / FOCAL;
    let x = (f32(col + dx) / f32(GRID) * 2.0 - 1.0) * half_w;
    let y = surface(x, z);
    let phi = mix(0.22, 1.3, clamp(P.tilt, 0.0, 1.0));
    let sp = sin(phi);
    let cp = cos(phi);
    let dy = y - EYE_H;
    let vz = max(-dy * sp + z * cp, 0.02);
    let vy = dy * cp + z * sp;
    // The compiled-in projection, in clip space, turned into centred units.
    let short = min(V.resolution.x, V.resolution.y);
    var v: MeshVertex;
    v.pos = vec4<f32>(x * FOCAL / aspect * V.resolution.x / short, vy * FOCAL * V.resolution.y / short, 0.0, vz);
    v.colour = vec4<f32>(0.0);
    v.data = vec4<f32>(y, x, z + item(0u).x, vz);
    return v;
}

fn ripple_colour(t: f32) -> vec3<f32> {
    let x = fract(t + V.palette_shift + item(1u).z) * 32.0;
    let i = u32(floor(x)) % 32u;
    let pal = mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
    return mix(V.tint.rgb, pal * 1.2, V.palette_mix);
}

// Each pixel of the plane: contours of the height, or dots or grid lines on
// the ground, a pixel-sized line width whatever the distance.
fn paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    let h = f.data.x;
    let ground = f.data.yz;
    let depth = f.data.w;
    let w = max(P.width, 0.3);
    let density = max(P.density, 0.5);
    let v = h * density * 3.0;
    let fv = max(fwidth(v), 1e-5);
    let dc = abs(fract(v + 0.5) - 0.5) / fv;
    let contour = (1.0 - smoothstep(w * 0.5 - 0.5, w * 0.5 + 0.5, dc)) * clamp(1.6 - fv * 2.5, 0.0, 1.0);
    let g = ground * density * 0.5;
    let fg = max(fwidth(g), vec2<f32>(1e-5));
    let cell = (fract(g) - 0.5) / fg;
    let dot_cov = (1.0 - smoothstep(w - 0.7, w + 0.7, length(cell))) * clamp(1.6 - max(fg.x, fg.y) * 6.0, 0.0, 1.0);
    let gl = abs(fract(g + 0.5) - 0.5) / fg;
    let grid_cov = (1.0 - smoothstep(w * 0.5 - 0.5, w * 0.5 + 0.5, min(gl.x, gl.y))) * clamp(1.6 - max(fg.x, fg.y) * 3.0, 0.0, 1.0);
    var cov = contour;
    if (P.print > 0.5 && P.print < 1.5) { cov = dot_cov; }
    if (P.print > 1.5) { cov = grid_cov; }
    let ht = clamp(0.5 + h / max(height_scale(), 0.05), 0.0, 1.0);
    let bright = exp(-depth * P.fade * 0.35) * (0.6 + 0.6 * ht);
    return vec4<f32>(ripple_colour(ht * 0.8) * cov * bright * 1.3, 1.0);
}
