// Frequency hallway: a corridor whose floor, ceiling and walls carry rows
// of glowing bars. Every Spacing of travel a row is born at the far end
// with the 32 bands of that moment; rows ride toward the eye (faster with
// the energy and on each beat) and are reborn behind it. On each surface
// 16 bars, mirrored about its middle (the lows there, the highs toward the
// corners), stand out from it toward the corridor's middle by their level.
// Each bar is a quad facing the eye, painted as a soft glowing bar.
//
// Items, every one working out the same clock from last frame's:
//   0                 (distance travelled, distance at the last birth, births, 0)
//   1                 (last beat pulse, beat kick, 1 once started, sway's sureness)
//   BANDS0 + 8s + g   row slot s's bands 4g .. 4g+3, as born
//   META0 + s         row slot s: (distance at its birth, 0, 0, 0)
// Triangles: row r's bar b (64 a row: 16 a surface) is quad r * 64 + b.

const MAX_ROWS: u32 = 48u;
const BANDS0: u32 = 2u;
const META0: u32 = 386u;
const Z_NEAR: f32 = 0.3;
const FOCAL: f32 = 0.8;

fn row_count() -> u32 {
    return clamp(u32(params().rows), 8u, MAX_ROWS);
}

fn far() -> f32 {
    return Z_NEAR + f32(row_count()) * params().spacing;
}

struct Clock {
    d: f32,
    last: f32,
    births: f32,
    reborn: u32,
    kick: f32,
};

fn clock() -> Clock {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let c0 = item(0u);
    let c1 = item(1u);
    var k: Clock;
    k.kick = c1.y * exp(-dt / 0.35);
    if (V.beat > c1.x + 0.4) {
        k.kick = 1.0;
    }
    let speed = P.speed * 0.8 * (0.5 + 0.5 * V.energy) * (1.0 + 1.5 * P.kick * k.kick);
    k.d = c0.x + speed * dt;
    k.last = c0.y;
    k.births = c0.z;
    k.reborn = MAX_ROWS;
    if (k.d - k.last >= P.spacing) {
        k.reborn = u32(k.births) % row_count();
        k.births = k.births + 1.0;
        k.last = k.d;
    }
    return k;
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let k = clock();
    let started = item(1u).z > 0.5;
    if (i == 0u) {
        return vec4<f32>(k.d, k.last, k.births % 65536.0, 0.0);
    }
    if (i == 1u) {
        return vec4<f32>(V.beat, k.kick, 1.0, ease(item(1u).w, V.confidence, 0.5, 1.0));
    }
    if (i >= META0) {
        let s = i - META0;
        if (!started) {
            return vec4<f32>(-f32(row_count() - 1u - min(s, row_count() - 1u)) * P.spacing, 0.0, 0.0, 0.0);
        }
        return select(item(i), vec4<f32>(k.d, 0.0, 0.0, 0.0), s == k.reborn);
    }
    let s = (i - BANDS0) / 8u;
    let g = (i - BANDS0) % 8u;
    if (!started) {
        return vec4<f32>(0.0);
    }
    if (s == k.reborn) {
        return vec4<f32>(band(4u * g), band(4u * g + 1u), band(4u * g + 2u), band(4u * g + 3u));
    }
    return item(i);
}

// Row slot s's band at t 0..1 (low to high), interpolated.
fn row_band(s: u32, t: f32) -> f32 {
    let x = clamp(t, 0.0, 1.0) * 31.0;
    let a = u32(floor(x));
    let b = min(a + 1u, 31u);
    let va = item(BANDS0 + s * 8u + a / 4u)[a % 4u];
    let vb = item(BANDS0 + s * 8u + b / 4u)[b % 4u];
    return mix(va, vb, fract(x));
}

fn surface_on(surface: u32) -> bool {
    let P = params();
    if (P.surfaces > 1.5) {
        return surface >= 2u;
    }
    if (P.surfaces > 0.5) {
        return surface < 2u;
    }
    return true;
}

fn vertex(i: u32) -> MeshVertex {
    let P = params();
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    v.colour = vec4<f32>(0.0);
    v.data = vec4<f32>(0.0);
    let quad = i / 6u;
    let corner = i % 6u;
    let r = quad / 64u;
    let bar = quad % 64u;
    let surface = bar / 16u;
    let k = bar % 16u;
    if (r >= row_count() || !surface_on(surface)) {
        return v;
    }
    let d = item(0u).x;
    let z = far() - (d - item(META0 + r).x);
    if (z <= Z_NEAR) {
        return v;
    }
    // Mirrored about the surface's middle: 8 a side, the lows in the middle.
    let side = select(-1.0, 1.0, k >= 8u);
    let kk = k % 8u;
    let t = (f32(kk) + 0.5) / 8.0;
    let level = row_band(r, t);
    let half_w = P.aspect;
    let half_h = 1.0;
    // Along the surface (u) and out from it (n), in corridor units.
    var along: vec2<f32>;
    var out_n: vec2<f32>;
    var at: vec2<f32>;
    var span: f32;
    switch (surface) {
        case 0u: { at = vec2<f32>(0.0, -half_h); along = vec2<f32>(1.0, 0.0); out_n = vec2<f32>(0.0, 1.0); span = half_w; }
        case 1u: { at = vec2<f32>(0.0, half_h); along = vec2<f32>(1.0, 0.0); out_n = vec2<f32>(0.0, -1.0); span = half_w; }
        case 2u: { at = vec2<f32>(-half_w, 0.0); along = vec2<f32>(0.0, 1.0); out_n = vec2<f32>(1.0, 0.0); span = half_h; }
        default: { at = vec2<f32>(half_w, 0.0); along = vec2<f32>(0.0, 1.0); out_n = vec2<f32>(-1.0, 0.0); span = half_h; }
    }
    let cell = span / 8.0;
    let centre = at + along * side * (f32(kk) + 0.5) * cell;
    let hw = 0.5 * cell * P.thickness;
    let reach = max(level, 0.02) * P.reach * min(half_w, half_h);
    // The quad's corners: across (-1, 1) and out (0, 1).
    var c = vec2<f32>(-1.0, 0.0);
    switch (corner) {
        case 0u: { c = vec2<f32>(-1.0, 0.0); }
        case 1u: { c = vec2<f32>(1.0, 0.0); }
        case 2u: { c = vec2<f32>(1.0, 1.0); }
        case 3u: { c = vec2<f32>(-1.0, 0.0); }
        case 4u: { c = vec2<f32>(1.0, 1.0); }
        default: { c = vec2<f32>(-1.0, 1.0); }
    }
    // A little wider than the bar, so paint() has room for its glow.
    let pad = 1.6;
    let world = centre + along * c.x * hw * pad + out_n * (c.y * reach * 1.15 - 0.05 * reach);
    // The view sways once a bar.
    let sway = P.sway * 0.12 * sin(TAU * bar_phase()) * item(1u).w;
    let cam = vec2<f32>(sway, 0.15 * sway);
    let screen = (world - cam) * FOCAL / z;
    v.pos = vec4<f32>(screen, 0.0, 1.0);
    var col = colour(t);
    if (P.colouring > 1.5) {
        col = colour(0.15 + 0.8 * level);
    } else if (P.colouring > 0.5) {
        col = colour(fract((item(META0 + r).x) * 0.15));
    }
    let fog = exp(-P.fog * (z - Z_NEAR) * 0.35);
    let birth = smoothstep(far(), far() - 2.0 * P.spacing, z);
    // Near rows fade out before they fill the view.
    let near = smoothstep(Z_NEAR + 0.2, Z_NEAR + 1.4, z);
    v.colour = vec4<f32>(col * (0.5 + 1.1 * level) * fog * birth * near, 0.0);
    // For paint: across in bar widths, out in bar lengths.
    v.data = vec4<f32>(c.x * pad, c.y * 1.15 - 0.05, 0.0, 0.0);
    return v;
}

// A bar with a soft edge and a glow round it, ending at zero.
fn paint(f: MeshFragment) -> vec4<f32> {
    let q = f.data.xy;
    let dx = max(abs(q.x) - 1.0, 0.0);
    let dy = max(max(-q.y, q.y - 1.0), 0.0);
    let inside = select(0.0, 1.0, abs(q.x) <= 1.0 && q.y >= 0.0 && q.y <= 1.0);
    let edge = fwidth(q.x) * 1.5;
    let core = inside * clamp((1.0 - abs(q.x)) / max(edge, 1e-4), 0.0, 1.0);
    let glow = max(exp(-(dx * 4.0 + dy * 8.0)) - 0.05, 0.0) * 0.2 * (1.0 - inside);
    return vec4<f32>(f.colour.rgb * (core + glow), 0.0);
}
