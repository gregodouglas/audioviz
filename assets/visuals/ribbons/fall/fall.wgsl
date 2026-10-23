// Path Fall (after vizz.fm's Light Fall): long strands of light bound to
// one path in 3D. The path drops as a column from high above, bends round
// an arc of radius Bend and runs along a floor toward the camera and past
// it, fanning out as it goes. Each strand keeps its own place across the
// path (u sideways, v through its thickness), wanders off it by the curl
// noise (Meander) and slides along it; its tail and head fade. Specks of
// dust stream along the same path, quicker. The world: y up, the column at
// z = COLUMN_Z, the floor at y = 0 coming toward -z, where the camera is.
//
// Items: strand k is item k (head's place along the path, u, v, pitch class
// plus its seed as the fraction); FALL_SHARED holds the section's colour
// offset and the dust's clock.
//
// Light answers the music (each strand by its own note, onsets, a wave of
// light running down the path on the beat); movement as the layer's Motion
// says (the flow's rush, the fan breathing with the bass, both eased).

const STRANDS: u32 = 2048u;
const SEGMENTS: u32 = 64u;
const DUST: u32 = 2048u;
const FALL_SHARED: u32 = 2048u;
const TOP: f32 = 14.0;
const COLUMN_Z: f32 = 6.0;
const FLOOR_END: f32 = 6.0;
const NEAR: f32 = 0.08;

fn strand_count() -> u32 {
    return u32(clamp(params().strands, 16.0, f32(STRANDS)));
}

fn dust_count() -> u32 {
    return u32(clamp(params().dust, 0.0, 1.0) * f32(DUST));
}

fn bend() -> f32 {
    return clamp(params().bend, 0.2, 4.0);
}

// The column's length, the arc's, and the whole path's.
fn column_len() -> f32 {
    return TOP - bend();
}

fn arc_len() -> f32 {
    return 0.5 * 3.14159265 * bend();
}

fn path_len() -> f32 {
    return column_len() + arc_len() + COLUMN_Z - bend() + FLOOR_END;
}

// How far the flow runs in a second, in world units.
fn flow_speed() -> f32 {
    let P = params();
    return P.speed * 6.0 * (1.0 + motion() * (0.4 * (min(bass_soft(), 2.0) - 1.0) + 1.2 * onset_soft() + P.rush * build()));
}

// ---------------------------------------------------------------- the path

// A point of the path at s (its length from the top; past either end it
// carries straight on), for the strand at (u, v) across it.
fn path_point(s: f32, u: f32, v: f32, salt: f32) -> vec3<f32> {
    let P = params();
    let r = bend();
    let l1 = column_len();
    let l2 = arc_len();
    var centre: vec3<f32>;
    var normal: vec3<f32>;
    if (s < l1) {
        centre = vec3<f32>(0.0, TOP - s, COLUMN_Z);
        normal = vec3<f32>(0.0, 0.0, 1.0);
    } else if (s < l1 + l2) {
        let a = (s - l1) / r;
        centre = vec3<f32>(0.0, r - r * sin(a), COLUMN_Z - r + r * cos(a));
        normal = vec3<f32>(0.0, -sin(a), cos(a));
    } else {
        centre = vec3<f32>(0.0, 0.0, COLUMN_Z - r - (s - l1 - l2));
        normal = vec3<f32>(0.0, -1.0, 0.0);
    }
    // Past the column's foot the path fans out sideways and flattens onto
    // the floor (kept well under the camera's eye); the bass breathes the
    // fan (eased), as much as the layer's Motion says.
    let g = max(s - l1, 0.0);
    let fan = P.fan * (1.0 + 0.2 * motion() * clamp(bass_soft() - 1.0, -0.5, 1.0));
    let half_w = P.column + fan * g * 0.4;
    let thick = min(P.column, 0.5) * mix(1.0, 0.25, smoothstep(0.0, l2, g));
    var du = 0.0;
    var dv = 0.0;
    if (P.meander > 0.0) {
        // Curl-like noise on the path's own coordinates, so neighbours
        // wander together; it drifts slowly.
        let q = vec2<f32>(s * 0.6, u * 1.6 + salt);
        let t = V.time * 0.08;
        du = potential(q, t) * P.meander * (0.2 + 0.12 * half_w);
        dv = potential(q + vec2<f32>(23.0, 7.0), t) * P.meander * (0.12 + 0.1 * thick);
    }
    return centre + vec3<f32>(u * half_w + du, 0.0, 0.0) + normal * (v * thick + dv);
}

// ---------------------------------------------------------------- the camera

struct Eye {
    pos: vec3<f32>,
    right: vec3<f32>,
    up: vec3<f32>,
    fwd: vec3<f32>,
    focal: f32,
    focus: f32,
};

fn the_eye() -> Eye {
    let P = params();
    var pos: vec3<f32>;
    var look: vec3<f32>;
    var focal: f32;
    let r = bend();
    switch (u32(P.shot)) {
        // Side: the bend from the side, the column above and behind.
        case 1u: {
            pos = vec3<f32>(-7.5, 2.6, 1.0);
            look = vec3<f32>(0.0, 1.8, COLUMN_Z - r - 0.5);
            focal = 1.5;
        }
        // Inside: close beside the strands as they come round the bend.
        case 2u: {
            pos = vec3<f32>(-0.6, 1.3, COLUMN_Z - r - 0.6);
            look = vec3<f32>(0.5, r * 0.4, COLUMN_Z - r * 0.3);
            focal = 1.0;
        }
        // Front: low over the floor, facing the column.
        default: {
            pos = vec3<f32>(0.0, 0.7, -1.5);
            look = vec3<f32>(0.0, 0.9 + r * 0.15, COLUMN_Z - r);
            focal = 1.2;
        }
    }
    // A slow drift of the eye, never with the music.
    let t = V.time;
    pos += vec3<f32>(sin(t * 0.031) * 0.6, sin(t * 0.023 + 1.1) * 0.15, sin(t * 0.017 + 2.0) * 0.3) * P.drift;
    var e: Eye;
    e.pos = pos;
    e.fwd = normalize(look - pos);
    e.right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), e.fwd));
    e.up = cross(e.fwd, e.right);
    e.focal = focal;
    e.focus = length(look - pos);
    return e;
}

// A world point on screen: centred xy, and its depth.
fn on_screen(e: Eye, p: vec3<f32>) -> vec3<f32> {
    let d = p - e.pos;
    let z = dot(d, e.fwd);
    let xy = vec2<f32>(dot(d, e.right), dot(d, e.up)) * e.focal / max(z, NEAR);
    return vec3<f32>(xy, z);
}

// ---------------------------------------------------------------- strands

fn strand_pitch(w: f32) -> f32 {
    return floor(w);
}

fn strand_seed(w: f32) -> f32 {
    return (fract(w) - 0.001) / 0.998;
}

// A pitch class for a new strand, as likely as its note is loud now (with
// a floor, so silence still spreads them round the wheel).
fn pick_pitch(r: f32) -> f32 {
    var total = 0.0;
    for (var i = 0u; i < 12u; i++) {
        total += 0.15 + chroma(i);
    }
    var want = r * total;
    for (var i = 0u; i < 12u; i++) {
        want -= 0.15 + chroma(i);
        if (want <= 0.0) {
            return f32(i);
        }
    }
    return 11.0;
}

fn spawn(k: u32, t: f32, stagger: bool) -> vec4<f32> {
    let P = params();
    let r = hash22(vec2<f32>(f32(k) * 0.731 + 3.0, t + seed()));
    let r2 = hash22(vec2<f32>(f32(k) * 1.117 + 5.0, t * 0.37 + seed()));
    // Evenly over a disc across the path.
    let rad = sqrt(r.x);
    let ang = r.y * TAU;
    let reach = clamp(P.reach, 0.05, 1.0) * path_len();
    // The first strands are anywhere along the path, so they do not all
    // arrive together; the rest come in from above, a little apart.
    var head = -r2.x * 3.0;
    if (stagger) {
        head = r2.x * (path_len() + reach);
    }
    let pitch = pick_pitch(hash21(vec2<f32>(f32(k) * 0.37, 3.3 + t + seed())));
    return vec4<f32>(head, rad * cos(ang), rad * sin(ang), pitch + 0.001 + r2.y * 0.998);
}

fn prepare(i: u32) -> vec4<f32> {
    let P = params();
    let dt = min(V.dt, 0.05);
    if (i == FALL_SHARED) {
        let last = item(FALL_SHARED);
        let sec = ease(last.x, section_seed(4.0) * 0.5 * P.sections, 3.0, 3.0);
        return vec4<f32>(sec, fract((last.y + flow_speed() * 1.6 * dt) / 4096.0) * 4096.0, 0.0, 0.0);
    }
    if (i >= strand_count()) {
        return item(i);
    }
    var s = item(i);
    if (s.w <= 0.0) {
        return spawn(i, V.time, true);
    }
    s.x += flow_speed() * dt;
    let reach = clamp(P.reach, 0.05, 1.0) * path_len();
    if (s.x - reach > path_len()) {
        return spawn(i, V.time, false);
    }
    return s;
}

// The light a strand of this pitch class gives at s: its note, onsets, and
// a wave running down the path from the top on each beat.
fn strand_light(pitch: f32, s: f32) -> f32 {
    let P = params();
    let since = since_beat();
    let front = since * 9.0;
    let ripple = exp(-(s - front) * (s - front) / 1.6) * exp(-since * 1.1) * (1.0 - V.silent);
    return (0.55 + 0.7 * chroma(u32(pitch) % 12u)) * (1.0 + 0.35 * V.onset) + P.pulse * ripple * 1.5;
}

// One corner of a strip along a strand: the node at s_own, its side
// (-1 or 1), and the segment's other node at s_other (a segment with a
// node behind the camera is left out). `t` is the node's place along the
// strand (0 tail, 1 head), `w` its width in world units; nearer than
// `near` it fades out (close up a strand or a speck would only be a blur).
fn strip_corner(e: Eye, u: f32, v: f32, salt: f32, s_own: f32, s_other: f32, ds: f32, side: f32,
                fade: f32, w: f32, near: f32, colour: vec3<f32>) -> MeshVertex {
    let P = params();
    var out: MeshVertex;
    out.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    out.colour = vec4<f32>(0.0);
    out.data = vec4<f32>(0.0);
    let a = on_screen(e, path_point(s_own, u, v, salt));
    let b = on_screen(e, path_point(s_other, u, v, salt));
    if (a.z < NEAR || b.z < NEAR || fade <= 0.0) {
        return out;
    }
    let ahead = on_screen(e, path_point(s_own + ds, u, v, salt));
    let behind = on_screen(e, path_point(s_own - ds, u, v, salt));
    var tangent = ahead.xy - behind.xy;
    if (dot(tangent, tangent) < 1e-12) {
        tangent = vec2<f32>(0.0, 1.0);
    }
    tangent = normalize(tangent);
    let normal = vec2<f32>(-tangent.y, tangent.x);
    // Its width in pixels by perspective, then softened away from the focus
    // (depth of field), keeping its light: a blurred strand is wider and
    // dimmer. Thinner than a pixel, it dims rather than flickers.
    let short = min(V.resolution.x, V.resolution.y);
    let half_px = short * 0.5;
    let sharp = w * e.focal / a.z * half_px;
    let coc = P.focus * abs(1.0 - e.focus / a.z) * 6.0 * (short / 1440.0);
    let sigma = clamp(sqrt(sharp * sharp + coc * coc), 0.7, short * 0.04);
    let gain = pow(clamp(max(sharp, 0.15) / sigma, 0.0, 1.0), 0.6);
    // Far away it dims into the dark.
    let fog = exp(-max(a.z - e.focus * 1.2, 0.0) * 0.07) * smoothstep(near * 0.5, near, a.z);
    let hw = 3.0 * sigma / half_px;
    out.pos = vec4<f32>(a.xy + normal * side * hw, 0.0, 1.0);
    out.colour = vec4<f32>(colour * fade * gain * fog, 0.0);
    out.data = vec4<f32>(side, 0.0, 0.0, 0.0);
    return out;
}

fn vertex(i: u32) -> MeshVertex {
    let P = params();
    let e = the_eye();
    let tri = i / 3u;
    let corner = i % 3u;
    let which = tri % 2u;
    // Two triangles a quad: (A-, B-, A+) and (A+, B-, B+).
    let at_b = (which == 0u && corner == 1u) || (which == 1u && corner >= 1u);
    let side = select(-1.0, 1.0, (which == 0u && corner == 2u) || (which == 1u && corner != 1u));
    let sec = item(FALL_SHARED).x;
    let len = path_len();
    var none: MeshVertex;
    none.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    none.colour = vec4<f32>(0.0);
    none.data = vec4<f32>(0.0);

    let strand_tris = STRANDS * SEGMENTS * 2u;
    if (tri < strand_tris) {
        let k = tri / (SEGMENTS * 2u);
        if (k >= strand_count()) {
            return none;
        }
        let q = (tri % (SEGMENTS * 2u)) / 2u;
        let st = item(k);
        if (st.w <= 0.0) {
            return none;
        }
        let pitch = strand_pitch(st.w);
        let sd = strand_seed(st.w);
        let reach = clamp(P.reach, 0.05, 1.0) * len;
        let ds = reach / f32(SEGMENTS);
        let n_own = q + select(0u, 1u, at_b);
        let n_other = q + select(1u, 0u, at_b);
        let t = f32(n_own) / f32(SEGMENTS);
        let s_own = st.x - reach + reach * t;
        let s_other = st.x - reach + reach * f32(n_other) / f32(SEGMENTS);
        let fade = smoothstep(0.0, 0.4, t) * (1.0 - smoothstep(0.93, 1.0, t));
        let colour = stroke_colour(pitch, sec, (sd - 0.5) * 4.0, P.along * s_own / len) * strand_light(pitch, s_own);
        return strip_corner(e, st.y, st.z, sd * 7.0, s_own, s_other, ds * 0.5, side, fade,
                            0.012 * P.width * (0.7 + 0.6 * sd), 0.5, colour);
    }

    // Dust: short quick streaks, each one segment, on a clock of their own.
    let d = tri - strand_tris;
    let k = d / 2u;
    if (k >= dust_count()) {
        return none;
    }
    let h = hash22(vec2<f32>(f32(k) * 0.913 + 11.0, seed() + 2.0));
    let h2 = hash22(vec2<f32>(f32(k) * 1.371 + 17.0, seed() + 5.0));
    let rad = sqrt(h2.x) * 1.4;
    let ang = h2.y * TAU;
    let u = rad * cos(ang);
    let v = rad * sin(ang);
    let rate = 0.7 + 0.6 * h.y;
    let span = len + 4.0;
    let head = fract(h.x + item(FALL_SHARED).y * rate / span) * span - 2.0;
    let streak = 0.12 + 0.18 * h.y;
    let s_own = select(head - streak, head, at_b);
    let s_other = select(head, head - streak, at_b);
    let pitch = floor(h.y * 12.0);
    let fade = smoothstep(-2.0, 0.0, head) * (1.0 - smoothstep(len - 1.0, len + 2.0, head)) * select(0.3, 1.0, at_b);
    let colour = mix(stroke_colour(pitch, sec, h.x - 0.5, P.along * s_own / len), vec3<f32>(1.0), 0.35)
        * strand_light(pitch, s_own) * 1.5;
    return strip_corner(e, u, v, 3.0, s_own, s_other, streak * 0.5, side, fade, 0.006 * P.width, 3.0, colour);
}

// The strand's cross-section: a Gaussian, three sigmas to the strip's edge,
// added. Scaled by what Persistence lets build up, so a strand that holds
// still shows the same at any Persistence and only movement smears.
fn paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    let a = f.data.x;
    let falloff = exp(-4.5 * a * a);
    let keep = 1.0 - pow(max(P.persistence, 1e-6), V.dt);
    return vec4<f32>(f.colour.rgb * falloff * P.intensity * 0.35 * keep, 0.0);
}
