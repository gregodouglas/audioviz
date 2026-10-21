// Spectrum wheel: a disc of spectrum wedges, each wedge's brightness its
// band's level raised to a power, colours running round the palette; outside
// the disc the wedges carry on as fading rays. It turns slowly, may jump on a
// beat, and its band mapping can walk one wedge per beat.
// After the Wheel in Felix Turner's Uberviz (Lantern); written fresh.
//
// prepare() keeps what the compiled-in Wheel kept on the CPU:
//   item 0      (spin 0..1, band step, last beat pulse, 0)
//   item 1 + w  wedge w: (colour times brightness, smoothed level)
// so shade() only finds its wedge and reads it.

fn wedges() -> u32 {
    return u32(clamp(params().count, 4.0, 64.0));
}

// The palette at t without the tint, as the compiled-in Wheel mixed it.
fn wheel_colour(t: f32) -> vec3<f32> {
    let x = fract(t + V.palette_shift) * 32.0;
    let i = u32(floor(x)) % 32u;
    let pal = mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(x));
    return mix(V.tint.rgb, pal * 1.2, V.palette_mix);
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let n = wedges();
    let last = item(0u);
    // A beat: the pulse jumped since last frame.
    let struck = V.beat > last.z + 0.4 && P.react > 0.0;
    if (i == 0u) {
        var spin = last.x;
        var step = last.y;
        if (struck) {
            if (hash21(vec2<f32>(V.beat_count, 7.13)) < P.jump) {
                spin = hash21(vec2<f32>(V.beat_count, 3.71));
            }
            if (P.walk > 0.5) {
                step = f32((u32(step) + 1u) % n);
            }
        }
        return vec4<f32>(fract(spin - V.dt * P.speed * 0.05), step, V.beat, 0.0);
    }
    let w = i - 1u;
    if (w >= n) {
        return vec4<f32>(0.0);
    }
    // The band this wedge shows (walking with the step), resampled from the
    // 32 raw bands (or their levels in balance), lit further by its attack;
    // a fast attack and slow release so the wheel breathes.
    let src = (w + u32(last.y)) % n;
    let x = f32(src) / f32(n) * 32.0;
    let k = u32(floor(x));
    let each = mix(band_raw(min(k, 31u)), band_raw(min(k + 1u, 31u)), fract(x));
    let t = min(x, 31.0) / 31.0;
    let v = max(select(each, level_at(t), P.spectrum > 0.5) + P.punch * attack_at(t), 0.0) * P.gain;
    let cur = item(i).w;
    let rate = select(1.0 - exp(-V.dt / 0.25), 1.0 - exp(-V.dt / 0.03), v > cur);
    let lvl = cur + (v - cur) * rate;
    let bright = pow(clamp(lvl, 0.0, 1.5), max(P.curve, 0.1));
    return vec4<f32>(wheel_colour(f32(w) / f32(n) * P.range) * bright, lvl);
}

fn wedge(i: u32) -> vec3<f32> {
    return item(1u + i).rgb;
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let half = min(V.resolution.x, V.resolution.y) * 0.5;
    let p = (uv - 0.5) * V.resolution / half;
    let r = length(p);
    let nu = wedges();
    let n = f32(nu);
    let ang = atan2(p.y, p.x) / TAU + 0.5 + item(0u).x;
    let w = fract(ang) * n;
    let i = u32(floor(w)) % nu;
    let f = fract(w);
    // Soft edge between a wedge and its neighbours.
    let soft = max(P.soft, 1e-3);
    var col = wedge(i);
    if (f < soft) {
        col = mix(wedge((i + nu - 1u) % nu), col, 0.5 + 0.5 * f / soft);
    } else if (f > 1.0 - soft) {
        col = mix(col, wedge((i + 1u) % nu), 0.5 * (f - (1.0 - soft)) / soft);
    }
    // The disc, a dark centre, and rays fading out beyond the rim.
    let rim = P.radius;
    var shape = 1.0 - smoothstep(rim - 0.01, rim + 0.01, r);
    if (P.rays > 0.0) {
        let beyond = max(r - rim, 0.0) / max(P.rays * 2.0, 1e-3);
        shape = max(shape, 0.55 * exp(-beyond * beyond * 2.0) * step(rim, r));
    }
    let hole = rim * P.hole;
    shape *= smoothstep(hole, hole + 0.02, r);
    return col * shape;
}
