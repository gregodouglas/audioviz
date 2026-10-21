// Sigil: a neon sacred-geometry glyph. Every line is a signed-distance tube
// with a hot core and a coloured halo. Centre rings breathe with their band,
// rays strike on onsets at the angle of the loudest pitch class and fade
// outward, twelve circle-and-dot orbs on a counter-rotating orbit are lit by
// the chord, a nested star polygon and cardinal triangles turn with the
// energy and step on bars, a pen walks the sigil trace with the beat, and
// paint drips from whatever is bright when it is loud. The arrangement (ring
// radii and styles, ray symmetry, the star, the trace) is seeded by the
// track: a track change puts the elements out in order (1.6 s) and lights a
// new glyph (3.2 s). Rays, the pen, drips and the drop flare go into the
// trail, so they leave afterimages.
//
// Prepare items (the state the compiled-in Sigil kept on the CPU):
//   0       (lit 0..1, going out, pending seed or -1, seed)
//   1       (last track, started, last New arrangement count, smoothed energy)
//   2       (spin, orbit angle, star turn, explode)
//   3       (triangle turn, its target, last bar, pen)
//   4       (pen last frame, seconds since a ray, last onset hit, ray cursor)
//   5       (drip clock, drips spawned, spread, orbit radius after spread)
//   6, 7    the six grouped bands, smoothed; then (7.z, 7.w) which rays and
//           drips are live, as bits, so shade() visits only those
//   8..14   the arrangement: (symmetry, orbit radius, orb radius, ray length),
//           (spin direction, nesting, star n, star k), (star radius, triangle
//           radius, triangle size, trace points), trace classes (two items),
//           ring speeds (two items)
//   15..20  rings (radius, width, style: 0 solid / >0 dashes / -1 double, turn)
//   21..24  this frame's trace points, xy pairs (from last frame's
//           arrangement: on a rebuild frame everything is dark)
//   25..48  rays (angle, strength, length, palette position)
//   81      (the section last frame, 0, 0, 0)
//   49..80  drips, two items each: (x, top, head, palette position),
//           (strength, speed, age, life)
// Every item works out the frame's events (rebuild, ray strike, drip spawn)
// from last frame's items the same way, so they all agree.

const N_RAYS: u32 = 24u;
const N_RINGS: u32 = 6u;
const N_DRIPS: u32 = 16u;
const ARR0: u32 = 8u;
const RING0: u32 = 15u;
const TRACE0: u32 = 21u;
const RAY0: u32 = 25u;
const DRIP0: u32 = 49u;
const SECTION_ITEM: u32 = 81u;
// Seconds for the elements to go out, and to light back up.
const OUT_S: f32 = 1.6;
const IN_S: f32 = 3.2;
// Ray life (strength e-fold) and the least time between strikes, seconds.
const RAY_LIFE: f32 = 0.5;
const RAY_GAP: f32 = 0.25;
const PI: f32 = 3.14159265;

// ---------------------------------------------------------------- random

fn pcg(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

fn rnd(state: ptr<function, u32>) -> f32 {
    *state = pcg(*state);
    return f32(*state >> 8u) / 16777216.0;
}

// Seeds are whole numbers below 2^24, so an item holds them exactly.
fn seed_of(track: f32) -> f32 {
    return f32(u32(clamp(track, 0.0, 1.0) * 16777215.0));
}

// The seed for a section of the track: the track's own for the first
// section (A), so a song starts on its glyph, and one per label after, so a
// section that comes back gets its glyph back.
fn section_seed_of(track: f32, label: f32) -> f32 {
    return seed_of(select(fract(track + label * 0.618034), track, label < 0.5));
}

fn next_seed(seed: f32) -> f32 {
    return f32(pcg(u32(seed) + 1u) >> 8u);
}

// ---------------------------------------------------------------- arrangement

fn sort6(r: ptr<function, array<f32, 6>>) {
    for (var i = 1u; i < 6u; i++) {
        let v = (*r)[i];
        var j = i;
        while (j > 0u && (*r)[j - 1u] > v) {
            (*r)[j] = (*r)[j - 1u];
            j--;
        }
        (*r)[j] = v;
    }
}

// The glyph's layout from a seed, as the arrangement items (ARR0 ..
// TRACE0 - 1) hold it. Built straight into one array, and the trace packed
// into an integer: D3D's FXC unrolls loops that write arrays at a runtime
// index, which made this shader take seconds to build.
fn arrangement(seed: u32) -> array<vec4<f32>, 13> {
    var s = seed ^ 0xA5A55A5Au;
    var out: array<vec4<f32>, 13>;
    let orbit_radius = 0.6 + 0.26 * rnd(&s);
    let inner = 0.11 + 0.1 * rnd(&s);
    // Six radii between the inner ring and the edge, kept clear of the orbit.
    var radii: array<f32, 6>;
    radii[0] = inner;
    for (var i = 1u; i < 6u; i++) {
        radii[i] = inner + (0.92 - inner) * rnd(&s);
    }
    sort6(&radii);
    for (var i = 0u; i < 6u; i++) {
        let d = radii[i] - orbit_radius;
        if (abs(d) < 0.1) {
            radii[i] = orbit_radius + select(0.1, -0.1, d < 0.0);
        }
    }
    // Re-sort after the push and spread near-coincident rings apart.
    sort6(&radii);
    for (var i = 1u; i < 6u; i++) {
        if (radii[i] - radii[i - 1u] < 0.03) {
            radii[i] = radii[i - 1u] + 0.03;
        }
    }
    // Rings: radius, width, style (0 solid, a dash count, -1 double), turn;
    // their speeds go in items 5 and 6.
    var speed = array<f32, 8>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for (var i = 0u; i < 6u; i++) {
        let u = rnd(&s);
        var style = -1.0;
        if (u < 0.45) {
            style = 0.0;
        } else if (u < 0.8) {
            style = 12.0 * f32(1u + min(u32(rnd(&s) * 4.99), 4u));
            style = select(style, 72.0, style > 48.0);
        }
        let width = 0.0045 + 0.005 * rnd(&s) * select(1.0, 1.4, i == 0u);
        speed[i] = select(-1.0, 1.0, (i & 1u) == 0u) * (0.03 + 0.08 * rnd(&s));
        out[7u + i] = vec4<f32>(radii[i], width, style, rnd(&s) * TAU);
    }
    // Rays per onset: 1 1 1 2 2 3 by a sixth each.
    let sk = min(u32(rnd(&s) * 5.99), 5u);
    let symmetry = select(select(1.0, 2.0, sk >= 3u), 3.0, sk == 5u);
    let orb_radius = 0.035 + 0.03 * rnd(&s);
    // Centre star polygon: {3/1} {4/1} {5/2} {6/2} {7/3} {8/3}, between the
    // inner rings and the orbit.
    var ns = array<vec2<f32>, 8>(vec2<f32>(3.0, 1.0), vec2<f32>(4.0, 1.0), vec2<f32>(5.0, 2.0), vec2<f32>(6.0, 2.0), vec2<f32>(7.0, 3.0), vec2<f32>(8.0, 3.0), vec2<f32>(6.0, 2.0), vec2<f32>(5.0, 2.0));
    let nk = ns[min(u32(rnd(&s) * 7.99), 7u)];
    let poly_r = orbit_radius * (0.5 + 0.3 * rnd(&s));
    // Triangles inside the orbit or beyond it.
    let outside = rnd(&s) < 0.5;
    let tri_r = select((poly_r + orbit_radius) * 0.5, orbit_radius + orb_radius + 0.12, outside);
    let tri_size = select(clamp((orbit_radius - poly_r) * 0.32, 0.035, 0.08), 0.075, outside);
    // Trace: four to seven distinct orbit points, no point twice in three,
    // four bits each.
    let tn = 4u + min(u32(rnd(&s) * 3.99), 3u);
    var bits = 0u;
    var last1 = 99u;
    var last2 = 99u;
    var n = 0u;
    for (var guard = 0u; guard < 64u && n < tn; guard++) {
        let c = min(u32(rnd(&s) * 11.99), 11u);
        if (c != last1 && c != last2) {
            bits |= c << (4u * n);
            last2 = last1;
            last1 = c;
            n++;
        }
    }
    let ray_len = 0.3 + 0.35 * rnd(&s);
    let spin_dir = select(1.0, -1.0, rnd(&s) < 0.5);
    let nest = 1.0 + f32(min(u32(rnd(&s) * 2.99), 2u));
    let t = vec4<u32>(0u, 4u, 8u, 12u);
    out[0] = vec4<f32>(symmetry, orbit_radius, orb_radius, ray_len);
    out[1] = vec4<f32>(spin_dir, nest, nk.x, nk.y);
    out[2] = vec4<f32>(poly_r, tri_r, tri_size, f32(n));
    out[3] = vec4<f32>((vec4<u32>(bits) >> t) & vec4<u32>(15u));
    out[4] = vec4<f32>((vec4<u32>(bits) >> (t + 16u)) & vec4<u32>(15u));
    out[5] = vec4<f32>(speed[0], speed[1], speed[2], speed[3]);
    out[6] = vec4<f32>(speed[4], speed[5], 0.0, 0.0);
    return out;
}

// Last frame's arrangement, field by field, from the items that hold it.
// (On a rebuild frame the frame step still sees the old one; everything is
// dark then, and the arrangement items already hold the new one.)
fn arr_radius(k: u32) -> f32 {
    return item(RING0 + k).x;
}

fn arr_trace(k: u32) -> f32 {
    return item(ARR0 + 3u + k / 4u)[k % 4u];
}

// ---------------------------------------------------------------- the frame

struct G {
    lit: f32,
    going_out: f32,
    pending: f32,
    seed: f32,
    last_track: f32,
    last_section: f32,
    started: f32,
    last_button: f32,
    energy: f32,
    spin: f32,
    orbit: f32,
    poly_rot: f32,
    explode: f32,
    tri_rot: f32,
    tri_target: f32,
    last_bar: f32,
    pen: f32,
    pen_prev: f32,
    since_ray: f32,
    last_onset: f32,
    cursor: f32,
    drip_acc: f32,
    spawns: f32,
    spread: f32,
    orbit_r: f32,
    bands: array<f32, 6>,
    // This frame's events.
    reseed: bool,
    clear: bool,
    strike: bool,
    pitch: f32,
    strength: f32,
    spawn: bool,
    pace: f32,
    dt: f32,
};

fn ray_symmetry() -> u32 {
    let P = params();
    return max(select(u32(P.symmetry), u32(item(ARR0).x), P.symmetry < 0.5), 1u);
}

// The frame from last frame's items: what the compiled-in Sigil's render did
// on the CPU before it wrote its uniform.
fn advance() -> G {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let i0 = item(0u);
    let i1 = item(1u);
    let i2 = item(2u);
    let i3 = item(3u);
    let i4 = item(4u);
    let i5 = item(5u);
    let b0 = item(6u);
    let b1 = item(7u);
    var g: G;
    g.dt = dt;
    g.lit = i0.x;
    g.going_out = i0.y;
    g.pending = i0.z;
    g.seed = i0.w;
    g.last_track = i1.x;
    g.last_section = item(SECTION_ITEM).x;
    g.started = i1.y;
    g.last_button = i1.z;
    g.energy = i1.w;
    g.spin = i2.x;
    g.orbit = i2.y;
    g.poly_rot = i2.z;
    g.explode = i2.w;
    g.tri_rot = i3.x;
    g.tri_target = i3.y;
    g.last_bar = i3.z;
    g.pen = i3.w;
    g.pen_prev = i3.w;
    g.since_ray = i4.y;
    g.last_onset = i4.z;
    g.cursor = i4.w;
    g.drip_acc = i5.x;
    g.spawns = i5.y;

    // Track change: queue a rebuild (or reseed at once while still dark).
    if (g.started < 0.5) {
        g.started = 1.0;
        g.seed = seed_of(V.track);
        g.reseed = true;
        g.pending = -1.0;
        g.last_track = V.track;
        g.last_button = P.new_arrangement;
        g.energy = 1.0;
        g.since_ray = 1.0;
    } else {
        if (V.track == g.last_track && section() != g.last_section && P.by_section > 0.5) {
            // A new section of the song: its own glyph (the one it had, if it came before).
            g.pending = section_seed_of(V.track, section());
            g.going_out = 1.0;
        }
        if (V.track != g.last_track) {
            g.last_track = V.track;
            if (P.rebuild > 0.5) {
                if (g.lit < 0.3 && g.going_out < 0.5) {
                    g.seed = seed_of(V.track);
                    g.reseed = true;
                } else {
                    g.pending = seed_of(V.track);
                    g.going_out = 1.0;
                }
            }
        }
        if (P.new_arrangement != g.last_button) {
            g.last_button = P.new_arrangement;
            g.pending = next_seed(g.seed);
            g.going_out = 1.0;
        }
    }
    if (g.going_out > 0.5) {
        g.lit -= dt / OUT_S;
        if (g.lit <= 0.0) {
            g.lit = 0.0;
            g.going_out = 0.0;
            if (g.pending >= 0.0) {
                g.seed = g.pending;
                g.pending = -1.0;
                g.reseed = true;
            }
            g.clear = true;
            g.pen = 0.0;
            g.pen_prev = 0.0;
        }
    } else if (g.lit < 1.0) {
        g.lit = min(g.lit + dt / IN_S, 1.0);
    }

    // Music.
    g.energy += (V.energy - g.energy) * (1.0 - exp(-dt / 0.4));
    // Six band groups (sub, bass, low mid, mid, presence, air) by maximum.
    var edges = array<u32, 7>(0u, 4u, 9u, 15u, 21u, 27u, 32u);
    var bands = array<f32, 6>(b0.x, b0.y, b0.z, b0.w, b1.x, b1.y);
    for (var i = 0u; i < 6u; i++) {
        var m = 0.0;
        for (var j = edges[i]; j < edges[i + 1u]; j++) {
            m = max(m, band_raw(j));
        }
        let v = clamp(m, 0.0, 1.5);
        let k = select(0.18, 0.04, v > bands[i]);
        bands[i] += (v - bands[i]) * (1.0 - exp(-dt / k));
    }
    g.bands = bands;
    let flare = V.onsets.x;

    // Rays: one strike per onset, at the loudest pitch class.
    g.since_ray += dt;
    let hit = V.hits.w > 0.5;
    if (hit && g.last_onset < 0.5 && g.since_ray > RAY_GAP && g.lit > 0.3 && g.going_out < 0.5 && V.silent < 0.5) {
        g.strike = true;
        g.since_ray = 0.0;
        var best = 0u;
        var bv = -1.0;
        for (var i = 0u; i < 12u; i++) {
            if (chroma(i) > bv) {
                bv = chroma(i);
                best = i;
            }
        }
        g.pitch = f32(best);
        g.strength = min(0.55 + 0.7 * min(V.strengths.w, 1.0) + 0.5 * flare, 1.5);
    }
    g.last_onset = select(0.0, 1.0, hit);

    // Motion: the glyph spins slowly, the orbit counter-rotates, rings turn.
    g.pace = (0.5 + 0.5 * g.energy) * P.spin;
    let dir = item(ARR0 + 1u).x;
    g.spin += dt * 0.025 * dir * g.pace;
    g.orbit -= dt * 0.06 * dir * g.pace;
    g.poly_rot -= dt * 0.04 * dir * g.pace;
    // Triangles step a twelfth of a turn every bar and glide there (a bar
    // is known by the beat count at its first beat, from the analysis's bar).
    let bar = V.beat_count - beat_in_bar();
    if (bar != g.last_bar && V.confidence > 0.3) {
        g.last_bar = bar;
        g.tri_target += TAU / 12.0;
    }
    g.tri_target += dt * 0.02 * g.pace;
    g.tri_rot += (g.tri_target - g.tri_rot) * (1.0 - exp(-dt / 0.25));
    // The pen walks one trace segment per beat, or at a steady pace without a beat.
    let segs = max(item(ARR0 + 2u).w, 2.0) - 1.0;
    if (V.confidence > 0.3) {
        let want = min(f32(u32(V.beat_count) % max(u32(segs), 1u)) + clamp(V.beat_phase, 0.0, 1.0), segs);
        var diff = want - g.pen;
        if (diff < -segs * 0.5) {
            diff += segs;
        }
        let x = g.pen + diff * (1.0 - exp(-dt / 0.08));
        g.pen = x - segs * floor(x / segs);
    } else {
        let x = g.pen + dt * 0.7 * g.pace;
        g.pen = x - segs * floor(x / segs);
    }

    // Spread: everything outside the inner ring moves radially, and drops
    // push it outward for a moment (kicks a little).
    let push = max(0.5 * flare, 1.5 * drop_pulse());
    let k_ex = select(0.6, 0.1, push > g.explode);
    g.explode += (push - g.explode) * (1.0 - exp(-dt / k_ex));
    g.spread = max(P.spread * (1.0 + 0.6 * P.explode * g.explode * P.react), 0.05);
    let r0 = arr_radius(0u);
    g.orbit_r = r0 + (item(ARR0).y - r0) * g.spread;

    // Drips: one more each time the clock passes a whole number.
    let rate = P.drips * (0.35 + 1.5 * max(g.energy - 0.9, 0.0) + 0.8 * V.onsets.w) * g.lit;
    g.drip_acc += dt * rate;
    if (g.drip_acc >= 1.0) {
        g.drip_acc -= 1.0;
        g.spawn = true;
        g.spawns += 1.0;
    }
    return g;
}

// The ray slots a strike takes: the weakest after the cursor for each of the
// symmetric rays, so live rays survive. Slot k of the result is the k-th ray.
struct Picks {
    slot: array<u32, 6>,
    cursor: f32,
};

fn ray_picks(g: G) -> Picks {
    var s: array<f32, 24>;
    for (var j = 0u; j < N_RAYS; j++) {
        s[j] = select(item(RAY0 + j).y, 0.0, g.clear);
    }
    var out: Picks;
    var slots: array<u32, 6>;
    var cur = u32(g.cursor);
    let sym = ray_symmetry();
    for (var k = 0u; k < sym; k++) {
        var best = cur % N_RAYS;
        for (var j = 0u; j < N_RAYS; j++) {
            let idx = (cur + j) % N_RAYS;
            if (s[idx] < 0.02) {
                best = idx;
                break;
            }
            if (s[idx] < s[best]) {
                best = idx;
            }
        }
        cur = best + 1u;
        slots[k] = best;
        s[best] = g.strength;
    }
    out.slot = slots;
    out.cursor = f32(cur);
    return out;
}

fn spread_r(g: G, r: f32) -> f32 {
    let r0 = arr_radius(0u);
    return r0 + (r - r0) * g.spread;
}

fn ray_item(g: G, j: u32, picks: Picks) -> vec4<f32> {
    var r = select(item(RAY0 + j), vec4<f32>(0.0), g.clear);
    if (g.strike) {
        let sym = ray_symmetry();
        for (var k = 0u; k < sym; k++) {
            if (picks.slot[k] == j) {
                r = vec4<f32>(g.pitch / 12.0 * TAU + f32(k) * TAU / f32(sym), g.strength, item(ARR0).w * (0.8 + 0.4 * g.strength), g.pitch / 12.0);
            }
        }
    }
    r.y *= exp(-g.dt / RAY_LIFE);
    if (r.y < 0.004) {
        r.y = 0.0;
    }
    return r;
}

// Drip k this frame: spawned from a lit orb, a ring bottom or a trace point
// when the clock passes, then running down, slowing as it dries.
fn drip_item(g: G, k: u32, second: bool) -> vec4<f32> {
    var a = select(item(DRIP0 + 2u * k), vec4<f32>(0.0), g.clear);
    var b = select(item(DRIP0 + 2u * k + 1u), vec4<f32>(0.0), g.clear);
    if (g.spawn) {
        // The weakest drip (the first of equals) takes it, if it is weak enough.
        var slot = 0u;
        var least = 1e9;
        for (var q = 0u; q < N_DRIPS; q++) {
            let st = select(item(DRIP0 + 2u * q + 1u).x, 0.0, g.clear);
            if (st < least) {
                least = st;
                slot = q;
            }
        }
        if (slot == k && least < 0.3) {
            var s = pcg(u32(g.spawns) * 0x9E3779B9u ^ u32(g.seed));
            let u = rnd(&s);
            let tau12 = TAU / 12.0;
            var at = vec3<f32>(0.0);
            if (u < 0.55) {
                // An orb that is lit.
                var best = 0u;
                var bv = -1.0;
                for (var c = 0u; c < 12u; c++) {
                    let v = chroma_slow(c) + rnd(&s) * 0.5;
                    if (v > bv) {
                        bv = v;
                        best = c;
                    }
                }
                let ang = g.orbit + f32(best) * tau12;
                at = vec3<f32>(g.orbit_r * cos(ang), g.orbit_r * sin(ang) - item(ARR0).z, f32(best) / 12.0 + 0.5);
            } else if (u < 0.8) {
                let i = min(u32(rnd(&s) * 5.99), 5u);
                let r = select(spread_r(g, arr_radius(i)), arr_radius(0u), i == 0u);
                let jitter = (rnd(&s) - 0.5) * 0.6;
                at = vec3<f32>(r * sin(jitter), -r * cos(jitter), f32(i) / 6.0 + 0.04);
            } else {
                let i = min(u32(rnd(&s) * (item(ARR0 + 2u).w - 0.01)), 7u);
                let ang = g.orbit + arr_trace(i) * tau12;
                at = vec3<f32>(g.orbit_r * cos(ang), g.orbit_r * sin(ang), 0.5);
            }
            a = vec4<f32>(at.x, at.y, at.y, at.z);
            b = vec4<f32>(0.7 + 0.5 * min(g.energy, 1.5), 0.0, 0.0, 2.5 + 2.5 * rnd(&s));
        }
    }
    if (b.x > 0.0) {
        let dt = g.dt;
        b.z += dt;
        // Slow paint: it speeds up a little, then dries once its life is over.
        let dry = clamp(b.z / b.w, 0.0, 1.0);
        b.y = min((b.y + dt * 0.05) * (1.0 - dry), 0.1);
        a.z -= b.y * dt;
        // The run keeps a bounded length: the top follows the head.
        let max_len = 0.1 + 0.3 * fract(a.w);
        if (a.y - a.z > max_len) {
            a.y = a.z + max_len;
        }
        if (b.z > b.w) {
            b.x -= dt * 0.5;
        }
        if (a.z < -1.6 || b.x <= 0.0) {
            b.x = 0.0;
        }
    }
    return select(a, b, second);
}

fn prepare(i: u32) -> vec4<f32> {
    let g = advance();
    // The arrangement: built on a rebuild, else kept, the rings turning.
    if (i >= ARR0 && i < TRACE0) {
        var v = item(i);
        if (g.reseed) {
            v = arrangement(u32(g.seed))[i - ARR0];
        }
        if (i >= RING0) {
            let k = i - RING0;
            let speed = item(ARR0 + 5u + k / 4u)[k % 4u];
            v.w += g.dt * speed * g.pace;
        }
        return v;
    }
    // The strike's ray slots, worked out once for the items that need them.
    var picks: Picks;
    picks.cursor = g.cursor;
    if (g.strike && (i == 4u || i == 7u || (i >= RAY0 && i < DRIP0))) {
        picks = ray_picks(g);
    }
    if (i >= RAY0 && i < DRIP0) {
        return ray_item(g, i - RAY0, picks);
    }
    if (i == SECTION_ITEM) {
        return vec4<f32>(section(), 0.0, 0.0, 0.0);
    }
    if (i >= DRIP0) {
        return drip_item(g, (i - DRIP0) / 2u, ((i - DRIP0) & 1u) == 1u);
    }
    if (i >= TRACE0) {
        // Two trace points per item, on the orbit at their pitch classes.
        let tn = item(ARR0 + 2u).w;
        var out = vec4<f32>(0.0);
        for (var h = 0u; h < 2u; h++) {
            let k = 2u * (i - TRACE0) + h;
            if (f32(k) < tn) {
                let ang = g.orbit + arr_trace(k) * TAU / 12.0;
                let q = g.orbit_r * vec2<f32>(cos(ang), sin(ang));
                if (h == 0u) {
                    out = vec4<f32>(q, out.zw);
                } else {
                    out = vec4<f32>(out.xy, q);
                }
            }
        }
        return out;
    }
    switch (i) {
        case 0u: { return vec4<f32>(g.lit, g.going_out, g.pending, g.seed); }
        case 1u: { return vec4<f32>(g.last_track, g.started, g.last_button, g.energy); }
        case 2u: { return vec4<f32>(g.spin, g.orbit, g.poly_rot, g.explode); }
        case 3u: { return vec4<f32>(g.tri_rot, g.tri_target, g.last_bar, g.pen); }
        case 4u: { return vec4<f32>(g.pen_prev, g.since_ray, g.last_onset, picks.cursor); }
        case 5u: { return vec4<f32>(g.drip_acc, g.spawns, g.spread, g.orbit_r); }
        case 6u: { return vec4<f32>(g.bands[0], g.bands[1], g.bands[2], g.bands[3]); }
        default: {
            // A ray is live when it still shows after this frame's decay
            // (as ray_item works it out); a drip from last frame (a new one
            // shows from the next frame).
            let decay = exp(-g.dt / RAY_LIFE);
            var rays = 0u;
            var drips = 0u;
            if (!g.clear) {
                for (var j = 0u; j < N_RAYS; j++) {
                    if (item(RAY0 + j).y * decay >= 0.004) {
                        rays |= 1u << j;
                    }
                }
                for (var q = 0u; q < N_DRIPS; q++) {
                    if (item(DRIP0 + 2u * q + 1u).x > 0.0) {
                        drips |= 1u << q;
                    }
                }
            }
            if (g.strike) {
                for (var k = 0u; k < ray_symmetry(); k++) {
                    rays |= 1u << picks.slot[k];
                }
            }
            return vec4<f32>(g.bands[4], g.bands[5], f32(rays), f32(drips));
        }
    }
}

// ---------------------------------------------------------------- colour

// Set at the top of shade() for the helpers below.
var<private> glow_k: f32;
var<private> hot_k: f32;
var<private> lit_k: f32;

fn pal(x: f32) -> vec3<f32> {
    let t = fract(x) * 32.0;
    let i = u32(floor(t)) % 32u;
    return mix(V.palette[i].rgb, V.palette[(i + 1u) % 32u].rgb, fract(t));
}

// Tint ramp when no palette is set: the tint, leaning warm or cool by position.
fn own(t: f32) -> vec3<f32> {
    let base = V.tint.rgb;
    let warm = vec3<f32>(1.0, 0.55, 0.35);
    let cool = vec3<f32>(0.45, 0.6, 1.0);
    let lean = mix(cool, warm, 0.5 + 0.5 * sin(t * TAU));
    return normalize(mix(base, lean * length(base), 0.35)) * length(base);
}

fn col_at(pos: f32) -> vec3<f32> {
    let c = mix(own(pos), pal(pos + V.palette_shift), V.palette_mix);
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return max(mix(vec3<f32>(l), c, 1.25), vec3<f32>(0.0));
}

// A neon tube at distance `d` with half-width `w`: white-hot core plus a
// coloured halo with a long tail.
fn tube(d: f32, w: f32, c: vec3<f32>) -> vec3<f32> {
    // Far from the line only the 1/d tail is left (the core and the near
    // halo are below 1e-5 there), so skip their exponentials.
    if (d > w * 48.0) {
        return c * (0.07 * w / (d + w * 0.5)) * glow_k;
    }
    let core = exp(-(d * d) / (w * w));
    let halo = exp(-d / (w * 4.0)) * 0.45 + 0.07 * w / (d + w * 0.5);
    return mix(c, vec3<f32>(1.0), hot_k) * core * (1.3 + 1.2 * hot_k) + c * halo * glow_k;
}

// Visibility of an element with light-up order `order` (0 = first) and a
// per-element id for the strike flicker near the threshold.
fn lit(order: f32, id: f32) -> f32 {
    let on = smoothstep(order, order + 0.1, lit_k);
    let near = 1.0 - smoothstep(0.0, 0.22, abs(lit_k - order - 0.05));
    let strike = step(0.4, hash21(vec2<f32>(floor(V.time * 26.0), id)));
    return on * mix(1.0, strike, near);
}

// A vector's component picked by select, not by v[k]: a runtime component
// index in the ring loop cost 0.8 ms a frame at 1440p on D3D.
fn band6(i: u32) -> f32 {
    let v = item(6u + i / 4u);
    let k = i % 4u;
    return select(select(v.w, v.z, k == 2u), select(v.y, v.x, k == 0u), k < 2u);
}

// Orb lighting: the slow profile lifted so chords read, the fast one on top.
fn orb_chroma(i: u32) -> f32 {
    return clamp(0.6 * chroma_slow(i) + 0.6 * chroma(i), 0.0, 1.2);
}

// ---------------------------------------------------------------- shapes

// Distance to a regular polygon of `n` sides and circumradius `r`, rotated `rot`.
fn polygon(p: vec2<f32>, n: f32, r: f32, rot: f32) -> f32 {
    let a = atan2(p.y, p.x) + rot;
    let seg = TAU / n;
    let b = (floor(0.5 + a / seg)) * seg - a;
    return length(p) * cos(b) - r * cos(seg * 0.5);
}

// Distance to the segment from `a` to `b`, and the position along it.
fn segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return vec2<f32>(length(pa - ba * h), h);
}

// Outline distance to the star polygon {n/k} of circumradius `r`: every
// vertex joined to the k-th next one (k = 1 is the plain polygon).
fn polygram(p: vec2<f32>, n: u32, k: u32, r: f32, rot: f32) -> f32 {
    // One sin/cos for the figure: vertex i+1 is vertex i turned by a step,
    // and the k-th next vertex is vertex i turned by k steps.
    var d = 1e3;
    let st = TAU / f32(n);
    let cs = vec2<f32>(cos(st), sin(st));
    let ck = vec2<f32>(cos(st * f32(k)), sin(st * f32(k)));
    var v = r * vec2<f32>(cos(rot), sin(rot));
    for (var i = 0u; i < n; i++) {
        let w = vec2<f32>(v.x * ck.x - v.y * ck.y, v.x * ck.y + v.y * ck.x);
        d = min(d, segment(p, v, w).x);
        v = vec2<f32>(v.x * cs.x - v.y * cs.y, v.x * cs.y + v.y * cs.x);
    }
    return d;
}

fn trace_point(i: u32) -> vec2<f32> {
    let v = item(TRACE0 + i / 2u);
    return select(v.zw, v.xy, (i & 1u) == 0u);
}

// Point at `t` segments along the trace.
fn trace_at(t: f32, tn: u32) -> vec2<f32> {
    let si = u32(clamp(floor(t), 0.0, f32(tn - 2u)));
    return mix(trace_point(si), trace_point(si + 1u), clamp(t - f32(si), 0.0, 1.0));
}

// ---------------------------------------------------------------- ground

fn fbm(p: vec2<f32>) -> f32 {
    return 0.5 * noise(p) + 0.3 * noise(p * 2.1 + 5.0) + 0.2 * noise(p * 4.3 - 2.0);
}

// Dark blue wash, brush streaks, a few violet patches, a vignette and sparse
// splatter in the glyph's colours. Barely reactive.
fn ground(p: vec2<f32>, energy: f32, gain: f32) -> vec3<f32> {
    let q = p * 1.1 + vec2<f32>(V.time * 0.008, V.time * 0.004);
    let wash = fbm(q);
    let streak = noise(vec2<f32>(p.x * 2.2 + p.y * 0.7, p.y * 16.0 + wash * 3.0));
    var g = vec3<f32>(0.020, 0.040, 0.135) * (0.5 + 1.0 * wash) * (0.8 + 0.4 * streak);
    let blotch = fbm(q * 2.3 + 9.0);
    g += vec3<f32>(0.06, 0.025, 0.14) * blotch * blotch * 0.7;
    g += vec3<f32>(0.0, 0.03, 0.05) * pow(noise(q * 0.7 + 31.0), 3.0);
    g *= 1.0 - 0.55 * smoothstep(0.7, 1.8, length(p));
    // Splatter: one dot in a few of the cells of a grid, static per cell.
    let cell = 16.0;
    let gi = floor(p * cell);
    let h = hash22(gi);
    if (h.x < 0.11) {
        let cp = (gi + 0.25 + 0.5 * hash22(gi + 3.1)) / cell;
        let rr = (0.12 + 0.45 * h.y * h.y) / cell * 0.5;
        let d = length(p - cp);
        let cc = col_at(hash21(gi + 7.7));
        g += cc * (0.25 + 0.6 * h.y) * smoothstep(rr, rr * 0.4, d) * (0.6 + 0.4 * lit(0.3 + 0.6 * h.y, h.x * 900.0));
    }
    return g * gain * (0.92 + 0.12 * energy);
}

// ---------------------------------------------------------------- glyph

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let scale = max(P.scale, 0.1);
    // Half-extent 1 along the short side, then the glyph scale.
    let p = centred(uv) / scale;
    let px = pixel() / scale;
    let lw = P.line_width;
    glow_k = P.glow;
    hot_k = P.hot;
    lit_k = item(0u).x;
    let energy = item(1u).w;
    let st = item(2u);
    let spin = st.x;
    let beat = V.beat;
    let flare = V.onsets.x;
    let react = P.react;
    let spread = item(5u).z;
    let orbit_r = item(5u).w;
    let a0 = item(ARR0);
    let a1 = item(ARR0 + 1u);
    let a2 = item(ARR0 + 2u);

    let r = length(p);
    let ang = atan2(p.y, p.x);

    var acc = vec3<f32>(0.0);
    if (P.ground > 0.001) {
        acc = ground(p, energy, P.ground);
    }
    var fresh = vec3<f32>(0.0);

    // Centre rings, one per band.
    let r_in = item(RING0).x;
    for (var i = 0u; i < N_RINGS; i++) {
        let R = item(RING0 + i);
        let b = band6(i);
        let radius = select(r_in + (R.x - r_in) * spread, R.x, i == 0u);
        let rad = radius * (1.0 + 0.06 * b * react);
        let w = max(R.y * lw * (1.0 + 0.5 * b * react), px * 1.1);
        var d = abs(r - rad);
        var m = 1.0;
        if (R.z > 0.5) {
            // Dashed: R.z ticks around the ring, soft gaps.
            let a = fract((ang + R.w + spin) * R.z / TAU);
            m = smoothstep(0.14, 0.22, abs(a - 0.5));
        } else if (R.z < -0.5) {
            // Double: two thin lines either side.
            d = abs(d - 0.018);
        }
        var bright = (0.3 + 0.6 * b * react) * P.rings;
        if (i == 0u) {
            bright = (bright + 0.8 * beat * react) * P.core;
        }
        acc += tube(d, w, col_at(f32(i) / 6.0 + 0.04)) * bright * m * lit(f32(i) * 0.11, f32(i) + 1.0);
    }

    // Rays: from the inner ring outward, tapered, fading along their length.
    let r0 = r_in * 0.9;
    var live_rays = u32(item(7u).z);
    while (live_rays != 0u) {
        let s = firstTrailingBit(live_rays);
        live_rays &= live_rays - 1u;
        let Rr = item(RAY0 + s);
        let a = Rr.x + spin;
        let dir = vec2<f32>(cos(a), sin(a));
        let len = Rr.z * (1.0 + 0.6 * flare * react);
        let sg = segment(p, dir * r0, dir * (r0 + len));
        let t = sg.y;
        let fade = (1.0 - t) * (1.0 - t);
        let w = max(0.0055 * lw * (1.0 - 0.6 * t), px * 1.1);
        fresh += tube(sg.x, w, col_at(Rr.w)) * Rr.y * fade * P.rays * lit(0.25, 40.0 + f32(s));
    }

    // Orbs: twelve circle-and-dot on the orbit; only the nearest is evaluated.
    {
        let orad = orbit_r * (1.0 + 0.03 * band6(1u) * react);
        let step_a = TAU / 12.0;
        let k = floor(0.5 + (ang - st.y) / step_a);
        let kk = u32((i32(k) % 12 + 12) % 12);
        let ca = st.y + k * step_a;
        let c = orad * vec2<f32>(cos(ca), sin(ca));
        let dc = length(p - c);
        let ch = orb_chroma(kk);
        let is_key = select(0.0, 1.0, u32(V.key) == kk);
        let bright = (0.22 + 1.3 * ch * react + 0.3 * is_key * react) * P.orbs;
        let col = col_at(f32(kk) / 12.0 + 0.5);
        let w = max(0.0055 * lw, px * 1.1);
        let ring = tube(abs(dc - a0.z), w, col);
        let dot_r = a0.z * (0.22 + 0.25 * ch * react + 0.2 * beat * react);
        let dotl = tube(max(dc - dot_r, 0.0), w * 1.3, col) * 1.2;
        acc += (ring + dotl) * bright * lit(0.62 + 0.02 * f32(kk % 3u), 100.0 + f32(kk));
    }

    let nest = select(u32(P.nest), u32(a1.y), P.nest < 0.5);

    // Centre polygram, nested, breathing with the bass and flashing on the beat.
    if (P.shapes > 0.001) {
        var skips = array<u32, 6>(1u, 1u, 2u, 2u, 3u, 3u);
        let by_track = P.sides < 0.5;
        let n = select(clamp(u32(P.sides), 3u, 8u), u32(a1.z), by_track);
        let k = select(skips[n - 3u], u32(a1.w), by_track);
        let poly_r = r_in + (a2.x - r_in) * spread;
        let bass = band6(1u);
        let pcol = col_at(0.9);
        let w = max(0.005 * lw, px * 1.1);
        for (var lv = 0u; lv < nest; lv++) {
            let shrink = pow(0.94, f32(lv));
            let rr = poly_r * shrink * (1.0 + 0.05 * bass * react);
            // Levels overlay: each a fraction of a step round, alternate ones counter-turning.
            let rot = st.z * select(1.0, -1.3, (lv & 1u) == 1u) + f32(lv) * PI / f32(n) / f32(max(nest, 1u)) * 2.0;
            // Far outside the figure the distance is about r - rr and the tube is
            // only its faint tail: skip the segments.
            var d = r - rr;
            if (d < w * 48.0) {
                d = polygram(p, n, k, rr, rot);
            }
            let bright = (0.45 + 0.35 * bass * react + 0.5 * beat * react) * P.shapes;
            acc += tube(d, w, pcol) * bright * lit(0.15 + 0.08 * f32(lv), 200.0 + f32(lv));
        }
    }

    // Cardinal triangles (nested, pointing outward) and diagonal diamonds.
    if (P.shapes > 0.001) {
        let tri_r = r_in + (a2.y - r_in) * spread;
        let tri_size = a2.z;
        let tri_rot = item(3u).x;
        let step_a = TAU / 8.0;
        let kq = floor(0.5 + ang / step_a);
        let kq8 = u32((i32(kq) % 8 + 8) % 8);
        let ca = kq * step_a;
        let dir = vec2<f32>(cos(ca), sin(ca));
        let q = p - tri_r * dir;
        let w = max(0.005 * lw, px * 1.1);
        if ((kq8 & 1u) == 0u) {
            // Cardinal: triangles, apex outward, own-axis turn on bars.
            let mid = band6(3u);
            let tcol = col_at(0.65);
            for (var lv = 0u; lv < nest; lv++) {
                let size = tri_size * pow(0.62, f32(lv));
                var d = length(q) - size;
                if (d < w * 48.0) {
                    d = abs(polygon(q, 3.0, size, -ca + tri_rot * select(1.0, -1.0, (lv & 1u) == 1u)));
                }
                let bright = (0.4 + 0.5 * mid * react) * P.shapes;
                acc += tube(d, w, tcol) * bright * lit(0.4 + 0.07 * f32(lv), 300.0 + f32(lv) + f32(kq8));
            }
        } else {
            // Diagonal: a diamond lit by the treble, with a dot.
            let treb = band6(4u);
            let dcol = col_at(0.3);
            let d = abs(polygon(q, 4.0, tri_size * 0.7 * (1.0 + 0.15 * treb * react), -ca));
            let bright = (0.3 + 0.7 * treb * react) * P.shapes;
            acc += (tube(d, w, dcol) + tube(length(q), w * 1.2, dcol) * 0.8) * bright * lit(0.52, 320.0 + f32(kq8));
        }
    }

    // Sigil trace: a polyline through orbit points, a dim steady line with a
    // beat-driven pen whose glow stays in the trail.
    let tn = u32(a2.w);
    if (tn >= 2u && P.trace > 0.001) {
        let w = max(0.004 * lw, px * 1.1);
        let tcol = col_at(0.5);
        var d = 1e3;
        for (var i = 0u; i + 1u < tn; i++) {
            d = min(d, segment(p, trace_point(i), trace_point(i + 1u)).x);
        }
        let p0 = trace_point(0u);
        let pe = trace_point(tn - 1u);
        let pprev = trace_point(tn - 2u);
        // Terminals: a small circle at the start, a cross-bar at the end.
        d = min(d, abs(length(p - p0) - 0.03));
        let tdir = normalize(pe - pprev);
        let perp = vec2<f32>(-tdir.y, tdir.x);
        d = min(d, segment(p, pe - perp * 0.035, pe + perp * 0.035).x);
        let on = lit(0.75, 400.0);
        acc += tube(d, w, tcol) * 0.3 * P.trace * on;
        // Pen: a stroke from where it was last frame to where it is, via the
        // vertex when it crossed one, so the trail is a continuous line.
        let pen = item(3u).w;
        let pen0 = item(4u).x;
        let pa = trace_at(pen0, tn);
        let pb = trace_at(pen, tn);
        var dpen = 1e3;
        if (abs(pen - pen0) < 1.5 && floor(pen) != floor(pen0)) {
            let v = trace_point(u32(clamp(max(floor(pen), floor(pen0)), 0.0, f32(tn - 1u))));
            dpen = min(segment(p, pa, v).x, segment(p, v, pb).x);
        } else if (abs(pen - pen0) < 1.5) {
            dpen = segment(p, pa, pb).x;
        } else {
            dpen = length(p - pb);
        }
        fresh += tube(dpen, w * 1.6, tcol) * (0.8 + 0.6 * beat * react) * P.trace * on;
    }

    // Drips: paint running down from bright elements, a tapering line with a
    // bead at the head, kept in the trail.
    if (P.drips > 0.001) {
        let w = max(0.0065 * lw, px * 1.1);
        var live_drips = u32(item(7u).w);
        while (live_drips != 0u) {
            let i = firstTrailingBit(live_drips);
            live_drips &= live_drips - 1u;
            let strength = clamp(item(DRIP0 + 2u * i + 1u).x, 0.0, 1.5);
            if (strength < 0.01) {
                continue;
            }
            let D = item(DRIP0 + 2u * i);
            if (abs(p.x - D.x) > 0.12) {
                continue;
            }
            let sg = segment(p, vec2<f32>(D.x, D.y), vec2<f32>(D.x, D.z));
            let taper = mix(1.0, 0.4, sg.y);
            let bead = length(p - vec2<f32>(D.x, D.z)) - w * 0.8;
            let d = min(sg.x / taper, max(bead, 0.0));
            fresh += tube(d, w, col_at(D.w)) * strength * 0.8 * P.drips;
        }
    }

    // Drops: a soft flare from the centre, left in the trail.
    fresh += col_at(V.palette_shift + 0.5) * flare * react * exp(-r * 4.5) * 0.45 * P.core * lit(0.0, 7.0);

    // Persistence: afterimages keep the brightest of the fade and the new light.
    let keep = exp(-clamp(V.dt, 0.0, 0.1) / max(P.persistence, 0.02));
    let kept = max(trail(uv) * keep, fresh);
    write_trail(kept);
    return acc + kept;
}
