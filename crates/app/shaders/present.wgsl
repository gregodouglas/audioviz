// Final composite: HDR color + bloom, auto/manual exposure, AgX tonemap,
// vignette, grain, dither. Output is linear; the sRGB swapchain encodes it.

@group(0) @binding(0) var hdr_tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> P: PostParams;
@group(0) @binding(3) var bloom_tex: texture_2d<f32>;
@group(0) @binding(4) var expo_tex: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

// --- AgX (Troy Sobotka), minimal fit as used in Blender ---------------------
fn agx_contrast(x: vec3<f32>) -> vec3<f32> {
    let x2 = x * x;
    let x4 = x2 * x2;
    return 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232;
}

fn agx(c: vec3<f32>) -> vec3<f32> {
    let m = mat3x3<f32>(
        vec3<f32>(0.842479062253094, 0.0423282422610123, 0.0423756549057051),
        vec3<f32>(0.0784335999999992, 0.878468636469772, 0.0784336),
        vec3<f32>(0.0792237451477643, 0.0791661274605434, 0.879142973793104),
    );
    let min_ev = -12.47393;
    let max_ev = 4.026069;
    var v = m * max(c, vec3<f32>(1e-10));
    v = clamp(log2(v), vec3<f32>(min_ev), vec3<f32>(max_ev));
    v = (v - min_ev) / (max_ev - min_ev);
    return agx_contrast(v);
}

fn agx_eotf(c: vec3<f32>) -> vec3<f32> {
    let inv = mat3x3<f32>(
        vec3<f32>(1.19687900512017, -0.0528968517574562, -0.0529716355144438),
        vec3<f32>(-0.0980208811401368, 1.15190312990417, -0.0980434501171241),
        vec3<f32>(-0.0990297440797205, -0.0989611768448433, 1.15107367264116),
    );
    let v = max(inv * c, vec3<f32>(0.0));
    return pow(v, vec3<f32>(2.2));
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let hdr = textureSample(hdr_tex, samp, in.uv).rgb;
    // The up-chain accumulates levels with 0.6x per step. Normalize by the
    // level count so the tight half-res glow matches the original slider feel
    // and the wide levels stay subdued.
    let bloom = textureSample(bloom_tex, samp, in.uv).rgb / 6.0;
    // expo.wgsl state: .r = auto exposure gain, .b = reduce-flashing gain
    // (exactly 1.0 when flags bit1 is off, so that path is unchanged). Both
    // scale the scene-referred colour before the tonemapper, so a capped rise
    // here is a capped rise on screen.
    let expo = textureSampleLevel(expo_tex, samp, vec2<f32>(0.5), 0.0);
    var col = (hdr + bloom * P.bloom) * P.exposure * expo.r * expo.b;
    let lum = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
    col = max(mix(vec3<f32>(lum), col, P.saturation), vec3<f32>(0.0));
    if (P.look == 1u) {
        // Neon: compress brightness through the same curve but on the brightest
        // channel only, so hue and saturation survive; only very hot cores
        // whiten, the way a tube's centre does.
        let m = max(col.r, max(col.g, col.b));
        let t = agx_eotf(agx(vec3<f32>(m))).x;
        let kept = col * (t / max(m, 1e-5));
        col = mix(kept, vec3<f32>(t), 0.7 * smoothstep(6.0, 40.0, m));
    } else {
        col = agx_eotf(agx(col));
    }

    let d = in.uv - 0.5;
    col *= 1.0 - 0.35 * dot(d, d) * 2.0;

    let n_grain = ign(in.pos.xy + vec2<f32>(fract(P.time * 7.31) * 1000.0, fract(P.time * 3.17) * 1000.0)) - 0.5;
    // Grain scales with brightness so pure black stays still (no background shimmer).
    col += n_grain * P.grain * 0.12 * col;
    col += (ign(in.pos.xy) - 0.5) * (1.0 / 255.0);

    return vec4<f32>(max(col, vec3<f32>(0.0)), 1.0);
}
