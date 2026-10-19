// The tank behind the jellies (shade, per pixel): deep water when asked
// for, and marine snow, the drifting specks of the deep sea, lit by the
// jellies' glow (the glow itself is drawn round each jelly as a sprite, in
// mesh.wgsl). A share of the specks are living plankton that spark
// blue-green: on the hi-hats, and round a jelly that has just stroked (it
// stirs them, as a swimmer does in the sea).

// Distances from the eye of the four layers of snow, near to far.
fn snow_depth(l: u32) -> f32 {
    var z = array<f32, 4>(1.25, 2.3, 3.9, 6.6);
    return z[min(l, 3u)];
}

// Snow layer l this frame (items SNOW0 + l): where the camera's turn and
// its look point put the layer on screen (it is a grid fixed in the water,
// so near layers move faster), the size of its cells on screen, and how out
// of focus it is (sharp at the jellies' distance).
fn snow_step(i: u32) -> vec4<f32> {
    let l = i - SNOW0;
    let cam = the_camera();
    let z = snow_depth(l);
    let f = cam.focal;
    let beyond = (z - cam.dist) / z;
    let look = camera_dir(cam, cam.look).xy;
    var shift = vec2<f32>(-f * cam.yaw * beyond, -f * cam.pitch * beyond) - f * look / z;
    // Sinking slowly, swaying a little with the water.
    shift.y -= f * 0.018 * V.time / z;
    shift.x += f * 0.01 * sin(V.time * 0.05 + f32(l)) / z;
    // A cell is about a third of a unit of water across.
    let focus = abs(1.0 / z - 1.0 / max(cam.dist, 0.5));
    return vec4<f32>(shift, 0.36 * f / z, 0.22 * focus);
}

// The jellies' light at screen point p (rgb), and how much one that just
// stroked is stirring the water there (a).
fn jelly_light(p: vec2<f32>) -> vec4<f32> {
    var glow = vec3<f32>(0.0);
    var stir = 0.0;
    for (var j = 0u; j < jelly_count(); j++) {
        let a = item(HALO0 + 2u * j);
        let d = length(p - a.xy);
        // Ten sizes out its glow is under a hundredth: not worth the reads.
        if (a.w <= 0.0 || d > a.z * 10.0) {
            continue;
        }
        let b = item(HALO0 + 2u * j + 1u);
        glow += b.rgb * a.w * halo(d, a.z);
        stir += b.w * halo(d, a.z * 0.8);
    }
    return vec4<f32>(glow, stir);
}

// One layer of marine snow at screen point p: one speck per cell, a soft
// flake at its own angle, lit a little from above and more by a jelly near.
fn snow_layer(p: vec2<f32>, l: u32, natural: bool) -> vec3<f32> {
    let P = params();
    let at_l = item(SNOW0 + l);
    let q = (p - at_l.xy) / at_l.z;
    let base = floor(q);
    let h2 = hash22(base + f32(l) * 37.0 + 11.0);
    let e = q - (base + 0.25 + 0.5 * h2);
    let size = 0.045 * (0.5 + h2.x);
    let blur = size + at_l.w;
    // Most pixels are nowhere near their cell's speck.
    if (dot(e, e) > blur * blur * 9.0) {
        return vec3<f32>(0.0);
    }
    let h = hash22(base + f32(l) * 37.0);
    var d = rotate(e, h.x * 6.2831);
    d.x *= mix(1.0, 0.55, h.y * (size / blur));
    let flake = exp(-dot(d, d) / (blur * blur)) * (size * size) / (blur * blur);
    var pale = vec3<f32>(0.55, 0.7, 0.85);
    if (!natural) {
        pale = colour(h.y);
    }
    let lit = jelly_light(p);
    var c = (pale * (0.012 + 0.03 * h.y) + lit.rgb * P.halo * 0.5) * flake;
    // The living ones (one in eight, in the middle and far layers) spark as
    // sharp points: some on each high onset, and round a stroke.
    if (l > 0u && h2.y < 0.125) {
        let ck = item(0u);
        let pick = select(0.0, 1.0, hash21(base + vec2<f32>(ck.z * 1.37, f32(l))) < 0.25);
        let spark = exp(-ck.y / 0.35) * music() * pick + lit.a;
        var blue = vec3<f32>(0.25, 1.0, 0.8);
        if (!natural) {
            blue = colour(0.5);
        }
        c += blue * spark * exp(-dot(e, e) / (size * size * 0.5)) * 0.35;
    }
    return water(c, snow_depth(l), eye_near(), select(0.0, 1.0, natural));
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let p = centred(uv);
    let natural = natural_bodies() > 0.5;
    var c = vec3<f32>(0.0);
    if (P.water > 0.001) {
        let cam = the_camera();
        var deep = vec3<f32>(0.0, 0.012, 0.03);
        var near = vec3<f32>(0.0, 0.035, 0.065);
        if (!natural) {
            deep = colour(0.55) * 0.02;
            near = colour(0.55) * 0.045;
        }
        // Kept faint: water filling the frame would be lifted by the levelling.
        c += mix(deep, near, smoothstep(-1.0, 1.0, p.y)) * P.water * 0.12;
        let sx = p.x + p.y * 0.35 + cam.yaw * 0.8;
        let shafts = pow(noise(vec2<f32>(sx * 2.5, V.time * 0.04)), 5.0) * smoothstep(-0.3, 1.0, p.y);
        c += near * shafts * 0.3 * P.water;
    }
    if (P.plankton > 0.001) {
        var s = vec3<f32>(0.0);
        for (var l = 0u; l < 4u; l++) {
            s += snow_layer(p, l, natural);
        }
        c += s * P.plankton;
    }
    return shown(L_TANK, c);
}
