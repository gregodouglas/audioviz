// Scene 3: a star field and a faint geodesic grid seen through a
// gravitational lens. Bass sets the lens mass (Einstein radius); the spectrum
// perturbs the lens shape; onsets twinkle the stars.

@group(0) @binding(0) var unused_tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> P: StarParams;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

fn band(i: u32) -> f32 {
    let v = P.bands[i / 4u];
    switch (i % 4u) {
        case 0u: { return v.x; }
        case 1u: { return v.y; }
        case 2u: { return v.z; }
        default: { return v.w; }
    }
}

// One layer of stars on a hash grid in source-plane coordinates `p`.
// `sigma` is the star radius in the same units, so lensing stretches them
// into arcs near the ring.
fn stars(p: vec2<f32>, sigma: f32, seed: f32) -> vec3<f32> {
    let cell = floor(p);
    let f = fract(p);
    var acc = vec3<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let o = vec2<f32>(f32(x), f32(y));
            let c = cell + o;
            let h = hash22(c + seed);
            let present = hash21(c * 1.7 + seed + 2.0);
            if (present > P.density * 0.55) { continue; }
            let d = length(f - (o + h));
            let mag = pow(hash21(c + seed + 9.0), 6.0);           // few bright, many dim
            let temp = hash21(c * 0.37 + seed + 4.0);              // color temperature
            let col = mix(vec3<f32>(1.0, 0.72, 0.45), vec3<f32>(0.62, 0.78, 1.0), temp);
            let s = sigma * (0.7 + 1.3 * mag);
            acc += col * (0.08 + 1.4 * mag) * exp(-(d * d) / (2.0 * s * s));
        }
    }
    return acc;
}

fn grid_lines(p: vec2<f32>, spacing: f32) -> f32 {
    let g = p / spacing;
    let w = fwidth(g) * 1.2;
    let fx = abs(fract(g.x) - 0.5);
    let fy = abs(fract(g.y) - 0.5);
    let lx = 1.0 - smoothstep(0.5 - w.x * 1.5, 0.5 - w.x * 0.5, fx);
    let ly = 1.0 - smoothstep(0.5 - w.y * 1.5, 0.5 - w.y * 0.5, fy);
    return max(lx, ly);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let aspect = P.resolution.x / P.resolution.y;
    // Image plane, y up, half-height = 1.
    let th = (in.uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0);
    let r = length(th);
    let phi = atan2(th.y, th.x);

    // Lens shape: Einstein radius with low-order multipole wobble from the
    // spectrum (bands 2..9 -> m = 2..9), slowly rotating.
    var te = P.einstein;
    var wob = 0.0;
    for (var m = 2u; m < 10u; m++) {
        let a = band(m) * 0.015;
        wob += a * cos(f32(m) * phi + P.time * (0.11 + 0.03 * f32(m)));
    }
    te *= 1.0 + wob;

    // Point-mass lens: source position and magnification.
    let r2 = max(r * r, 1e-5);
    let beta = th - te * te * th / r2;
    let q = pow(te / max(r, 1e-3), 4.0);
    let mu = min(1.0 / max(abs(1.0 - q), 0.06), 14.0);

    // Source plane: slow drift and rotation so the sky is never static.
    let ang = P.time * 0.006;
    let rot = mat2x2<f32>(cos(ang), -sin(ang), sin(ang), cos(ang));
    let src = rot * beta + vec2<f32>(P.time * 0.004 * P.drift, P.time * 0.0015 * P.drift);

    var col = vec3<f32>(0.0);
    col += stars(src * 3.0, 0.012, 1.0) * 0.6;
    col += stars(src * 7.0 + 11.0, 0.02, 2.0) * 0.35;
    col += stars(src * 15.0 + 23.0, 0.03, 3.0) * 0.12;
    col *= 1.0 + 0.5 * P.onset * P.twinkle;

    // Geodesic grid: thin lines in the source plane, so the lens bends them.
    // Coarse grid everywhere, fine grid only near the lens where bending shows.
    let near = exp(-max(r - te, 0.0) / (te * 1.5 + 0.05));
    let g = grid_lines(src, 0.25) * 0.5 + grid_lines(src, 0.0625) * 0.35 * near;
    col += P.tint.rgb * g * 0.004 * P.grid;

    col *= mu;
    // The shadow: nothing comes from inside the horizon.
    col *= smoothstep(te * 0.30, te * 0.55, r);
    // A faint photon-ring glow that breathes with the beat.
    let ring = exp(-pow((r - te) / (0.015 + 0.01 * P.beat), 2.0));
    col += P.tint.rgb * ring * (0.08 + 0.12 * P.beat);

    return vec4<f32>(col * 0.05, 1.0);
}
