// Jellyfish: a swarm of one species in a deep tank, in 3D. Six species,
// each with its own body and its own light, and each light answering a
// different part of the sound:
//   moon jelly     flat saucer, four horseshoe gonads that breathe with the
//                  bass, a fringe of short tentacles, four short frilly arms
//   sea nettle     deep striped bell (stripes pulse with the mids), long
//                  trailing tentacles, long spiralling oral arms
//   crystal jelly  clear flat bell with ~100 hair-thin canals and a ring of
//                  green rim lights that flash on onsets
//   atolla         deep red crown bell with a coronal groove; on a drop it
//                  spins its blue alarm pinwheel round the rim
//   comb jelly     a glassy oval that glides (no pulse) while rainbows race
//                  along its eight comb rows with the hi-hats
//   lion's mane    a big lobed bell and hundreds of fine hairs down which
//                  waves of light run with the sustained low end
// They live without the music (swim strokes on their own rhythm, a slow
// current, each heading for its own wandering spot and keeping apart); the
// Music control decides how much the music sways them.
//
// Uses the creatures library (camera, current, stroke, chain_link...).
//
// Prepare items:
//   0                   (1 once started)
//   1                   smoothed sound: (lows, mids, highs, sustained energy)
//   2                   (last any onset hit, last low hit, drop envelope, 0)
//   3                   the close-up camera: (where it looks, how far away)
//   JELLY0 + 5j + f     jelly j: (position, seconds into its stroke),
//                       (velocity, 0), (axis, spin), (size, hue, frames
//                       alive, last beat), (glow, flash, phase a, phase b)
//   STRAND0 + (j * STRANDS + s) * SEG + k
//                       strand s (tentacle or hair) of jelly j, link k: (x, y, z, 0)
//   ARM0 + (j * ARMS + a) * ARM_SEG + k
//                       oral arm a of jelly j, link k: (x, y, z, 0)
//   PROFILE0 + j * NODES + n
//                       the moon jelly's bell profile at node n (crown 0 to
//                       margin RINGS): (radius, height, tangent angle, stroke)
// Line point i is strand link STRAND0 + i. Triangles per jelly: the bell's
// outer skin (exumbrella), its inner skin (subumbrella; moon jelly only),
// each RINGS x SECTORS quads, then the arms (ARM_SEG - 1 links, ARM_ACROSS
// quads across each, folded into a V).

const MAX_J: u32 = 12u;
const STRANDS: u32 = 160u;
const SEG: u32 = 16u;
const ARMS: u32 = 4u;
const ARM_SEG: u32 = 16u;
const ARM_ACROSS: u32 = 8u;
const JELLY0: u32 = 4u;
const STRAND0: u32 = 64u;           // JELLY0 + 5 * MAX_J
const ARM0: u32 = 30784u;           // STRAND0 + MAX_J * STRANDS * SEG
const PROFILE0: u32 = 31552u;       // ARM0 + MAX_J * ARMS * ARM_SEG
const RINGS: u32 = 24u;
const NODES: u32 = 25u;             // RINGS + 1
const SECTORS: u32 = 64u;
const SHELL_CORNERS: u32 = 9216u;   // RINGS * SECTORS * 2 * 3
const BELL_CORNERS: u32 = 18432u;   // two skins
const CORNERS_J: u32 = 21312u;      // BELL_CORNERS + ARMS * (ARM_SEG - 1) * ARM_ACROSS * 2 * 3
// The tank's half size (world units), and a bell's radius at size 1.
const TANK: vec3<f32> = vec3<f32>(2.4, 1.1, 1.3);
const BELL_R: f32 = 0.2;

const MOON: u32 = 0u;
const NETTLE: u32 = 1u;
const CRYSTAL: u32 = 2u;
const ATOLLA: u32 = 3u;
const COMB: u32 = 4u;
const LION: u32 = 5u;

// ---------------------------------------------------------------- species

struct Species {
    scale: f32,        // bell size
    strands: f32,      // tentacles or hairs
    strand_len: f32,   // their length, in bell radii
    strand_px: f32,    // their width, in pixels near the middle of the tank
    arm_len: f32,      // oral arms' length, in bell radii (0: none)
    arm_w: f32,        // and width
    period: f32,       // seconds between strokes on its own (0: no strokes)
    swim: f32,         // push per stroke
};

fn species_of(kind: u32) -> Species {
    var s: Species;
    switch (kind) {
        case 1u: { s = Species(0.9, 24.0, 5.0, 1.6, 3.5, 0.22, 1.1, 1.0); }
        case 2u: { s = Species(0.95, 28.0, 3.0, 0.3, 0.25, 0.1, 2.0, 0.7); }
        case 3u: { s = Species(0.7, 22.0, 0.8, 1.2, 0.0, 0.0, 0.9, 1.1); }
        case 4u: { s = Species(0.85, 2.0, 3.5, 0.9, 0.0, 0.0, 0.0, 0.6); }
        case 5u: { s = Species(1.9, 120.0, 5.0, 0.55, 2.0, 0.4, 2.2, 0.6); }
        default: { s = Species(1.3, 160.0, 0.6, 0.32, 1.25, 0.14, 1.7, 0.8); }
    }
    return s;
}

fn kind() -> u32 {
    return min(u32(params().species), 5u);
}

// Colour of a part: 0 bell, 1 its light, 2 tentacles, 3 arms. Natural is
// the species' own; Palette takes the layer's palette by hue and part.
fn tone(part: u32, hue: f32) -> vec3<f32> {
    if (params().colours < 0.5) {
        return colour(hue + f32(part) * 0.17);
    }
    var c = array<vec3<f32>, 4>(vec3<f32>(0.5, 0.7, 1.0), vec3<f32>(0.95, 0.42, 1.0), vec3<f32>(0.62, 0.8, 1.0), vec3<f32>(0.68, 0.78, 1.0));
    switch (kind()) {
        case 1u: { c = array<vec3<f32>, 4>(vec3<f32>(1.0, 0.5, 0.12), vec3<f32>(1.0, 0.3, 0.05), vec3<f32>(1.0, 0.4, 0.1), vec3<f32>(1.0, 0.82, 0.62)); }
        case 2u: { c = array<vec3<f32>, 4>(vec3<f32>(0.7, 0.85, 1.0), vec3<f32>(0.2, 1.0, 0.35), vec3<f32>(0.75, 0.9, 1.0), vec3<f32>(0.6, 1.0, 0.7)); }
        case 3u: { c = array<vec3<f32>, 4>(vec3<f32>(0.75, 0.03, 0.06), vec3<f32>(0.15, 0.45, 1.0), vec3<f32>(0.8, 0.1, 0.12), vec3<f32>(0.8, 0.1, 0.1)); }
        case 4u: { c = array<vec3<f32>, 4>(vec3<f32>(0.6, 0.75, 1.0), vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(0.85, 0.85, 1.0), vec3<f32>(1.0, 1.0, 1.0)); }
        case 5u: { c = array<vec3<f32>, 4>(vec3<f32>(1.0, 0.22, 0.06), vec3<f32>(1.0, 0.6, 0.2), vec3<f32>(1.0, 0.42, 0.12), vec3<f32>(1.0, 0.62, 0.3)); }
        default: {}
    }
    return c[part];
}

// How strongly the music sways them: Natural, Reactive, Performance.
fn music() -> f32 {
    let m = params().music;
    return select(select(1.8, 1.0, m < 1.5), 0.15, m < 0.5);
}

fn the_camera() -> Camera {
    let P = params();
    // A little from below, as in an aquarium: bells show what glows inside.
    var c = drifting_camera(P.camera * 0.8, 4.6, 2.3);
    c.pitch -= 0.28;
    if (P.view > 0.5) {
        // Close-up: following the first jelly, slowly circling it, now level
        // with it, now under it looking up through the bell.
        let follow = item(3u);
        c.look = follow.xyz;
        c.dist = follow.w;
        c.yaw = V.time * (0.02 + 0.06 * P.camera) + V.instance * TAU;
        c.pitch = -0.32 + 0.28 * sin(V.time * 0.031 + V.instance * 5.0);
    }
    return c;
}

// Where the close-up camera looks and how far it stands (item 3): eased
// toward the first jelly, a little below its bell so the tentacles fit.
fn follow_step() -> vec4<f32> {
    let J = jelly_at(0u);
    let R = radius(J);
    let want = vec4<f32>(J.pos - J.axis * R * 0.6, R * 3.8);
    let last = item(3u);
    if (item(0u).x < 0.5 || last.w <= 0.0 || J.age < 2.5) {
        return want;
    }
    return mix(last, want, 1.0 - exp(-clamp(V.dt, 0.0, 0.1) / 1.2));
}

// ---------------------------------------------------------------- a jelly

struct Jelly {
    pos: vec3<f32>,
    since: f32,
    vel: vec3<f32>,
    axis: vec3<f32>,
    spin: f32,
    size: f32,
    hue: f32,
    age: f32,
    last_beat: f32,
    glow: f32,
    flash: f32,
    pa: f32,
    pb: f32,
};

fn jelly_at(j: u32) -> Jelly {
    let b = JELLY0 + 5u * j;
    let a0 = item(b);
    let a1 = item(b + 1u);
    let a2 = item(b + 2u);
    let a3 = item(b + 3u);
    let a4 = item(b + 4u);
    var J: Jelly;
    J.pos = a0.xyz;
    J.since = a0.w;
    J.vel = a1.xyz;
    J.axis = a2.xyz;
    J.spin = a2.w;
    J.size = a3.x;
    J.hue = a3.y;
    J.age = a3.z;
    J.last_beat = a3.w;
    J.glow = a4.x;
    J.flash = a4.y;
    J.pa = a4.z;
    J.pb = a4.w;
    return J;
}

fn jelly_count() -> u32 {
    return clamp(u32(params().count), 1u, MAX_J);
}

// The bell's radius in world units.
fn radius(J: Jelly) -> f32 {
    return BELL_R * species_of(kind()).scale * params().size * J.size;
}

// A point of the bell in its own units (margin radius 1, y along the axis):
// t runs from the crown (0) to the margin (1) (the comb jelly: pole to
// pole), th round it (0..1); c is the stroke there.
fn bell_local(k: u32, t: f32, th: f32, c: f32) -> vec3<f32> {
    let ang = th * TAU;
    var r = 0.0;
    var y = 0.0;
    if (k == COMB) {
        // Comb jelly: an oval, a slow waist wobble instead of strokes.
        let a = t * 3.14159;
        r = sin(a) * 0.55 * (1.0 + 0.04 * sin(V.time * 1.3 + t * 6.0));
        y = cos(a);
    } else {
        var amax = 1.35;
        var hk = 0.35;
        switch (k) {
            case 1u: { amax = 1.75; hk = 0.8; }
            case 2u: { amax = 1.25; hk = 0.28; }
            case 3u: { amax = 1.6; hk = 0.55; }
            case 5u: { amax = 1.5; hk = 0.5; }
            default: {}
        }
        let a = t * amax;
        r = sin(a) / sin(min(amax, 1.5708));
        y = (cos(a) - cos(amax)) * hk;
        if (k == ATOLLA) {
            // The coronal groove.
            r *= 1.0 - 0.07 * exp(-pow((t - 0.55) / 0.07, 2.0));
        }
        if (k == LION) {
            // Eight lobes at the margin.
            r *= 1.0 + 0.1 * cos(ang * 8.0) * t * t * t;
        }
        if (k == NETTLE || k == ATOLLA) {
            // A scalloped margin.
            r *= 1.0 + 0.025 * cos(ang * select(32.0, 20.0, k == ATOLLA)) * smoothstep(0.85, 1.0, t);
        }
        // The stroke: the margin pulls in most, the bell grows taller.
        r *= 1.0 - 0.3 * c * pow(t, 0.7);
        y -= 0.18 * c * t * t;
    }
    return vec3<f32>(r * cos(ang), y, r * sin(ang));
}

// The stroke at t: it starts at the margin and travels up to the crown.
fn local_stroke(J: Jelly, t: f32) -> f32 {
    if (species_of(kind()).period <= 0.0) {
        return 0.0;
    }
    return stroke(J.since - 0.12 * (1.0 - t), 0.22, 0.55);
}

// The jelly's own frame: its axis, and two directions round it turned by its spin.
fn frame_x(J: Jelly) -> vec3<f32> {
    let e = perpendicular(J.axis);
    return e * cos(J.spin) + cross(J.axis, e) * sin(J.spin);
}

fn to_world(J: Jelly, l: vec3<f32>) -> vec3<f32> {
    let ex = frame_x(J);
    let ez = cross(J.axis, ex);
    return J.pos + (ex * l.x + J.axis * l.y + ez * l.z) * radius(J);
}

fn dir_to_world(J: Jelly, l: vec3<f32>) -> vec3<f32> {
    let ex = frame_x(J);
    let ez = cross(J.axis, ex);
    return ex * l.x + J.axis * l.y + ez * l.z;
}

// ---------------------------------------------------------------- the moon jelly's bell
//
// The bell is a profile from the crown (s = 0) to the margin (s = 1), drawn
// by its tangent angle turning down along it and spun round the axis. A
// stroke is the coronal muscle curling the outer bell in and down; the thin
// margin is passive and follows late, so it whips in, then flares past rest
// on the release. The middle is thick jelly (mesoglea) and hardly bends.

const MOON_CROWN: f32 = 0.3;
// Seconds to close, and for the springy jelly to bring it back past rest.
const MOON_SQUEEZE: f32 = 0.32;
const MOON_SPRING: f32 = 0.4;

fn moon_rest_angle(s: f32) -> f32 {
    return 1.05 * pow(s, 1.7);
}

// The stroke at s: the muscle squeezes the outer bell at once, the margin
// lags behind it.
fn moon_drive(J: Jelly, s: f32) -> f32 {
    return elastic_stroke(J.since - 0.12 * smoothstep(0.55, 1.0, s), MOON_SQUEEZE, MOON_SPRING);
}

fn moon_angle(J: Jelly, s: f32) -> f32 {
    let c = moon_drive(J, s);
    let w = smoothstep(0.2, 1.0, s);
    // A release overshoots below zero: the margin flares out most.
    let flare = min(c, 0.0) * 1.8 * smoothstep(0.6, 1.0, s);
    return moon_rest_angle(s) + 1.1 * max(c, 0.0) * w * w + flare;
}

// The jelly between the skins, in bell radii: thick in the middle, thin at the margin.
fn moon_thick(s: f32) -> f32 {
    return 0.22 * pow(max(1.0 - s * s, 0.0), 1.2) + 0.012;
}

// Eight lobes round the margin, with a notch for the rhopalium (a sense
// organ) between each pair; th is 0..1 round the bell.
fn moon_lobe(th: f32, s: f32) -> f32 {
    let x = fract(th * 8.0);
    let lobe = 0.5 - 0.5 * cos(x * TAU);
    let d = min(x, 1.0 - x);
    let notch = exp(-(d * d) / (0.035 * 0.035));
    return 1.0 + (0.035 * lobe - 0.05 * notch) * smoothstep(0.8, 1.0, s);
}

// Node n of jelly j's profile for this frame (prepare item): the tangent
// angles integrated from the crown, scaled so the margin at rest is 1 out.
fn profile_step(i: u32) -> vec4<f32> {
    let idx = i - PROFILE0;
    let j = idx / NODES;
    let n = idx % NODES;
    let J = jelly_at(j);
    let ds = 1.0 / f32(RINGS);
    var rest = 0.0;
    var r = 0.0;
    var y = 0.0;
    for (var m = 0u; m < RINGS; m++) {
        let s = (f32(m) + 0.5) * ds;
        rest += cos(moon_rest_angle(s)) * ds;
        if (m < n) {
            let a = moon_angle(J, s);
            r += cos(a) * ds;
            y -= sin(a) * ds;
        }
    }
    let s = f32(n) * ds;
    return vec4<f32>(r / rest, y / rest + MOON_CROWN, moon_angle(J, s), moon_drive(J, s));
}

// A point of the moon jelly's skin, in the bell's own units: ring n, th round
// (0..1); inner is the subumbrella, under the jelly. Also gives the normal
// (outward and up) in w-less form.
struct Skin {
    pos: vec3<f32>,
    normal: vec3<f32>,
    stroke: f32,
};

fn moon_skin(j: u32, J: Jelly, n: u32, th: f32, inner: bool) -> Skin {
    let pr = item(PROFILE0 + j * NODES + n);
    let s = f32(n) / f32(RINGS);
    let ang = th * TAU;
    let lob = moon_lobe(th, s);
    let ca = cos(ang);
    let sa = sin(ang);
    var k: Skin;
    k.normal = vec3<f32>(sin(pr.z) * ca, cos(pr.z), sin(pr.z) * sa);
    k.pos = vec3<f32>(pr.x * lob * ca, pr.y, pr.x * lob * sa);
    // The margin never moves quite as one: a slow ripple round it.
    let ripple = noise(vec2<f32>(th * 7.0 + f32(j) * 13.0, V.time * 0.6 + f32(j))) - 0.5;
    k.pos.y += 0.05 * ripple * s * s * s;
    if (inner) {
        k.pos -= k.normal * moon_thick(s);
    }
    k.stroke = pr.w;
    return k;
}

// One frame of jelly j: spawn, strokes, swimming, steering, its light.
fn jelly_step(j: u32) -> Jelly {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    var J = jelly_at(j);
    let n = jelly_count();
    if (j >= n) {
        var none: Jelly;
        return none;
    }
    let k = kind();
    let sp = species_of(k);
    let id = f32(j);
    let h1 = creature_hash(id, 1.0);
    let h2 = creature_hash(id, 2.0);
    let h3 = creature_hash(id, 3.0);
    if (J.age < 0.5) {
        J.pos = (vec3<f32>(h1, h2, h3) * 2.0 - 1.0) * TANK * 0.8;
        J.vel = vec3<f32>(0.0);
        J.axis = normalize(vec3<f32>(h2 - 0.5, 1.2, h3 - 0.5));
        J.spin = h1 * TAU;
        J.size = 0.8 + 0.4 * h3;
        J.hue = id / f32(n);
        J.since = h2 * max(sp.period, 1.0);
        J.age = 1.0;
        J.last_beat = V.beat_count;
        return J;
    }
    J.age = min(J.age + 1.0, 10.0);
    let env = item(1u);
    let ev = item(2u);
    let mk = music();
    let mode = P.music;
    let hit = V.hits.w > 0.5 && ev.x < 0.5;
    let low_hit = V.hits.x > 0.5 && ev.y < 0.5;
    let drop = mode > 0.5 && V.onsets.x > 0.85 && V.swell > 1.0 && ev.z < 0.3;
    let R = radius(J);

    // Strokes: on its own rhythm; with the music, on a beat once it is
    // ready, in Performance also on bass hits; everyone on a drop.
    if (sp.period > 0.0) {
        // A moon jelly that wants to go down pulses seldom and sinks (it is a little heavier than water).
        let sinking = k == MOON && wander_point(id, TANK * 0.8, 0.02 + 0.03 * P.drift).y < J.pos.y - 0.2;
        let period = sp.period * mix(0.85, 1.25, h1) * select(1.0, 2.6, sinking);
        var go = J.since > period;
        let beat = V.beat_count != J.last_beat && V.confidence > 0.3;
        if (mode > 0.5 && beat && J.since > period * select(0.6, 0.35, mode > 1.5)) {
            go = true;
        }
        if (mode > 1.5 && low_hit) {
            go = true;
        }
        if (drop) {
            go = true;
        }
        if (go && J.since > 0.3) {
            J.since = 0.0;
        }
    }
    J.last_beat = V.beat_count;
    J.since = min(J.since + dt, 60.0);

    // Where to go: its own wandering spot, apart from the others, back from the walls.
    let home = wander_point(id, TANK * 0.8, 0.02 + 0.03 * P.drift);
    // The pull toward the spot is at most 1, so keeping apart always wins up close.
    let to_home = home - J.pos;
    var toward = to_home / max(length(to_home), 1.0);
    for (var o = 0u; o < n; o++) {
        if (o != j) {
            toward += keep_apart(J.pos, item(JELLY0 + 5u * o).xyz, R * 6.0) * 3.0;
        }
    }
    let outside = max(max(abs(J.pos.x) / TANK.x, abs(J.pos.y) / TANK.y), abs(J.pos.z) / TANK.z) > 0.92;
    if (outside) {
        toward = -J.pos;
    }
    // Bells swim up their axis: to go down they stop swimming and sink, so
    // the axis aims up and across; the comb jelly simply heads there.
    let down = toward.y < -0.2 * length(toward);
    var want = normalize(toward + vec3<f32>(0.0, 1e-4, 0.0));
    if (k != COMB) {
        // A moon jelly leans into a turn but stays within about 35 degrees of upright.
        let upright = select(0.6, 1.5, k == MOON);
        want = normalize(vec3<f32>(toward.x, max(toward.y, 0.0), toward.z) + vec3<f32>(0.0, upright * length(toward) + 0.1, 0.0));
    }
    J.axis = turn_toward(J.axis, want, 1.0 - exp(-dt * (0.25 + 0.35 * P.drift)));
    J.spin += dt * (h2 - 0.5) * 0.25;

    var acc = vec3<f32>(0.0, -0.03, 0.0);
    var drag = 1.3;
    if (k == MOON) {
        // Thrust is the water the bell pushes out as it closes, and a smaller
        // push as it opens again (the stopping vortex under it: passive
        // energy recapture), then it glides through the pause.
        let rate = (elastic_stroke(J.since, MOON_SQUEEZE, MOON_SPRING) - elastic_stroke(J.since - dt, MOON_SQUEEZE, MOON_SPRING)) / max(dt, 1e-4);
        let push = max(rate, 0.0) + 0.35 * max(-rate, 0.0);
        acc += J.axis * push * sp.swim * P.swim * 0.2 * (1.0 + 0.6 * env.x * mk) * select(1.0, 0.3, down);
        acc.y -= 0.04;
        drag = 0.9;
    } else if (k == COMB) {
        acc += J.axis * sp.swim * P.swim * 0.25;
    } else if (J.since < 0.3) {
        acc += J.axis * sp.swim * P.swim * 1.1 * (1.0 + 0.6 * env.x * mk) * select(1.0, 0.25, down);
    }
    if (down) {
        acc.y -= 0.05;
    }
    J.vel = J.vel * exp(-dt * drag) + acc * dt * (R / BELL_R);
    let flow = current(J.pos, V.time) * 0.05 * P.current * (0.6 + 0.4 * env.w * mk);
    J.pos = clamp(J.pos + (J.vel + flow) * dt, -TANK * 1.1, TANK * 1.1);

    // Its light.
    let window = fract(V.time * 1.3) < dt * 1.3;
    switch (k) {
        case 1u: {
            J.glow = 0.35 + env.y * mk;
            J.flash = max(J.flash * exp(-dt / 0.35), V.onsets.y * mk * 0.8);
        }
        case 2u: {
            J.glow = 0.3 + env.z * mk;
            J.flash *= exp(-dt / 0.6);
            let by_music = hit && hash21(vec2<f32>(V.beat_count + V.time, id)) < 0.55;
            let by_itself = window && hash21(vec2<f32>(floor(V.time * 1.3), id)) < 0.12;
            if (select(by_music, by_itself, mode < 0.5)) {
                J.flash = 1.0;
            }
        }
        case 3u: {
            J.flash *= exp(-dt / 2.5);
            let strong = V.onsets.x > 0.9 && V.energy > 1.2 && low_hit && h3 < 0.5;
            let rare = window && hash21(vec2<f32>(floor(V.time * 1.3), id + 50.0)) < 0.015;
            if (drop || (mode > 0.5 && strong) || rare) {
                J.flash = 1.0;
            }
            J.pa = (J.pa + dt * (0.12 + 1.6 * J.flash)) % 64.0;
            J.glow = 0.25 + 0.6 * J.flash;
        }
        case 4u: {
            J.pa = (J.pa + dt * (0.7 + 2.2 * env.z * mk)) % 64.0;
            J.glow = 0.45 + env.z * mk * 0.8;
        }
        case 5u: {
            J.pa = (J.pa + dt * (0.25 + 1.2 * env.w * mk)) % 64.0;
            J.glow = 0.4 + env.w * mk * 0.7;
        }
        default: {
            J.glow = 0.35 + 0.25 * sin(V.time * 0.5 + h1 * TAU) + env.x * mk * 0.9;
        }
    }
    return J;
}

// ---------------------------------------------------------------- strands and arms

fn strand_count() -> u32 {
    let P = params();
    if (P.tentacles < 0.01) {
        return 0u;
    }
    return clamp(u32(round(species_of(kind()).strands * P.tentacles)), 1u, STRANDS);
}

// Where strand s (of n) leaves the bell, in the bell's own units.
fn strand_root(J: Jelly, s: u32, n: u32) -> vec3<f32> {
    let k = kind();
    if (k == COMB) {
        // Two tentacles from the sides, halfway down.
        return bell_local(k, 0.55, select(0.0, 0.5, s == 1u), 0.0) * 0.9;
    }
    var th = (f32(s) + 0.5) / f32(max(n, 1u));
    var t = 1.0;
    if (k == LION) {
        // Hair in eight clusters under the lobes.
        let cluster = s % 8u;
        let within = f32(s / 8u) / f32(max(n / 8u, 1u));
        th = (f32(cluster) + 0.5 + (within - 0.5) * 0.7) / 8.0;
        t = 0.88 + 0.12 * fract(f32(s) * 0.618);
    }
    return bell_local(k, t, th, local_stroke(J, t));
}

fn strand_length(s: u32) -> f32 {
    let sp = species_of(kind());
    // The atolla trails one long tentacle.
    let long = kind() == ATOLLA && s == 0u;
    // Each strand its own length, so the ends are ragged, not a cut edge.
    let ragged = select(mix(0.55, 1.0, fract(f32(s) * 0.618 + 0.3)), 1.0, kind() == COMB);
    return sp.strand_len * select(1.0, 7.0, long) * ragged * params().reach;
}

// The direction strands hang: away from the bell, a little down, behind
// its motion, with the current, bent by a sway wave running down them.
fn hang_dir(J: Jelly, at: vec3<f32>, k: u32, s: u32, j: u32, amp: f32) -> vec3<f32> {
    let P = params();
    let hang = normalize(-J.axis * 0.75 + vec3<f32>(0.0, -0.45, 0.0) - J.vel * 2.0 + current(at, V.time) * 0.15 * P.current);
    let e1 = perpendicular(hang);
    let e2 = cross(hang, e1);
    let w = V.time * 1.6 - f32(k) * 0.38 + f32(s) * 0.9 + f32(j) * 2.3;
    return normalize(hang + (e1 * sin(w) + e2 * cos(w * 1.3 + f32(s))) * amp);
}

fn strand_step(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let idx = i - STRAND0;
    let k = idx % SEG;
    let s = (idx / SEG) % STRANDS;
    let j = idx / (SEG * STRANDS);
    let J = jelly_at(j);
    let n = strand_count();
    let moon = kind() == MOON;
    var anchor = to_world(J, strand_root(J, s, n));
    // The moon jelly's fringe hangs from the underside of the margin and
    // carries on the way the margin points: curled in on a stroke, splayed
    // out as it flares.
    let th = (f32(s) + 0.5) / f32(max(n, 1u));
    let edge = moon_skin(j, J, RINGS, th, true);
    let pr = item(PROFILE0 + j * NODES + RINGS);
    let ang = th * TAU;
    let out_dir = dir_to_world(J, vec3<f32>(cos(pr.z) * cos(ang), -sin(pr.z), cos(pr.z) * sin(ang)));
    if (moon) {
        anchor = to_world(J, edge.pos);
    }
    if (j >= jelly_count() || s >= n) {
        return vec4<f32>(anchor, 0.0);
    }
    let len = strand_length(s) * radius(J) / f32(SEG);
    let hair = kind() == LION;
    let amp = 0.3 * P.sway * (0.5 + 0.5 * item(1u).y * music()) * select(1.0, 0.6, hair || moon);
    // Neighbouring fringe tentacles sway out of step, not as one comb.
    var rest = hang_dir(J, anchor, k, s, j, amp);
    let u = f32(k) / f32(SEG - 1u);
    if (moon) {
        // A fringe tentacle sways side to side along the margin, each out of
        // step with its neighbours (a sway turning round it would coil it).
        let hang = normalize(-J.axis * 0.75 + vec3<f32>(0.0, -0.45, 0.0) - J.vel * 2.0 + current(anchor, V.time) * 0.15 * P.current);
        let side = normalize(cross(hang, out_dir) + vec3<f32>(1e-6, 0.0, 0.0));
        let w = V.time * 1.1 - f32(k) * 0.18 + creature_hash(f32(s), 24.0 + f32(j)) * TAU;
        rest = normalize(hang + side * sin(w) * amp);
        // Near the root it follows the margin; further down the water and its weight take it.
        let follow = mix(0.75, 0.1, u);
        let h = vec3<f32>(creature_hash(f32(s), 21.0 + f32(j)), creature_hash(f32(s), 22.0 + f32(j)), creature_hash(f32(s), 23.0 + f32(j))) - 0.5;
        rest = normalize(out_dir * follow + rest * (1.0 - 0.4 * follow) + h * 0.45);
    }
    if (J.age < 1.5) {
        return vec4<f32>(anchor + rest * len * f32(k + 1u), 0.0);
    }
    let parent = select(item(i - 1u).xyz, anchor, k == 0u);
    // A fringe tentacle is stiff near the root and limp at the tip, so a bend runs down it.
    let rate = select(select(3.0, 1.6, hair), mix(4.5, 2.2, u), moon);
    let p = chain_link(item(i).xyz, parent, rest, 1.0 - exp(-dt * rate), len);
    return vec4<f32>(p, 0.0);
}

fn arm_step(i: u32) -> vec4<f32> {
    let P = params();
    let dt = clamp(V.dt, 0.0, 0.1);
    let idx = i - ARM0;
    let k = idx % ARM_SEG;
    let a = (idx / ARM_SEG) % ARMS;
    let j = idx / (ARM_SEG * ARMS);
    let J = jelly_at(j);
    let moon = kind() == MOON;
    // A moon jelly's arms are perradial: under the rhopalia, between the gonads.
    let th = (f32(a) + select(0.5, 0.0, moon)) / f32(ARMS);
    var root = vec3<f32>(0.12 * cos(th * TAU), 0.05, 0.12 * sin(th * TAU));
    if (moon) {
        // From the mouth, under the thick middle of the bell.
        root = vec3<f32>(0.05 * cos(th * TAU), MOON_CROWN - moon_thick(0.0) - 0.02, 0.05 * sin(th * TAU));
    }
    let anchor = to_world(J, root);
    let len = species_of(kind()).arm_len * P.arms * radius(J) / f32(ARM_SEG);
    if (j >= jelly_count() || len <= 0.0) {
        return vec4<f32>(anchor, 0.0);
    }
    // Arms hang under the bell; a sea nettle's spiral slowly round each other.
    let twist = select(0.0, f32(k) * 0.45 + V.time * 0.3, kind() == NETTLE) + f32(a) * 1.5708;
    let ex = frame_x(J);
    let ez = cross(J.axis, ex);
    let amp = 0.2 * P.sway * (0.5 + 0.5 * item(1u).y * music());
    var wobble = vec3<f32>(sin(V.time * 0.9 + f32(k) * 0.5 + f32(a)), 0.0, cos(V.time * 0.7 + f32(k) * 0.4)) * amp;
    if (moon) {
        // A moon jelly's arms billow: a slow wave runs down each, every arm its own.
        let ph = f32(a) * 1.7 + f32(j) * 2.9;
        wobble = (ex * sin(V.time * 0.7 - f32(k) * 0.42 + ph) + ez * cos(V.time * 0.55 - f32(k) * 0.33 + ph * 1.3)) * (0.25 + 0.5 * amp);
    }
    let splay = select(0.3, 0.22, moon);
    let rest = normalize(-J.axis + vec3<f32>(0.0, -0.3, 0.0) - J.vel * 2.0 + (ex * cos(twist) + ez * sin(twist)) * splay + current(anchor, V.time) * 0.1 * P.current + wobble);
    if (J.age < 1.5) {
        return vec4<f32>(anchor + rest * len * f32(k + 1u), 0.0);
    }
    let parent = select(item(i - 1u).xyz, anchor, k == 0u);
    let u = f32(k) / f32(ARM_SEG - 1u);
    let rate = select(2.0, mix(5.0, 1.5, u), moon);
    return vec4<f32>(chain_link(item(i).xyz, parent, rest, 1.0 - exp(-dt * rate), len), 0.0);
}

// The sound, smoothed and bounded: what sways them, never jolts them.
fn sound_step() -> vec4<f32> {
    let dt = clamp(V.dt, 0.0, 0.1);
    let last = item(1u);
    let want = vec4<f32>(
        clamp((V.bass - 0.8) * 1.2 + V.onsets.x * 0.5, 0.0, 2.0),
        clamp((V.mid - 0.8) * 1.2 + V.onsets.y * 0.4, 0.0, 2.0),
        clamp((V.treble - 0.8) * 1.2 + V.onsets.z * 0.6, 0.0, 2.0),
        clamp(V.energy - 0.6, 0.0, 2.0)
    );
    let up = vec4<f32>(vec3<f32>(1.0 - exp(-dt / 0.08)), 1.0 - exp(-dt / 1.2));
    let down = vec4<f32>(vec3<f32>(1.0 - exp(-dt / 0.35)), 1.0 - exp(-dt / 1.2));
    return last + (want - last) * select(down, up, want > last);
}

fn prepare(i: u32) -> vec4<f32> {
    if (i >= PROFILE0) {
        return profile_step(i);
    }
    if (i >= ARM0) {
        return arm_step(i);
    }
    if (i >= STRAND0) {
        return strand_step(i);
    }
    if (i >= JELLY0) {
        let J = jelly_step((i - JELLY0) / 5u);
        switch ((i - JELLY0) % 5u) {
            case 0u: { return vec4<f32>(J.pos, J.since); }
            case 1u: { return vec4<f32>(J.vel, 0.0); }
            case 2u: { return vec4<f32>(J.axis, J.spin); }
            case 3u: { return vec4<f32>(J.size, J.hue, J.age, J.last_beat); }
            default: { return vec4<f32>(J.glow, J.flash, J.pa, J.pb); }
        }
    }
    switch (i) {
        case 1u: { return sound_step(); }
        case 3u: { return follow_step(); }
        case 2u: {
            let ev = item(2u);
            let drop = V.onsets.x > 0.85 && V.swell > 1.0 && ev.z < 0.3;
            let env = select(ev.z * exp(-clamp(V.dt, 0.0, 0.1) / 2.0), 1.0, drop);
            return vec4<f32>(select(0.0, 1.0, V.hits.w > 0.5), select(0.0, 1.0, V.hits.x > 0.5), env, 0.0);
        }
        default: { return vec4<f32>(1.0, 0.0, 0.0, 0.0); }
    }
}

// ---------------------------------------------------------------- tentacles and hair

fn point(i: u32) -> LinePoint {
    let P = params();
    let k = i % SEG;
    let s = (i / SEG) % STRANDS;
    let j = i / (SEG * STRANDS);
    let J = jelly_at(j);
    let sp = species_of(kind());
    let cam = the_camera();
    let p3 = item(STRAND0 + i).xyz;
    let pr = project(cam, p3);
    var q: LinePoint;
    q.pos = pr.xy;
    let on = j < jelly_count() && s < strand_count() && J.age > 2.5 && k + 1u < SEG && pr.z > 0.0;
    let u = f32(k) / f32(SEG - 1u);
    let scale = pr.z / (cam.focal / cam.dist);
    q.width = select(0.0, sp.strand_px * scale * sqrt(P.size * J.size) * mix(1.0, 0.35, u), on);
    var light = (0.35 + 0.5 * J.glow) * (1.0 - 0.7 * u) * select(1.0, 0.35, kind() == LION);
    switch (kind()) {
        case 5u: {
            // Waves of light running down the hair.
            light *= 0.2 + 4.0 * pow(0.5 + 0.5 * sin(u * 12.0 - J.pa * 6.0 + f32(s / 8u) * 0.4), 4.0) * J.glow;
        }
        case 3u: {
            light *= select(0.7, 1.0, s == 0u);
        }
        case 2u: {
            light *= 0.1 + J.flash * 0.5;
        }
        case 4u: {
            light *= 0.4;
        }
        case 0u: {
            // A fringe of a hundred and sixty: each one faint.
            light *= 0.45;
        }
        default: {}
    }
    q.colour = tone(2u, J.hue + 0.1 * u) * light * depth_fade(cam, p3, cam.dist - 1.0, 0.35) * P.glow;
    return q;
}

// ---------------------------------------------------------------- bells and arms

fn vertex(vi: u32) -> MeshVertex {
    let P = params();
    let j = vi / CORNERS_J;
    let l = vi % CORNERS_J;
    let J = jelly_at(j);
    var v: MeshVertex;
    v.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    if (j >= jelly_count() || J.age < 1.5) {
        return v;
    }
    let cam = the_camera();
    let k = kind();
    let tri = l / 3u;
    let corner = l % 3u;
    // Corner offsets of a quad's two triangles.
    var da = array<u32, 6>(0u, 1u, 1u, 0u, 1u, 0u);
    var db = array<u32, 6>(0u, 0u, 1u, 0u, 1u, 1u);
    let c6 = (tri % 2u) * 3u + corner;
    if (l < BELL_CORNERS && k == MOON) {
        // The moon jelly's two skins: the outer (exumbrella) and, under the
        // jelly, the inner (subumbrella) that carries what shows through.
        let shell = l / SHELL_CORNERS;
        let quad = (l % SHELL_CORNERS) / 6u;
        let n = quad / SECTORS + da[c6];
        let th = f32(quad % SECTORS + db[c6]) / f32(SECTORS);
        let sk = moon_skin(j, J, n, th, shell == 1u);
        let wp = to_world(J, sk.pos);
        let wn = dir_to_world(J, sk.normal);
        v.pos = clip_pos(cam, wp);
        v.data = vec4<f32>(f32(n) / f32(RINGS), th, facing(cam, wp, wn), f32(j) + 0.5 + 64.0 * f32(shell));
        // The plan position (crown in the middle, margin 1 out), which
        // interpolates cleanly where the angle round would kink.
        let plan = f32(n) / f32(RINGS) * vec2<f32>(cos(th * TAU), sin(th * TAU));
        v.colour = vec4<f32>(depth_fade(cam, wp, cam.dist - 1.0, 0.35), sk.stroke, plan);
        return v;
    }
    if (l < BELL_CORNERS) {
        if (l >= SHELL_CORNERS) {
            return v;
        }
        let quad = tri / 2u;
        let t = f32(quad / SECTORS + da[c6]) / f32(RINGS);
        let th = f32(quad % SECTORS + db[c6]) / f32(SECTORS);
        let c = local_stroke(J, t);
        let lp = bell_local(k, t, th, c);
        let tt = clamp(t, 0.03, 0.97);
        let along = bell_local(k, tt + 0.02, th, local_stroke(J, tt + 0.02)) - bell_local(k, tt - 0.02, th, local_stroke(J, tt - 0.02));
        let round = bell_local(k, tt, th + 0.01, c) - bell_local(k, tt, th - 0.01, c);
        let wp = to_world(J, lp);
        let wn = dir_to_world(J, cross(along, round));
        v.pos = clip_pos(cam, wp);
        v.data = vec4<f32>(t, th, facing(cam, wp, wn), f32(j) + 0.5);
        v.colour = vec4<f32>(depth_fade(cam, wp, cam.dist - 1.0, 0.35), c, 0.0, 0.0);
        return v;
    }
    // An oral arm: a sheet along its links, folded into a V that opens away
    // from the axis, its lips ruffled (crenulated) and the whole arm twisting
    // a little down its length.
    let sp = species_of(k);
    if (sp.arm_len <= 0.0 || P.arms < 0.01) {
        return v;
    }
    let quad = (l - BELL_CORNERS) / 6u;
    let per_arm = (ARM_SEG - 1u) * ARM_ACROSS;
    let a = quad / per_arm;
    let link = (quad % per_arm) / ARM_ACROSS;
    let kk = link + da[c6];
    let xi = (quad % ARM_ACROSS) + db[c6];
    let x = f32(xi) / f32(ARM_ACROSS) * 2.0 - 1.0;
    let base = ARM0 + (j * ARMS + a) * ARM_SEG;
    let at = item(base + kk).xyz;
    let dir = normalize(item(base + min(kk + 1u, ARM_SEG - 1u)).xyz - item(base + max(kk, 1u) - 1u).xyz + vec3<f32>(1e-6, 0.0, 0.0));
    let u = f32(kk) / f32(ARM_SEG - 1u);
    let moon = k == MOON;
    let tha = ((f32(a) + select(0.5, 0.0, moon)) / f32(ARMS) + u * 0.08) * TAU;
    let radial = dir_to_world(J, vec3<f32>(cos(tha), 0.0, sin(tha)));
    let e_out = normalize(radial - dir * dot(radial, dir) + vec3<f32>(0.0, 1e-6, 0.0));
    let e_side = cross(dir, e_out);
    let width = sp.arm_w * radius(J) * P.arms * select(mix(1.0, 0.45, u), mix(1.0, 0.25, u * u), moon);
    let fold = select(0.25, 0.55, moon);
    // The lips: a fast ruffle and a slower, irregular wave, strongest at the edges.
    let lip = x * x;
    let ruffle = sin(u * 9.0 + f32(a) * 2.3 + V.time * 0.9) * 0.6 + sin(u * 4.0 - V.time * 0.5 + f32(a)) * 0.4;
    let wp = at + e_side * x * width + e_out * (abs(x) * fold + lip * ruffle * 0.35) * width;
    let wn = normalize(cross(dir, e_side + e_out * fold * sign(x)) + vec3<f32>(0.0, 1e-6, 0.0));
    v.pos = clip_pos(cam, wp);
    v.data = vec4<f32>(u, x, facing(cam, wp, wn), f32(j) + 16.5);
    v.colour = vec4<f32>(depth_fade(cam, wp, cam.dist - 1.0, 0.35), 0.0, 0.0, 0.0);
    return v;
}

// The species' own light on its bell at (t, th).
fn bell_light(J: Jelly, t: f32, th: f32) -> vec3<f32> {
    let ang = th * TAU;
    switch (kind()) {
        case 1u: {
            let stripes = pow(0.5 + 0.5 * cos(ang * 16.0), 6.0) * smoothstep(0.1, 0.3, t);
            return tone(1u, J.hue) * stripes * (0.5 + J.glow * 1.8 + J.flash * 1.2);
        }
        case 2u: {
            let canals = pow(0.5 + 0.5 * cos(ang * 96.0), 80.0) * smoothstep(0.2, 0.95, t) * 0.12;
            let dots = pow(0.5 + 0.5 * cos(ang * 48.0), 4.0) * smoothstep(0.8, 0.99, t);
            return tone(0u, J.hue) * canals * (0.3 + J.glow) + tone(1u, J.hue) * dots * (1.5 + 2.0 * J.glow + J.flash * 10.0);
        }
        case 3u: {
            let rim = smoothstep(0.9, 0.99, t);
            let dots = pow(0.5 + 0.5 * cos(ang * 20.0), 8.0) * rim * 0.5;
            let d = fract(th - J.pa);
            let wheel = exp(-d * 8.0) * rim * J.flash * 8.0;
            return tone(1u, J.hue) * (dots + wheel);
        }
        case 4u: {
            let m = abs(fract(th * 8.0 + 0.5) - 0.5);
            let row = exp(-m / 0.05) * smoothstep(0.1, 0.2, t) * smoothstep(0.9, 0.8, t);
            let plates = pow(0.5 + 0.5 * cos(t * TAU * 22.0 - J.pa * TAU * 1.5), 3.0);
            let irid = select(colour(t - J.pa * 0.2), spectrum(t * 2.0 - J.pa * 0.5 + th), params().colours > 0.5);
            return irid * row * plates * (0.8 + J.glow * 2.5);
        }
        case 5u: {
            let folds = pow(0.5 + 0.5 * cos(ang * 32.0), 8.0) * 0.5 * t;
            let heart = exp(-t * 3.0) * 0.6;
            return tone(1u, J.hue) * (folds + heart) * (0.5 + J.glow * 1.5);
        }
        default: {
            // Four horseshoe gonads seen through the bell, and faint canals.
            let q = t * vec2<f32>(cos(ang), sin(ang));
            var g = 0.0;
            for (var n = 0u; n < 4u; n++) {
                let a = (f32(n) + 0.5) * 1.5708;
                let c = 0.36 * vec2<f32>(cos(a), sin(a));
                let off = q - c;
                let d = abs(length(off) - 0.13);
                let open = smoothstep(0.5, -0.1, dot(normalize(off + vec2<f32>(1e-5, 0.0)), normalize(c)));
                g += max(exp(-d / 0.04) - 0.01, 0.0) * open;
            }
            let canals = pow(0.5 + 0.5 * cos(ang * 16.0), 40.0) * 0.2 * t;
            return tone(1u, J.hue) * (g * (0.8 + J.glow * 2.2) + canals);
        }
    }
}

// A line of half-width w at distance d, widened to at least a pixel (fw)
// and dimmed to match, so fine lines fade instead of shimmering.
fn thin_line(d: f32, w: f32, fw: f32) -> f32 {
    let ww = max(w, fw);
    return smoothstep(ww, 0.0, d) * (w / ww);
}

// The moon jelly's skin at s (crown 0 to margin 1), th round (0..1). It is
// light that the jelly itself gives off (like the fluorescence in aquarium
// photographs), more where the eye looks through more of it: the thick
// middle, and the thin skin seen edge on. The inner skin carries what shows
// through the glass: four horseshoe gonads round the stomach, sixteen radial
// canals (eight forking on their way out) joining the ring canal, and the
// fine rings of the swimming muscle, which light as it squeezes. fs and fth
// are one pixel in s and th.
fn moon_paint(J: Jelly, s_in: f32, th_in: f32, face: f32, inner: bool, squeeze: f32, plan: vec2<f32>, fs: f32, fth: f32, fp: f32) -> vec3<f32> {
    let P = params();
    var s = s_in;
    var th = th_in;
    if (inner) {
        s = length(plan);
        th = fract(atan2(plan.y, plan.x + 1e-7) / TAU);
    }
    let ang = th * TAU;
    // Deeper blue through the thick middle, paler and a touch greener toward the rim.
    let body = tone(0u, J.hue) * mix(vec3<f32>(0.78, 0.82, 1.1), vec3<f32>(0.92, 1.08, 1.05), smoothstep(0.2, 1.0, s));
    let rim = fresnel(face, 2.5);
    let through = min((moon_thick(s) + 0.03) / max(face, 0.15), 2.0);
    let seed = vec2<f32>(J.hue * 13.0, J.hue * 7.0);
    if (!inner) {
        // The jelly is not quite even: a soft mottling through it.
        let tissue = 0.75 + 0.5 * fbm(plan * 6.0 + seed, 4);
        var c = body * (0.018 * through * tissue + 0.3 * fresnel(face, 3.0) * smoothstep(0.1, 0.7, s)) * P.body;
        // Warts: clusters of stinging cells, tiny pale dots over the outer skin.
        let w = cells(plan * 55.0 + seed, 1.0, 0.0);
        let wart = smoothstep(0.17, 0.05, w.x) * step(0.55, w.z) * keep_detail(fp * 55.0);
        c += body * wart * 0.1 * P.body;
        // The thin margin glows, brightest on the lobes.
        let x = fract(th * 8.0);
        let lobe = 0.5 - 0.5 * cos(x * TAU);
        c += body * thin_line(1.0 - s, 0.012, fs) * (0.5 + 0.5 * lobe) * 0.9 * P.body;
        // Eight rhopalia, bright beads in the notches.
        let d = vec2<f32>(min(x, 1.0 - x) / 8.0 * TAU * s, 1.0 - s - 0.02);
        let rh = max(exp(-dot(d, d) / (0.012 * 0.012)) - 0.02, 0.0);
        c += mix(body, vec3<f32>(1.0), 0.6) * rh * (1.5 + 2.0 * J.glow) * P.light;
        return c;
    }
    var c = body * (0.012 + 0.08 * rim + 0.012 * fbm(plan * 10.0 + seed, 3)) * P.body;
    let gon = tone(1u, J.hue);
    let canal = mix(body, gon, 0.2) + vec3<f32>(0.1);
    let q = plan;
    let fq = fp;
    // The stomach in the middle, a four-cornered mouth.
    let st = smoothstep(0.15, 0.05, s * (1.0 + 0.3 * cos(ang * 4.0)));
    c += mix(body, gon, 0.4) * st * 0.07;
    // The gonads: four ruffled horseshoes, open toward the middle, a ribbon
    // of folded, lumpy tissue with a brighter fold along it.
    var g = 0.0;
    for (var n = 0u; n < 4u; n++) {
        let a = (f32(n) + 0.5) * 1.5708;
        let dir = vec2<f32>(cos(a), sin(a));
        let off = q - dir * 0.31;
        let l = length(off);
        let ring = 0.125 + 0.022 * (noise(off * 13.0 + f32(n) * 7.0) - 0.5);
        let open = smoothstep(-0.6, -0.2, dot(off / max(l, 1e-4), dir));
        let d = abs(l - ring);
        g += (thin_line(d, 0.03, fq) * 0.75 + thin_line(abs(d - 0.022), 0.006, fq) * 0.5) * open;
        g += smoothstep(ring, 0.0, l) * 0.04 * open;
    }
    let lumps = cells(q * 80.0 + seed, 1.0, V.time * 0.2);
    let folds = mix(0.6, 1.15, smoothstep(0.0, 0.18, lumps.y - lumps.x));
    let grain = mix(1.0, folds * (0.75 + 0.5 * fbm(q * 40.0, 3)), keep_detail(fq * 80.0));
    c += gon * g * grain * (0.6 + J.glow * 1.8) * P.light;
    // Sixteen radial canals from the stomach out to the ring canal.
    let u = th * 16.0;
    let kc = floor(u + 0.5);
    let wig = (noise(vec2<f32>(s * 9.0, kc * 3.7)) - 0.5) * 0.02;
    var dx = abs((u - kc) / 16.0 * TAU * s + wig);
    if (u32(kc) % 2u == 0u) {
        // Under a rhopalium (per- and interradial): forking three times on the way out.
        dx = abs(dx - 0.06 * smoothstep(0.42, 0.6, s));
        dx = abs(dx - 0.03 * smoothstep(0.6, 0.76, s));
        dx = abs(dx - 0.015 * smoothstep(0.74, 0.88, s));
    }
    let span = smoothstep(0.14, 0.2, s) * smoothstep(0.95, 0.92, s);
    var canals = thin_line(dx, 0.005, fq) * span * mix(1.0, 0.55, s);
    canals += thin_line(abs(s - 0.93), 0.005, fs) * 0.8;
    c += canal * canals * (0.2 + 0.18 * J.glow) * P.light;
    // The swimming muscle: faint, uneven rings in the outer bell, lit by the squeeze.
    let freq = 46.0;
    let even = 0.4 + 1.2 * fbm(vec2<f32>(s * 25.0, th * 9.0) + seed, 2);
    let bands = mix(pow(0.5 + 0.5 * cos(s * TAU * freq), 4.0) * even, 0.3, clamp(fs * freq * 1.5, 0.0, 1.0));
    let zone = smoothstep(0.4, 0.5, s) * smoothstep(0.92, 0.85, s);
    c += body * bands * zone * (0.008 + 0.15 * max(squeeze, 0.0)) * P.light;
    return c;
}

// A moon jelly's oral arm at u (root 0 to tip 1), x across (-1..1): clear
// jelly, the light in its crenulated lips and a groove down its fold.
fn moon_arm_paint(J: Jelly, u: f32, x: f32, face: f32, fu: f32) -> vec3<f32> {
    let P = params();
    let c = mix(tone(3u, J.hue), tone(1u, J.hue), 0.15 * (1.0 - u));
    let rim = fresnel(face, 2.0);
    // The lips: crenulated, a fine scallop with tiny tentacle-like processes.
    let side = select(0.0, 9.0, x > 0.0);
    let scallop = 0.5 + 0.5 * sin(u * 150.0 + 7.0 * noise(vec2<f32>(u * 9.0, side)) + side);
    let lipline = 1.0 - 0.12 * scallop - abs(x);
    let lip = smoothstep(0.14, 0.0, lipline) * (0.6 + 0.4 * scallop);
    let beads = 0.5 + 0.5 * noise(vec2<f32>(u * 120.0, select(0.0, 9.0, x > 0.0)));
    let groove = thin_line(abs(x), 0.05, fu * 4.0) * 0.2;
    let tissue = 0.7 + 0.6 * fbm(vec2<f32>(u * 14.0, x * 2.5 + J.hue * 9.0), 3);
    let light = (0.014 + 0.08 * rim) * tissue + lip * beads * 0.4 + groove;
    return c * light * (1.0 - 0.6 * u) * (0.6 + 0.5 * J.glow) * P.body;
}

fn paint(f: MeshFragment) -> vec4<f32> {
    let P = params();
    // Derivatives first, while every pixel is still on the same path.
    let fs = fwidth(f.data.x);
    let fth = fwidth(f.data.y);
    let fp = length(fwidth(f.colour.zw));
    let arm = f.data.w > 16.0 && f.data.w < 64.0;
    let J = jelly_at(u32(f.data.w) % 16u);
    let fade = f.colour.x * P.glow;
    if (kind() == MOON) {
        if (arm) {
            return vec4<f32>(moon_arm_paint(J, f.data.x, f.data.y, f.data.z, fs) * fade, 0.0);
        }
        let c = moon_paint(J, f.data.x, f.data.y, f.data.z, f.data.w > 64.0, f.colour.y, f.colour.zw, fs, fth, fp);
        return vec4<f32>(c * fade, 0.0);
    }
    if (arm) {
        let u = f.data.x;
        let x = f.data.y;
        let edge = pow(abs(x), 2.5);
        let ruffle = 0.5 + 0.5 * sin(u * 45.0 + V.time * 2.0 + x * 6.0);
        let light = (0.12 + 0.6 * edge + 0.3 * ruffle * edge) * (1.0 - 0.7 * u) * (0.5 + 0.6 * J.glow);
        return vec4<f32>(tone(3u, J.hue + 0.05) * light * 0.6 * fade, 0.0);
    }
    let t = f.data.x;
    let th = f.data.y;
    // Glass: the edges seen side on glow, the faces stay clear.
    let rim = fresnel(f.data.z, 2.2);
    let glass = (0.03 + 0.35 * rim) * P.body * select(1.0, 0.6, kind() == COMB);
    var c = tone(0u, J.hue) * glass;
    // A glowing margin (not on the comb jelly, whose "margin" is a pole).
    if (kind() != COMB) {
        c += tone(0u, J.hue) * max(exp(-(1.0 - t) / 0.03) - 0.01, 0.0) * 0.25 * P.body;
    }
    c += bell_light(J, t, th) * P.light;
    return vec4<f32>(c * fade, 0.0);
}

// ---------------------------------------------------------------- the tank

// Deep water with light shafts from above, and plankton at three depths
// that shifts with the camera.
fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let cam = the_camera();
    let p = centred(uv);
    var c = vec3<f32>(0.0);
    if (P.water > 0.001) {
        let deep = select(colour(0.55) * 0.02, vec3<f32>(0.0, 0.012, 0.03), P.colours > 0.5);
        let near = select(colour(0.55) * 0.045, vec3<f32>(0.0, 0.035, 0.065), P.colours > 0.5);
        // Kept faint: the layer's levelling evens out its mean brightness, and
        // water filling the frame would otherwise be lifted to full brightness.
        c += mix(deep, near, smoothstep(-1.0, 1.0, p.y)) * P.water * 0.12;
        let sx = p.x + p.y * 0.35 + cam.yaw * 0.8;
        let shafts = pow(noise(vec2<f32>(sx * 2.5, V.time * 0.04)), 5.0) * smoothstep(-0.3, 1.0, p.y);
        c += near * shafts * 0.3 * P.water;
    }
    if (P.plankton > 0.001) {
        for (var l = 0u; l < 3u; l++) {
            let depth = f32(l) + 1.0;
            let scale = 5.0 + 6.0 * depth;
            let q = (p + vec2<f32>(cam.yaw * (1.2 - 0.3 * depth), -V.time * 0.01 / depth)) * scale;
            let cell = floor(q);
            let hh = hash22(cell + f32(l) * 31.0);
            if (hh.x > 0.35) {
                continue;
            }
            let at = cell + 0.2 + 0.6 * hash22(cell + 7.0 + f32(l) * 13.0);
            let d = length(q - at);
            let r = 0.05 + 0.05 * hh.y;
            let twinkle = 0.5 + 0.5 * sin(V.time * (1.0 + 2.0 * hh.y) + hh.x * 40.0);
            let tint = select(colour(hh.y), vec3<f32>(0.6, 0.8, 1.0), P.colours > 0.5);
            c += tint * max(exp(-d * d / (r * r)) - 0.01, 0.0) * P.plankton * 0.02 * twinkle / depth;
        }
    }
    return c;
}
