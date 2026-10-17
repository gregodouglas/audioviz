// Black hole: a Schwarzschild lens with a thin accretion disk seen almost
// edge-on, in front of a sharp point starfield. Rays are marched through the
// pseudo-Newtonian null-geodesic field (a = -1.5 rs h^2 x / r^5, exact for the
// photon orbit equation), so the far side of the disk folds over and under
// the shadow and piles up on the photon ring. Pixels whose impact parameter
// clears the disk take the weak-field deflection analytically instead.
//
// Units: rs = 1 at mass 1; the camera sits at distance 16 on a slow orbit.
// Audio: rs breathes with the bass (CPU), the disk's brightness and texture
// follow energy / onsets, stars twinkle on transients and on their band.

@group(0) @binding(0) var unused_tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> P: StarParams;

const PI: f32 = 3.14159265;
const CAM_DIST: f32 = 16.0;
const FOCAL: f32 = 1.75;
const R_IN: f32 = 3.0;      // ISCO, in rs
const R_OUT: f32 = 9.5;     // disk outer edge, in rs
const R_FAR: f32 = 17.0;    // outer reach of the infall streams, in rs
const STEPS: i32 = 120;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn band(i: u32) -> f32 {
    let v = P.bands[(i / 4u) & 3u];
    switch (i % 4u) {
        case 0u: { return v.x; }
        case 1u: { return v.y; }
        case 2u: { return v.z; }
        default: { return v.w; }
    }
}

// ---------------------------------------------------------------- noise

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    return 0.5 * vnoise(p) + 0.3 * vnoise(p * 2.1 + 5.0) + 0.2 * vnoise(p * 4.3 - 2.0);
}

// Value noise periodic in x with period `k` cells (x in cell units).
fn pnoise(p: vec2<f32>, k: f32) -> f32 {
    var i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let x0 = i.x - k * floor(i.x / k);
    let x1 = (x0 + 1.0) - k * floor((x0 + 1.0) / k);
    let a = hash21(vec2<f32>(x0, i.y));
    let b = hash21(vec2<f32>(x1, i.y));
    let c = hash21(vec2<f32>(x0, i.y + 1.0));
    let d = hash21(vec2<f32>(x1, i.y + 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// ---------------------------------------------------------------- palettes

// Blackbody-ish: 0 = deep red, 0.7 = amber, 1 = white, 1.5 = blue-white.
fn blackbody(t: f32) -> vec3<f32> {
    var c = mix(vec3<f32>(0.75, 0.08, 0.01), vec3<f32>(1.0, 0.42, 0.10), smoothstep(0.0, 0.35, t));
    c = mix(c, vec3<f32>(1.0, 0.76, 0.46), smoothstep(0.35, 0.7, t));
    c = mix(c, vec3<f32>(1.0, 0.95, 0.88), smoothstep(0.7, 1.0, t));
    c = mix(c, vec3<f32>(0.78, 0.87, 1.0), smoothstep(1.0, 1.6, t));
    return c;
}

// Tinted: dim tint at the cool end, tint toward white at the hot end.
fn tinted(t: f32) -> vec3<f32> {
    let base = P.tint.rgb;
    var c = mix(base * 0.35, base, smoothstep(0.0, 0.5, t));
    c = mix(c, mix(base, vec3<f32>(1.0), 0.75), smoothstep(0.5, 1.0, t));
    c = mix(c, vec3<f32>(1.0), smoothstep(1.0, 1.6, t));
    return c;
}

fn star_pal(t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    let f = fract(x);
    return mix(P.palette[i].rgb, P.palette[(i + 1u) % 32u].rgb, f);
}

fn disk_palette(t: f32) -> vec3<f32> {
    let own = mix(tinted(t), blackbody(t), P.tint.w);
    // Layer palette: temperature runs along it (cool outer edge -> hot inner).
    let p = star_pal(clamp(t / 1.6, 0.0, 0.97) * 0.97 + P.palette_shift) * (0.6 + 0.8 * clamp(t, 0.0, 1.5));
    return mix(own, p, P.palette_mix);
}

// ---------------------------------------------------------------- disk

// Emission and opacity of the disk at `hit` (world units, plane y = 0) for a
// ray travelling in direction `dir` (away from the camera).
fn disk_sample(hit: vec3<f32>, dir: vec3<f32>) -> vec4<f32> {
    let r = length(hit.xz) / P.rs;
    if (r < R_IN * 0.97 || r > R_FAR) { return vec4<f32>(0.0); }
    let phi = atan2(hit.z, hit.x);
    let lr = log(r);

    // Outside the disk: thin streams of matter spiralling in to feed it.
    if (r > R_OUT * 0.98) {
        // Two or three thin log-spiral arms, broken into wisps, fading fast with radius.
        let arms = pow(0.5 + 0.5 * sin(3.0 * phi + 6.5 * lr - P.phase * 0.7), 10.0);
        let wisp = pow(pnoise(vec2<f32>((phi / (2.0 * PI) + 0.25 * lr) * 18.0, lr * 7.0 + P.phase * 0.35), 18.0), 2.5);
        let streams = arms * wisp * pow(R_OUT / r, 3.0) * smoothstep(R_FAR, R_OUT * 1.25, r);
        let gain = P.streams * (0.5 + 0.5 * P.energy) * 0.4;
        let col = disk_palette(0.3) * streams * gain;
        return vec4<f32>(col, clamp(streams * 0.2, 0.0, 0.3));
    }

    // Keplerian shear plus an inward flow: inner material laps the outer and
    // everything drifts toward the hole along log spirals.
    let omega = pow(r, -1.5) * 5.2;
    let a = (phi + P.phase * omega + 0.9 * lr) / (2.0 * PI);
    let lrf = lr + P.phase * 0.14;
    let k1 = 24.0;
    let k2 = 48.0;
    var n = 0.0;
    n += 0.55 * pnoise(vec2<f32>(a * k1, lrf * 9.0 + 3.1), k1);
    n += 0.30 * pnoise(vec2<f32>(a * k2 + 7.0, lrf * 19.0 - 2.0), k2);
    n += 0.15 * pnoise(vec2<f32>(a * 96.0 + 2.0, lrf * 38.0 + 5.0), 96.0);
    // Contrast rises with turbulence (onsets); the field never goes fully dark.
    n = pow(clamp(n, 0.0, 1.0), 1.2 + 2.2 * P.turb);
    var tex = 0.18 + 1.6 * n;
    // Infall tendrils at the inner edge: bright filaments racing inward.
    let tend = pow(pnoise(vec2<f32>(a * 40.0 + lrf * 4.0, lrf * 26.0 + P.phase * 2.2), 40.0), 7.0);
    tex += tend * (3.0 + 6.0 * P.onset) * smoothstep(R_IN * 2.2, R_IN, r) * P.streams;

    // Spectrum rings: 16 bands laid out inner -> outer, gently.
    let bi = clamp((lr - log(R_IN)) / (log(R_OUT) - log(R_IN)) * 15.99, 0.0, 15.99);
    let b0 = u32(bi);
    let bf = fract(bi);
    let be = mix(band(b0), band(min(b0 + 1u, 15u)), bf);
    let ring = 1.0 + 0.35 * be;

    // Radial profile: bright just outside the ISCO, falling outward, soft edges.
    let edge_in = smoothstep(R_IN * 0.97, R_IN * 1.12, r);
    let edge_out = 1.0 - smoothstep(R_OUT * 0.62, R_OUT, r);
    let prof = pow(R_IN * 1.08 / r, 2.6) * edge_in * edge_out;

    // Temperature falls as r^-3/4; Doppler and gravitational shift move it.
    var t = pow(R_IN * 1.12 / r, 0.75);
    let v = sqrt(0.5 / r);                                  // orbital speed, c = 1
    let tang = normalize(vec3<f32>(hit.z, 0.0, -hit.x));    // prograde: left side approaches
    let n_obs = -dir;                                       // toward the observer
    let cosang = dot(tang, n_obs);
    let gamma = 1.0 / sqrt(max(1.0 - v * v, 1e-3));
    let dop = 1.0 / (gamma * max(1.0 - v * cosang, 0.05));
    let grav = sqrt(max(1.0 - 1.0 / r, 0.02));
    let shift = pow(dop * grav, P.doppler);
    t *= shift;
    let beam = pow(shift, 3.0);

    // The beat pulses the innermost orbits, where the photon-ring images pile up.
    let pulse = 1.0 + 0.5 * P.beat * (1.0 - smoothstep(R_IN, R_IN * 1.8, r));
    let bright = 1.6 * prof * tex * ring * beam * pulse * (0.55 + 0.45 * P.energy) * (1.0 + 0.6 * P.flare) * P.disk;
    let col = disk_palette(t) * bright;
    // Opaque in the dense inner streaks, wispy outward and in the gaps.
    let alpha = clamp(tex * 0.9 * edge_in * edge_out * (0.3 + 0.7 * pow(R_IN / r, 0.8)), 0.0, 0.92);
    return vec4<f32>(col, alpha);
}

// ---------------------------------------------------------------- stars

// Octahedral map of a direction to [-1,1]^2; seams sit behind the camera.
fn oct(d: vec3<f32>) -> vec2<f32> {
    let n = d / (abs(d.x) + abs(d.y) + abs(d.z));
    let s = vec2<f32>(select(-1.0, 1.0, n.x >= 0.0), select(-1.0, 1.0, n.y >= 0.0));
    return select(n.xy, (1.0 - abs(n.yx)) * s, n.z < 0.0);
}

// One hash-grid layer of point stars. `inv` maps sky offsets to pixel
// offsets, so a star is ~1 px wide whatever the lens does to its surroundings.
fn stars(sky: vec2<f32>, inv: mat2x2<f32>, scale: f32, present: f32, gain: f32, seed: f32, spikes: f32) -> vec3<f32> {
    let p = sky * scale;
    let cell = floor(p);
    let f = fract(p);
    var acc = vec3<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let o = vec2<f32>(f32(x), f32(y));
            let c = cell + o;
            if (hash21(c * 1.7 + seed + 2.0) > present) { continue; }
            let h = hash22(c + seed);
            let d_sky = (f - (o + h)) / scale;
            let e = inv * d_sky;                                   // pixel offset
            let d2 = dot(e, e);
            if (d2 > 64.0) { continue; }
            let mag = pow(hash21(c + seed + 9.0), 5.0);            // few bright, many dim
            let temp = hash21(c * 0.37 + seed + 4.0);
            let col = mix(vec3<f32>(1.0, 0.72, 0.45), vec3<f32>(0.68, 0.80, 1.0), temp);
            // Twinkle: a sparkle on transients, phased per star, plus a breath on the
            // star's own band so the field shimmers with the spectrum.
            let hb = hash21(c * 2.3 + seed + 1.0);
            let sparkle = 0.5 + 0.5 * sin(P.time * (14.0 + 22.0 * hb) + hb * 40.0);
            let bb = band(u32(hb * 15.99));
            // A slow shimmer on every star; only a few flash on transients.
            let shimmer = 0.88 + 0.12 * sin(P.time * (0.7 + 1.4 * hb) + hb * 31.0);
            let flash = step(0.82, hb) * sparkle * P.onset * 1.6 + 0.25 * bb * step(0.6, hb);
            let tw = shimmer * (1.0 + P.twinkle * flash);
            let amp = gain * (0.04 + 1.6 * mag) * tw;
            let sigma = 0.62;
            var s = exp(-d2 / (2.0 * sigma * sigma));
            // Bright stars: a faint halo and subtle four-point diffraction.
            if (mag > 0.25) {
                let b = (mag - 0.25) / 0.75;
                let len = 2.0 + 9.0 * b;
                let ax = abs(e.x);
                let ay = abs(e.y);
                let spike = exp(-ay * ay * 2.2) * exp(-ax / len) + exp(-ax * ax * 2.2) * exp(-ay / len);
                s += spikes * b * (0.10 * spike + 0.14 * exp(-sqrt(d2) * 0.55));
            }
            acc += col * amp * s;
        }
    }
    return acc;
}

// ---------------------------------------------------------------- lens

struct Ray {
    dir: vec3<f32>,      // outgoing direction if escaped
    disk: vec4<f32>,     // accumulated disk emission (rgb) and opacity (a)
    captured: f32,       // 1 = fell into the hole
    haze: f32,           // volumetric glow along the path near the plane
};

fn march(x0: vec3<f32>, v0: vec3<f32>) -> Ray {
    var ray: Ray;
    ray.dir = v0;
    ray.disk = vec4<f32>(0.0);
    ray.captured = 0.0;
    ray.haze = 0.0;
    let rs = P.rs;
    let hv = cross(x0, v0);
    let h2 = dot(hv, hv);
    let k = 1.5 * rs * h2;
    let exit_r = R_FAR * rs * 1.3;

    var x = x0;
    var v = v0;
    var done = false;
    var crossings = 0.0;
    for (var i = 0; i < STEPS; i++) {
        let r = length(x);
        if (r < rs * 1.02) { ray.captured = 1.0; done = true; break; }
        if (r > exit_r && dot(x, v) > 0.0) { break; }
        let dl = clamp(0.09 * r, 0.035 * rs, 0.7 * rs);
        // Midpoint step of the pseudo-Newtonian geodesic.
        let xm = x + v * (0.5 * dl);
        let rm = length(xm);
        let rm2 = rm * rm;
        let a = -k * xm / (rm2 * rm2 * rm);
        let vn = v + a * dl;
        let xn = x + vn * dl;
        // Disk plane crossing.
        if (x.y * xn.y < 0.0) {
            let t = x.y / (x.y - xn.y);
            let hit = mix(x, xn, t);
            let s = disk_sample(hit, normalize(vn));
            crossings += 1.0;
            // Later images of the disk (the photon-ring pile-up) count for less,
            // so the ring reads as a glow rather than a hard line.
            let cw = select(1.0, 1.0 / (1.0 + 0.9 * (crossings - 1.0)), crossings > 1.5);
            let w = (1.0 - ray.disk.a) * cw;
            ray.disk = vec4<f32>(ray.disk.rgb + s.rgb * w, ray.disk.a + s.a * w);
        }
        // A whisper of volumetric glow above and below the disk body.
        let rr = rm / rs;
        if (rr > R_IN * 0.9 && rr < R_OUT) {
            let hz = exp(-abs(xm.y) / (0.09 * rm)) * pow(R_IN / rr, 2.0) * (1.0 - smoothstep(R_OUT * 0.6, R_OUT, rr));
            ray.haze += hz * dl / rs;
        }
        x = xn;
        v = vn;
    }
    if (!done && length(x) < rs * 1.6) { ray.captured = 1.0; }
    var vo = normalize(v);
    if (ray.captured < 0.5) {
        // Weak-field residual from here to infinity, so the marched path lands
        // where the analytic one would: alpha_rem = (rs/b)(1 - sqrt(1 - b^2/r^2)).
        let r = length(x);
        let b = max(sqrt(h2), 1e-4);
        let q = clamp(1.0 - b * b / (r * r), 0.0, 1.0);
        let alpha = (rs / b) * (1.0 - sqrt(q));
        let perp = x - vo * dot(x, vo);
        let toward = -perp / max(length(perp), 1e-5);
        vo = normalize(vo * cos(alpha) + toward * sin(alpha));
    }
    ray.dir = vo;
    return ray;
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let res = P.resolution;
    let short = min(res.x, res.y);
    // Image plane: half-extent 1 along the short side so the disk fits portrait too.
    let s = (in.uv * 2.0 - 1.0) * vec2<f32>(res.x / short, -res.y / short);

    // Camera on a slow orbit, `tilt` above the disk plane, looking at the hole.
    let el = P.tilt;
    let az = P.az;
    let cam = CAM_DIST * vec3<f32>(sin(az) * cos(el), sin(el), cos(az) * cos(el));
    let fwd = normalize(-cam);
    let right = normalize(cross(fwd, vec3<f32>(0.0, 1.0, 0.0)));
    let up = cross(right, fwd);
    let dir = normalize(fwd * FOCAL + right * s.x + up * s.y);

    // Impact parameter decides the path: march near the disk, bend analytically beyond it.
    let hv = cross(cam, dir);
    let b = length(hv);
    var ray: Ray;
    if (b > R_FAR * P.rs * 1.05) {
        ray.disk = vec4<f32>(0.0);
        ray.captured = 0.0;
        ray.haze = 0.0;
        // Deflection to second order: 2 rs / b + 15 pi rs^2 / (16 b^2).
        let alpha = 2.0 * P.rs / b + 2.945 * P.rs * P.rs / (b * b);
        let toward = normalize(-(cam - dir * dot(cam, dir)));
        ray.dir = normalize(dir * cos(alpha) + toward * sin(alpha));
    } else {
        ray = march(cam, dir);
    }

    // Sky lookup. Derivatives are taken here, in uniform control flow, and
    // turned into a sky->pixel map so stars stay points under magnification.
    let sky = oct(ray.dir);
    let jx = dpdx(sky);
    let jy = dpdy(sky);
    let det = jx.x * jy.y - jx.y * jy.x;
    let stretch = max(length(jx), length(jy));
    var col = vec3<f32>(0.0);
    if (ray.captured < 0.5 && abs(det) > 1e-14) {
        let inv = mat2x2<f32>(vec2<f32>(jy.y, -jx.y), vec2<f32>(-jy.x, jx.x)) * (1.0 / det);
        let dens = clamp(P.density, 0.05, 3.0);
        // Fade the field where one pixel spans more than a cell (photon-ring smear).
        let fade1 = 1.0 - smoothstep(0.25 / 230.0, 0.9 / 230.0, stretch);
        let fade2 = 1.0 - smoothstep(0.25 / 75.0, 0.9 / 75.0, stretch);
        // The band: a great circle of denser faint stars and dust.
        let bn = normalize(vec3<f32>(0.35, 1.0, 0.25));
        let band_w = exp(-pow(dot(ray.dir, bn), 2.0) / 0.05);
        // Three depths with their own parallax against the camera's orbit.
        col += stars(sky + vec2<f32>(P.az * 0.02, 0.0), inv, 230.0, 0.42 * dens * (1.0 + 1.6 * band_w), 0.22, 1.0, 0.0) * fade1;
        col += stars(sky + vec2<f32>(P.az * 0.05, 0.0), inv, 75.0, 0.30 * dens, 0.8, 2.0, 1.0) * fade2;
        col += stars(sky + vec2<f32>(P.az * 0.09, 0.0), inv, 26.0, 0.12 * dens, 1.8, 3.0, 1.0) * fade2;
        // Nebula haze and the band's dust, faint, in the disk's cool colour.
        let neb = fbm(sky * 3.0 + vec2<f32>(7.0, 2.0) + P.az * 0.02);
        let dust = fbm(sky * 9.0 - 3.0);
        let fine = fbm(sky * 22.0 + 11.0);
        let haze = (0.5 * neb * neb * neb * (0.5 + fine) + 0.7 * band_w * dust * dust * fine) * P.nebula;
        col += mix(disk_palette(0.35), P.tint.rgb, 0.35) * haze * 0.07;
    }

    // Disk over the stars. A soft knee keeps the photon ring, where images of
    // the disk pile up, from reading as a hard white circle.
    let lum = max(ray.disk.r, max(ray.disk.g, ray.disk.b));
    let knee = 1.0 / (1.0 + 0.22 * lum);
    col = col * (1.0 - ray.disk.a) + ray.disk.rgb * knee;
    let haze_col = disk_palette(0.6) * ray.haze * 0.06 * P.disk * (0.6 + 0.4 * P.energy);
    col += haze_col;
    return vec4<f32>(col * 0.08, 1.0);
}
