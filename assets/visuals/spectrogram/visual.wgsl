// Spectrogram: the spectrum over time, scrolling. The trail keeps the
// history as loudness (in red), moved by whole pixels each frame (the
// fraction carried in item 0, so the history is never resampled and
// blurred); the newest columns are written from the fine spectrum as each
// hop found it. Colour is put on as it is drawn, so a palette change
// recolours the whole history.
//
// Item 0: (pixels owed to the scroll, pixels moved this frame, 0, 0).

fn scroll_pixels() -> f32 {
    // The scroll's length in pixels: the frame's width, or its height for a waterfall.
    return select(V.resolution.x, V.resolution.y, params().scroll > 0.5);
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let owed = item(0u).x + scroll_pixels() / max(P.seconds, 0.5) * clamp(V.dt, 0.0, 0.1);
    let moved = floor(owed);
    return vec4<f32>(owed - moved, moved, 0.0, 0.0);
}

// The raw spectrum at a frequency, interpolated between rows.
fn raw_at(hz: f32) -> f32 {
    let x = clamp(hz / SPECTRUM_HZ * f32(SPECTRUM_ROWS) - 0.5, 0.0, f32(SPECTRUM_ROWS - 1u));
    let i = u32(floor(x));
    return mix(spectrum_row_raw(i), spectrum_row_raw(min(i + 1u, SPECTRUM_ROWS - 1u)), fract(x));
}

// The frequency at t 0..1 up the frequency axis.
fn hz_at(t: f32) -> f32 {
    let P = params();
    if (P.axis > 0.5) {
        let lo = max(P.lowest, 20.0);
        let hi = max(P.highest, lo * 1.5);
        return lo * pow(hi / lo, t);
    }
    let hi = max(P.highest, P.lowest + 200.0);
    return mix(P.lowest, hi, t);
}

// Where a frequency sits up the axis, 0..1 (for the guides).
fn t_at(hz: f32) -> f32 {
    let P = params();
    if (P.axis > 0.5) {
        let lo = max(P.lowest, 20.0);
        let hi = max(P.highest, lo * 1.5);
        return log(hz / lo) / log(hi / lo);
    }
    let hi = max(P.highest, P.lowest + 200.0);
    return (hz - P.lowest) / (hi - P.lowest);
}

// Black through violet and orange to pale yellow.
fn heat(v: f32) -> vec3<f32> {
    let a = vec3<f32>(0.0);
    let b = vec3<f32>(0.35, 0.06, 0.45);
    let c = vec3<f32>(0.95, 0.35, 0.22);
    let d = vec3<f32>(1.0, 0.95, 0.7);
    if (v < 0.4) {
        return mix(a, b, v / 0.4);
    }
    if (v < 0.75) {
        return mix(b, c, (v - 0.4) / 0.35);
    }
    return mix(c, d, (v - 0.75) / 0.25);
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let moved = item(0u).y;
    let px = uv * V.resolution;
    // Along the scroll: s is pixels from the newest edge; f is 0..1 up the
    // frequency axis.
    var s: f32;
    var f: f32;
    var src: vec2<f32>;
    if (P.scroll < 0.5) {
        s = V.resolution.x - px.x;
        f = 1.0 - uv.y;
        src = uv + vec2<f32>(moved / V.resolution.x, 0.0);
    } else if (P.scroll < 1.5) {
        s = V.resolution.y - px.y;
        f = uv.x;
        src = uv + vec2<f32>(0.0, moved / V.resolution.y);
    } else {
        s = px.y;
        f = uv.x;
        src = uv - vec2<f32>(0.0, moved / V.resolution.y);
    }
    // The newest columns come from the spectrum, the rest from the history.
    var v: f32;
    if (s < moved) {
        v = raw_at(hz_at(f));
    } else {
        v = trail(src).r;
    }
    write_trail(vec3<f32>(v, 0.0, 0.0));
    let level = pow(clamp((v - P.floor) / max(1.0 - P.floor, 0.05) * P.gain, 0.0, 1.0), max(P.contrast, 0.1));
    var c: vec3<f32>;
    if (P.colours < 0.5) {
        c = colour(level) * level;
    } else if (P.colours < 1.5) {
        c = heat(level);
    } else {
        c = colour(0.5) * level;
    }
    // Faint lines at round frequencies.
    if (P.guides > 0.5) {
        let span = select(hz_at(1.0) - hz_at(0.0), 0.0, P.axis > 0.5);
        let step = select(select(select(1000.0, 2000.0, span > 9000.0), 5000.0, span > 20000.0), 0.0, P.axis > 0.5);
        let side = select(V.resolution.y, V.resolution.x, P.scroll > 0.5);
        var g = 0.0;
        for (var k = 1; k < 12; k++) {
            var hz: f32;
            if (P.axis > 0.5) {
                hz = pow(10.0, floor(f32(k - 1) / 3.0) + 2.0) * select(select(1.0, 2.0, (k - 1) % 3 == 1), 5.0, (k - 1) % 3 == 2);
            } else {
                hz = step * f32(k);
            }
            let d = abs(t_at(hz) - f) * side;
            g = max(g, clamp(1.0 - d, 0.0, 1.0));
        }
        c += colour(0.5) * 0.25 * g * steady();
    }
    return c;
}
