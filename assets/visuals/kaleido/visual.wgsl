// Kaleidoscope: a drifting, turning window into a picture (the album art, or
// your photos as a slideshow with a crossfade). The layer's Kaleidoscope
// effect supplies the mirrors; without it this is a music-driven slideshow.
//
// The engine feeds the pictures ("pictures" in visual.json); each keeps its
// own window. Prepare items:
//   0   (slewed energy, onset kick, last onset, pictures seen)
//   1   the current picture's window: (angle, drift phase, turn direction, 0)
//   2   the previous picture's window, the same
// When a new picture arrives, the current window becomes the previous one
// and the new picture starts at the same angle, turning the other way.

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let last = item(0u);
    let started = last.w > 0.0 || V.picture_count > 0.0;
    let energy0 = select(1.0, last.x, last.x > 0.0);
    let energy = energy0 + (V.energy - energy0) * (1.0 - exp(-dt / 0.4));
    var kick = last.y;
    if (V.onset > last.z + 0.25) {
        kick += 0.9 * P.tumble;
    }
    kick *= exp(-dt / 0.3);
    if (i == 0u) {
        return vec4<f32>(energy, kick, V.onset, V.picture_count);
    }
    var cur = item(1u);
    var prv = item(2u);
    if (cur.z == 0.0) {
        cur.z = 1.0;
        prv.z = -1.0;
    }
    if (V.picture_count != last.w) {
        prv = cur;
        cur = vec4<f32>(cur.x, V.time * 0.1, -cur.z, 0.0);
    }
    let spin = dt * (0.03 + 0.10 * energy) * P.drift;
    let phase = dt * 0.12 * P.drift * (0.5 + 0.5 * energy);
    var w = select(prv, cur, i == 1u);
    w.x += w.z * (spin + kick * dt);
    w.y += phase;
    return w;
}

// Where screen point s looks in a picture of aspect ia through a window.
fn window(s: vec2<f32>, w: vec4<f32>, ia: f32, zoom: f32) -> vec2<f32> {
    // A window `zoom` of the picture's height tall, aspect-correct.
    let scale = vec2<f32>(zoom * 0.5 / max(ia, 0.01), zoom * 0.5);
    let room = max(1.0 - zoom, 0.0) * 0.5;
    let centre = vec2<f32>(0.5 + room * sin(w.y * 0.37) * 0.9, 0.5 + room * cos(w.y * 0.23 + 1.3) * 0.9);
    let c = cos(w.x);
    let sn = sin(w.x);
    let r = vec2<f32>(c * s.x - sn * s.y, sn * s.x + c * s.y);
    return centre + vec2<f32>(r.x, -r.y) * scale;
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let aspect = V.resolution.x / V.resolution.y;
    let s = (uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0);
    let zoom = clamp(P.zoom * (1.0 + 0.06 * clamp(V.bass - 1.0, -1.0, 1.5)), 0.1, 1.5);
    let now = picture(window(s, item(1u), V.picture_aspect, zoom));
    let before = picture_prev(window(s, item(2u), V.picture_prev_aspect, zoom));
    var col = mix(before, now, V.picture_fade);
    // Pull toward the tint when asked (own colour / album colour on a photo).
    let lum = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
    col = mix(col, V.tint.rgb * lum * 1.6, P.tint_mix);
    // Breathe with energy; a brief flash on onsets.
    return col * 0.6 * (0.75 + 0.25 * item(0u).x + 0.25 * V.onset * min(P.tumble, 1.0));
}
