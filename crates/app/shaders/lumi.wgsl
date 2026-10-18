// Modifier: luminescence. The picture excites two phosphor pools: a fast one
// that glows in the fresh colour and lets go quickly, and a slow one that
// charges up and gives a long afterglow in a hue-shifted colour (a dual-layer
// phosphor: blue flash, yellow persistence). Excitation also saturates the
// picture like fluorescence under UV, and transients make the charged areas
// sparkle like plankton.
// Two targets: (0) the picture out, (1) the pool state (fast, slow, cr, cb):
// the pools plus the remembered chroma of what excited them, read back next
// frame through the aux binding.
//   p0 = (fast tau, slow tau, fast gain, slow gain)
//   p1 = (fresh colour, excite)            or, in own-colour mode, (cos hue, sin hue, 0, excite)
//   p2 = (afterglow colour, sparkle)       or (0, 0, 0, sparkle)
//   p3 = (uv boost, onset, charge, own-colour flag)

struct LumiOut {
    @location(0) col: vec4<f32>,
    @location(1) state: vec4<f32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> M: ModParams;
@group(0) @binding(3) var prev: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

// Rebuild a colour of luminance `y` from stored chroma ratios (r/y - 1, b/y - 1).
fn from_chroma(y: f32, cr: f32, cb: f32) -> vec3<f32> {
    let r = y * (1.0 + cr);
    let b = y * (1.0 + cb);
    let g = max((y - LUMA.r * r - LUMA.b * b) / LUMA.g, 0.0);
    return max(vec3<f32>(r, g, b), vec3<f32>(0.0));
}

// Rotate hue by (cos, sin) in YIQ.
fn hue_turn(c: vec3<f32>, cs: vec2<f32>) -> vec3<f32> {
    let y = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    let i = dot(c, vec3<f32>(0.596, -0.274, -0.322));
    let q = dot(c, vec3<f32>(0.211, -0.523, 0.312));
    let i2 = i * cs.x - q * cs.y;
    let q2 = i * cs.y + q * cs.x;
    return max(vec3<f32>(y + 0.956 * i2 + 0.621 * q2, y - 0.272 * i2 - 0.647 * q2, y - 1.106 * i2 + 1.703 * q2), vec3<f32>(0.0));
}

@fragment
fn fs_main(in: FsIn) -> LumiOut {
    let c = textureSample(src, samp, in.uv).rgb;
    let lum = max(c.r, max(c.g, c.b));
    let st = textureSampleLevel(prev, samp, in.uv, 0.0);
    let dt = M.dt;
    // Remembered chroma of what is exciting this pixel (ratios to luminance),
    // pulled toward the current colour where it is lit.
    let y = max(dot(c, LUMA), 1e-5);
    let cur_cr = clamp(c.r / y - 1.0, -1.0, 6.0);
    let cur_cb = clamp(c.b / y - 1.0, -1.0, 6.0);
    let w = clamp(lum * 3.0, 0.0, 1.0) * (1.0 - exp(-dt / 0.12));
    let cr = mix(st.z, cur_cr, w);
    let cb = mix(st.w, cur_cb, w);
    // Fast pool holds the brightest recent excitation and lets go quickly.
    let fast = max(st.x * exp(-dt / max(M.p0.x, 0.02)), lum * (1.0 + M.p1.w * M.p3.y));
    // Slow pool charges with exposure and releases over seconds.
    let slow = st.y * exp(-dt / max(M.p0.y, 0.1)) + lum * dt * M.p3.z;
    // UV: saturate the picture where it is excited.
    let grey = vec3<f32>(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)));
    let uv_col = mix(grey, c, 1.0 + M.p3.x * smoothstep(0.0, 0.5, lum));
    // Sparkle: charged areas flicker with small soft points on transients.
    let px = in.uv * M.resolution;
    let cell = floor(px / 9.0);
    let slot = floor(M.time * 9.0);
    let h = hash21(cell * 0.173 + slot * 0.731);
    let jitter = hash22(cell * 0.311 + slot * 0.197) * 5.0 + 2.0;
    let d = length(px - (cell * 9.0 + jitter));
    let on = step(1.0 - 0.06 * M.p2.w * (0.3 + M.p3.y), h);
    let spark = on * exp(-d * d / 1.8) * slow * 10.0;
    var glow: vec3<f32>;
    if (M.p3.w > 0.5) {
        // Own colours: the fresh glow is the remembered colour, normalised so
        // its brightest channel is 1; the afterglow is that colour turned.
        var own = from_chroma(1.0, cr, cb);
        own = own / max(max(own.r, max(own.g, own.b)), 1e-4);
        let faded = hue_turn(own, M.p1.xy);
        glow = own * fast * M.p0.z + faded * (slow * M.p0.w + spark);
    } else {
        glow = M.p1.rgb * fast * M.p0.z + M.p2.rgb * (slow * M.p0.w + spark);
    }
    var out: LumiOut;
    out.col = vec4<f32>(max(uv_col, vec3<f32>(0.0)) + glow, 1.0);
    out.state = vec4<f32>(fast, slow, cr, cb);
    return out;
}
