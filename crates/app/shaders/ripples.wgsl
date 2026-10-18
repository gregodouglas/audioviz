// Ripples: a grid mesh made from vertex_index, displaced by flowing value
// noise plus rings dropped on beats, seen in perspective. One instance per
// row of cells, the far row first; the surface is opaque black, so a near
// hill covers what is behind it. The fragment prints contour lines of the
// height, dots or a grid on the ground, darker with distance.

@group(0) @binding(0) var<uniform> R: RipplesParams;

const Z_NEAR: f32 = 0.15;
const Z_FAR: f32 = 9.0;
const EYE_H: f32 = 0.9;
const FOCAL: f32 = 1.732;   // 60 degree vertical field of view

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) h: f32,
    @location(1) ground: vec2<f32>,
    @location(2) depth: f32,
};

// Value noise on an integer lattice (integer hash: identical at shared vertices).
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

fn surface(x: f32, z: f32) -> f32 {
    let q = vec2<f32>(x, z + R.flow) * R.scale;
    var h = (vnoise(q) * 0.65 + vnoise(q * 2.13 + vec2<f32>(7.3, 1.9)) * 0.35 - 0.5) * R.height;
    for (var k = 0; k < 4; k++) {
        let ring = R.rings[k];
        let age = R.time - ring.z;
        if (age >= 0.0 && age < 6.0) {
            let d = length(vec2<f32>(x, z) - ring.xy);
            let front = d - age * 0.9;
            let packet = exp(-front * front / 0.06);
            h += ring.w * 0.07 * sin(front * 16.0) * packet * exp(-age * 0.6);
        }
    }
    return h;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let n = R.grid;
    let row = n - 1u - ii;                  // far row first
    let col = vi / 6u;
    let corner = vi % 6u;
    var dx = 0u;
    var dz = 0u;
    switch (corner) {
        case 1u: { dx = 1u; }
        case 2u: { dz = 1u; }
        case 3u: { dz = 1u; }
        case 4u: { dx = 1u; }
        default: { if (corner == 5u) { dx = 1u; dz = 1u; } }
    }
    let s = f32(row + dz) / f32(n);
    let z = Z_NEAR + (Z_FAR - Z_NEAR) * s * s;   // cells bunch up near the eye
    let aspect = R.resolution.x / max(R.resolution.y, 1.0);
    let half_w = 0.3 + z * 1.2 * aspect / FOCAL;
    let x = (f32(col + dx) / f32(n) * 2.0 - 1.0) * half_w;
    let y = surface(x, z);
    // Camera at (0, EYE_H, 0), pitched down.
    let phi = mix(0.22, 1.3, R.tilt);
    let sp = sin(phi);
    let cp = cos(phi);
    let dy = y - EYE_H;
    let vz = max(-dy * sp + z * cp, 0.02);
    let vy = dy * cp + z * sp;
    var out: VsOut;
    out.pos = vec4<f32>(x * FOCAL / aspect, vy * FOCAL, 0.0, vz);
    out.h = y;
    out.ground = vec2<f32>(x, z + R.flow);
    out.depth = vz;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let w = max(R.width, 0.3);
    // Contours: lines of equal height.
    let v = in.h * R.density * 3.0;
    let fv = max(fwidth(v), 1e-5);
    let dc = abs(fract(v + 0.5) - 0.5) / fv;
    let contour = (1.0 - smoothstep(w * 0.5 - 0.5, w * 0.5 + 0.5, dc)) * clamp(1.6 - fv * 2.5, 0.0, 1.0);
    // Dots and grid ride on the ground coordinates.
    let g = in.ground * R.density * 0.5;
    let fg = max(fwidth(g), vec2<f32>(1e-5));
    let cell = (fract(g) - 0.5) / fg;
    let dot_cov = (1.0 - smoothstep(w - 0.7, w + 0.7, length(cell))) * clamp(1.6 - max(fg.x, fg.y) * 6.0, 0.0, 1.0);
    let gl = abs(fract(g + 0.5) - 0.5) / fg;
    let grid_cov = (1.0 - smoothstep(w * 0.5 - 0.5, w * 0.5 + 0.5, min(gl.x, gl.y))) * clamp(1.6 - max(fg.x, fg.y) * 3.0, 0.0, 1.0);
    var cov = contour;
    if (R.print == 1u) { cov = dot_cov; }
    if (R.print == 2u) { cov = grid_cov; }
    // Colour by height; darker with distance.
    let ht = clamp(0.5 + in.h / max(R.height, 0.05), 0.0, 1.0);
    let pal = ripples_pal(R, ht * 0.8 + R.palette_shift) * 1.2;
    let colour = mix(R.tint.rgb, pal, R.palette_mix);
    let bright = exp(-in.depth * R.fade * 0.35) * (0.6 + 0.6 * ht);
    return vec4<f32>(colour * cov * bright * 1.3, 1.0);
}
