// A jelly's life, once a frame in the prepare step: its stroke, how it
// swims and steers, its light, its bell's profile, and its glow in the water.

struct Jelly {
    pos: vec3<f32>,
    since: f32,
    vel: vec3<f32>,
    prev: f32,         // seconds into the stroke before this one (it hands over: stroke_handover)
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
    J.prev = a1.w;
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

// The bell's radius in world units.
fn radius(J: Jelly) -> f32 {
    return BELL_R * species_of(kind()).scale * params().size * J.size;
}

// The hue its colours start from (the palette's place, drifting with the music).
fn hue_of(J: Jelly) -> f32 {
    return J.hue + J.pb;
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

// ---------------------------------------------------------------- the bell

// How far a new stroke has taken over from the one before it, 0..1: the
// bell's shape crossfades from the old stroke (still playing out) to the
// new over a quarter second, so a stroke can begin at any moment, on any
// beat, and the bell never snaps from one shape to another.
fn stroke_handover(J: Jelly) -> f32 {
    return smoothstep(0.0, 0.25, J.since);
}

// The bell as this jelly strokes it: the species' bell, its curl and flare
// swelling with the bass (by Music), smoothly, as the sound is smoothed.
fn stroke_bell(k: u32) -> Bell {
    var b = bell_of(k);
    let strength = 1.0 + 0.35 * item(1u).x * music();
    b.curl *= strength;
    b.flare *= mix(1.0, strength, 0.6);
    return b;
}

// Node n of jelly j's bell profile this frame (a prepare item).
fn profile_step(i: u32) -> vec4<f32> {
    let idx = i - PROFILE0;
    let J = jelly_at(idx / NODES);
    let b = stroke_bell(kind());
    let old = bell_node(b, J.prev, idx % NODES, NODES);
    return mix(old, bell_node(b, J.since, idx % NODES, NODES), stroke_handover(J));
}

// Its flash as its light shows it. A crystal jelly's and an atolla's
// flashes are set off at full and fade; their light blooms in over the
// first moment (a fifth of a second, the atolla's alarm slower) instead
// of jumping. The others' flashes ease in already (jelly_step).
fn flash_of(J: Jelly) -> f32 {
    let k = kind();
    if (k == CRYSTAL || k == ATOLLA) {
        return J.flash * smoothstep(1.0, 0.72, J.flash);
    }
    return J.flash;
}

// A level easing toward `want`: up in `rise` seconds, down in `fall`, so a
// light swells and ebbs instead of jumping (living things do not strobe).
fn ease_to(level: f32, want: f32, dt: f32, rise: f32, fall: f32) -> f32 {
    return level + (want - level) * (1.0 - exp(-dt / select(fall, rise, want > level)));
}

// The groove clock (item GROOVE): what everything that waves runs on in
// place of the plain clock. It runs a little quicker as the music gets
// livelier (the sustained energy, the mids and a build, by Music), and it
// is added up frame by frame, so a change of pace never makes a wave jump.
// y is a soft beat: rising in 80 ms, fading over 0.4 s.
fn groove_step() -> vec4<f32> {
    let dt = clamp(V.dt, 0.0, 0.1);
    let g = item(GROOVE);
    let env = item(1u);
    let pace = 1.0 + (0.5 * env.w + 0.3 * env.y + 0.5 * build()) * music() * 0.6;
    return vec4<f32>(g.x + dt * pace, ease_to(g.y, V.beat * music(), dt, 0.08, 0.4), 0.0, 0.0);
}

fn groove_clock() -> f32 {
    return item(GROOVE).x;
}

fn profile(j: u32, n: u32) -> vec4<f32> {
    return item(PROFILE0 + j * NODES + min(n, RINGS));
}

// A point of jelly j's skin in the bell's own units: ring n, th round (0..1),
// inner the subumbrella.
fn jelly_skin(j: u32, n: u32, th: f32, inner: bool) -> BellPoint {
    let k = kind();
    let ripple = 0.05 * (noise(vec2<f32>(th * 7.0 + f32(j) * 13.0, groove_clock() * 0.6 + f32(j))) - 0.5);
    var p = bell_point(bell_of(k), profile(j, n), f32(n) / f32(RINGS), th, inner, select(ripple, 0.0, k == COMB));
    if (k == COMB) {
        // No strokes: a slow wobble runs down its waist instead.
        p.pos = vec3<f32>(p.pos.x, 0.0, p.pos.z) * (1.0 + 0.04 * sin(groove_clock() * 1.3 + f32(n) * 0.25)) + vec3<f32>(0.0, p.pos.y, 0.0);
    }
    return p;
}

// The skin's frame at s (0..1 from the crown), th round, in world
// directions: its normal, the way the profile runs out (along s) and the
// way round. Paint works it out per pixel from the profile, the angle
// interpolated smoothly between the rings (Catmull-Rom), so the shading has
// no seam at a ring even where a stroke curls the bell sharply (the mesh's
// corners alone would band it ring by ring).
struct SkinFrame {
    n: vec3<f32>,
    out: vec3<f32>,
    round: vec3<f32>,
};

fn skin_frame(J: Jelly, j: u32, s: f32, th: f32) -> SkinFrame {
    let b = bell_of(kind());
    let x = clamp(s, 0.0, 1.0) * f32(RINGS);
    let n0 = min(u32(x), RINGS - 1u);
    let t = x - f32(n0);
    let a0 = profile(j, max(n0, 1u) - 1u).z;
    let a1 = profile(j, n0).z;
    let a2 = profile(j, n0 + 1u).z;
    let a3 = profile(j, n0 + 2u).z;
    let a = a1 + 0.5 * t * (a2 - a0 + t * (2.0 * a0 - 5.0 * a1 + 4.0 * a2 - a3 + t * (3.0 * (a1 - a2) + a3 - a0)));
    let ang = th * TAU;
    let ca = cos(ang);
    let sa = sin(ang);
    let n2 = normalize(vec2<f32>(sin(a), cos(a) * b.aspect));
    let t2 = normalize(vec2<f32>(cos(a) * b.aspect, -sin(a)));
    var f: SkinFrame;
    f.n = dir_to_world(J, vec3<f32>(n2.x * ca, n2.y, n2.x * sa));
    f.out = dir_to_world(J, vec3<f32>(t2.x * ca, t2.y, t2.x * sa));
    f.round = dir_to_world(J, vec3<f32>(-sa, 0.0, ca));
    return f;
}

// ---------------------------------------------------------------- swimming

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
    let bell = bell_of(k);
    let T = tank();
    let id = f32(j);
    let h1 = creature_hash(id, 1.0);
    let h2 = creature_hash(id, 2.0);
    let h3 = creature_hash(id, 3.0);
    if (J.age < 0.5) {
        J.pos = (vec3<f32>(h1, h2, h3) * 2.0 - 1.0) * T * 0.8;
        J.vel = vec3<f32>(0.0);
        J.axis = normalize(vec3<f32>(h2 - 0.5, 1.2, h3 - 0.5));
        J.spin = h1 * TAU;
        J.size = 0.8 + 0.4 * h3;
        J.hue = id / f32(n);
        J.since = h2 * max(sp.period, 1.0);
        J.prev = 60.0;
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
    let drop = mode > 0.5 && new_drop(ev);
    let R = radius(J);
    // In a close-up the one the camera follows mostly hovers and shows itself.
    let hero = j == 0u && P.view > 0.5;
    let home = wander_point(id, T * 0.8, (0.02 + 0.03 * P.drift) * select(1.0, 0.5, hero));

    // Strokes: on its own rhythm; with the music, on a beat once it is
    // ready (in Reactive the bar's first beat, when the bar is sure), in
    // Performance also on bass hits; sooner in a build; everyone on a drop.
    // A jelly that wants to go down pulses seldom and sinks (it is a little
    // heavier than water).
    if (sp.period > 0.0) {
        let sinking = home.y < J.pos.y - 0.2;
        let period = sp.period * mix(0.85, 1.25, h1) * select(1.0, 2.6, sinking) * (1.0 - 0.3 * build() * min(mk, 1.0));
        var go = J.since > period;
        let on_bar = beat_in_bar() < 0.5 || bar_sure() < 0.4 || mode > 1.5;
        let beat = V.beat_count != J.last_beat && V.confidence > 0.3 && on_bar;
        if (mode > 0.5 && beat && J.since > period * select(0.6, 0.35, mode > 1.5)) {
            go = true;
        }
        if (mode > 1.5 && low_hit) {
            go = true;
        }
        if (drop) {
            go = true;
        }
        // A new stroke takes over from the last (stroke_handover), so it
        // may begin while the bell is still springing back.
        if (go && J.since > 0.25) {
            J.prev = J.since;
            J.since = 0.0;
        }
    }
    // Its colours glide along the palette, quicker on a beat.
    J.pb = fract(J.pb + dt * (0.004 + 0.2 * V.beat * mk));
    J.last_beat = V.beat_count;
    J.since = min(J.since + dt, 60.0);
    J.prev = min(J.prev + dt, 60.0);

    // Where to go: its own wandering spot, apart from the others, back from the walls.
    // The pull toward the spot is at most 1, so keeping apart always wins up close.
    let to_home = home - J.pos;
    var toward = to_home / max(length(to_home), 1.0);
    for (var o = 0u; o < n; o++) {
        if (o != j) {
            toward += keep_apart(J.pos, item(JELLY0 + 5u * o).xyz, R * 6.0) * 3.0;
        }
    }
    let outside = max(max(abs(J.pos.x) / T.x, abs(J.pos.y) / T.y), abs(J.pos.z) / T.z) > 0.92;
    if (outside) {
        toward = -J.pos;
    }
    // Bells swim up their axis: to go down they stop swimming and sink, so
    // the axis aims up and across, leaning into a turn but staying near
    // upright; the comb jelly simply heads there. Once a bell is really
    // sinking it tips over toward where it is going, the dome leading, so
    // its tentacles trail behind it instead of floating up round it; it
    // rights itself as it swims again.
    let down = toward.y < -0.2 * length(toward);
    var want = normalize(toward + vec3<f32>(0.0, 1e-4, 0.0));
    var tip = 0.0;
    if (k != COMB) {
        let upright = sp.upright * select(1.0, 3.0, hero);
        want = normalize(vec3<f32>(toward.x, max(toward.y, 0.0), toward.z) + vec3<f32>(0.0, upright * length(toward) + 0.1, 0.0));
        let speed = length(J.vel);
        let falling = smoothstep(0.3, 0.8, -J.vel.y / max(speed, 1e-5)) * smoothstep(0.03, 0.1, speed * BELL_R / R);
        tip = select(0.0, falling, down);
        want = normalize(mix(want, J.vel / max(speed, 1e-5), tip) + vec3<f32>(1e-4, 0.0, 0.0));
    }
    // Its frame round the axis is carried along as the axis turns (and
    // turned slowly by its own spin), then kept as the spin from
    // perpendicular()'s reference. That reference jumps where the axis
    // crosses a threshold (a jelly tipped far over), and the whole bell
    // turned on its axis in a frame; carried, it never does.
    let carried = frame_x(J);
    J.axis = turn_toward(J.axis, want, 1.0 - exp(-dt * (0.25 + 0.35 * P.drift + 0.3 * tip)));
    let ex = normalize(carried - J.axis * dot(carried, J.axis) + perpendicular(J.axis) * 1e-4);
    let e = perpendicular(J.axis);
    J.spin = atan2(dot(ex, cross(J.axis, e)), dot(ex, e)) + dt * (h2 - 0.5) * 0.25;

    var acc = vec3<f32>(0.0, -0.03, 0.0);
    var drag = 1.3;
    if (k == COMB) {
        // It swims by beating its combs: quicker with the highs.
        acc += J.axis * sp.swim * P.swim * 0.25 * (1.0 + 0.8 * env.z * mk);
    } else {
        // Thrust is the water the bell pushes out as it closes, and a smaller
        // push as it opens again (the stopping vortex under it: passive
        // energy recapture), then it glides through the pause.
        let rate = (elastic_stroke(J.since, bell.squeeze, bell.spring) - elastic_stroke(J.since - dt, bell.squeeze, bell.spring)) / max(dt, 1e-4) * stroke_handover(J);
        let push = max(rate, 0.0) + 0.35 * max(-rate, 0.0);
        acc += J.axis * push * sp.swim * P.swim * 0.2 * (1.0 + 0.6 * env.x * mk) * select(1.0, 0.3, down);
        acc.y -= 0.04;
        drag = 0.9;
    }
    if (down) {
        acc.y -= 0.05;
    }
    J.vel = J.vel * exp(-dt * drag) + acc * dt * (R / BELL_R);
    let flow = current(J.pos, V.time) * 0.05 * P.current * (0.6 + 0.4 * env.w * mk);
    J.pos = clamp(J.pos + (J.vel + flow) * dt, -T * 1.1, T * 1.1);

    // Its light: glow is what it sustains, flash what an event sets off and
    // lets fade, pa a phase for light that travels.
    let window = fract(V.time * 1.3) < dt * 1.3;
    switch (k) {
        case 1u: {
            // Stripes pulse with the mids, flash on the onsets there.
            J.glow = 0.35 + env.y * mk;
            J.flash = ease_to(J.flash, V.onsets.y * mk * 0.8, dt, 0.07, 0.35);
        }
        case 2u: {
            // Green rim lights: a flash round the ring on a strong low onset,
            // now and then on their own.
            J.glow = 0.3 + env.z * mk;
            J.flash *= exp(-dt / 0.6);
            let by_music = low_hit && V.onsets.x > 0.6 && hash21(vec2<f32>(V.beat_count + V.time, id)) < 0.55;
            let by_itself = window && hash21(vec2<f32>(floor(V.time * 1.3), id)) < 0.12;
            // (Only once the last has all but faded: starting over at full
            // restarts its bloom, and the light would drop out for a frame.)
            if (select(by_music, by_itself, mode < 0.5) && J.flash < 0.06) {
                J.flash = 1.0;
            }
            J.pa = (J.pa + dt * (0.1 + 0.8 * J.flash)) % 64.0;
        }
        case 3u: {
            // The alarm: on a drop the blue pinwheel spins round the rim.
            J.flash *= exp(-dt / 2.5);
            let strong = V.onsets.x > 0.9 && V.energy > 1.2 && low_hit && h3 < 0.5;
            let rare = window && hash21(vec2<f32>(floor(V.time * 1.3), id + 50.0)) < 0.015;
            if ((drop || (mode > 0.5 && strong) || rare) && J.flash < 0.06) {
                J.flash = 1.0;
            }
            J.pa = (J.pa + dt * (0.12 + 1.6 * J.flash)) % 64.0;
            // Between alarms its lights breathe faintly with the bass.
            J.glow = 0.2 + 0.7 * env.x * mk + 0.6 * flash_of(J);
        }
        case 4u: {
            // Rainbows race along the combs with the highs.
            J.pa = (J.pa + dt * (0.7 + 2.2 * env.z * mk)) % 64.0;
            J.glow = 0.3 + env.z * mk * 1.3;
            J.flash = ease_to(J.flash, V.onsets.z * mk * 0.6, dt, 0.06, 0.3);
        }
        case 5u: {
            // Waves of light run down the hair with the sustained low end.
            J.pa = (J.pa + dt * (0.25 + 1.2 * env.w * mk)) % 64.0;
            J.glow = 0.4 + env.w * mk * 0.7;
            J.flash = ease_to(J.flash, V.onsets.x * mk * 0.5, dt, 0.1, 0.5);
        }
        default: {
            // Gonads breathe with the bass; the rhopalia sparkle on the hi-hats.
            J.glow = 0.35 + 0.25 * sin(V.time * 0.5 + h1 * TAU) + env.x * mk * 0.9;
            J.flash = ease_to(J.flash, V.onsets.z * mk * 0.9, dt, 0.06, 0.3);
        }
    }
    // Every species gathers light through a build and blooms on the drop.
    J.glow += (0.3 * build() + 0.7 * drop_light(ev)) * min(mk, 1.2);
    return J;
}

// ---------------------------------------------------------------- the sound

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

// Item 2: this frame's onset hits (so each counts once), the time since the
// last drop plus one (0 before any), and the drops the analysis has found
// in this track.
fn events_step() -> vec4<f32> {
    let ev = item(2u);
    let since = select(select(0.0, min(ev.z + clamp(V.dt, 0.0, 0.1), 61.0), ev.z > 0.0), 1.0, new_drop(ev));
    return vec4<f32>(select(0.0, 1.0, V.hits.w > 0.5), select(0.0, 1.0, V.hits.x > 0.5), since, drop_count());
}

// Whether the analysis found a drop this frame (its count went up), and
// sure enough of it. ev is item 2 as last frame left it.
fn new_drop(ev: vec4<f32>) -> bool {
    return drop_count() > ev.w && drop_sure() > 0.3;
}

// A drop's light: blooming in over a fifth of a second, fading over ~2 s.
fn drop_light(ev: vec4<f32>) -> f32 {
    let t = ev.z - 1.0;
    return select(0.0, smoothstep(0.0, 0.2, t) * exp(-t / 1.8), ev.z > 0.0);
}

// Item 0: started, and clocks since the last high and low onsets (the
// plankton spark on them), with a count of high ones to pick which spark.
fn clock_step() -> vec4<f32> {
    let c = item(0u);
    let dt = clamp(V.dt, 0.0, 0.1);
    let high = V.hits.z > 0.5;
    let low = V.hits.x > 0.5;
    let started = c.x > 0.5;
    return vec4<f32>(1.0, select(min(c.y + dt, 60.0), 0.0, high || !started), select(c.z, c.z + 1.0, high) % 4096.0, select(min(c.w + dt, 60.0), 0.0, low || !started));
}

// ---------------------------------------------------------------- the camera and the glow

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

// How brightly jelly J glows all told (its light, flashes, the stroke).
fn glow_level(J: Jelly) -> f32 {
    let sp = species_of(kind());
    let b = bell_of(kind());
    let c = max(mix(bell_drive(b, J.prev, 0.0), bell_drive(b, J.since, 0.0), stroke_handover(J)), 0.0);
    // Soft: luminescence swells and ebbs, it does not strobe.
    return 0.3 + 0.4 * J.glow + 0.35 * flash_of(J) + 0.6 * sp.stroke_light * c;
}

// Jelly j's glow in the water (items HALO0 + 2j): where its bell is on
// screen, how big, how bright, in what colour, and how lately it stroked
// (the plankton round it spark when it does).
fn halo_step(i: u32) -> vec4<f32> {
    let j = (i - HALO0) / 2u;
    let J = jelly_at(j);
    if (j >= jelly_count() || J.age < 2.5) {
        return vec4<f32>(0.0);
    }
    let cam = the_camera();
    let centre = to_world(J, vec3<f32>(0.0, profile(j, 0u).y * 0.5, 0.0));
    let pr = project(cam, centre);
    if ((i - HALO0) % 2u == 0u) {
        let fade = depth_fade(cam, centre, cam.dist - 1.0, 0.35);
        return vec4<f32>(pr.xy, radius(J) * pr.z, select(0.0, glow_level(J) * fade, pr.z > 0.0));
    }
    return vec4<f32>(tone(LIGHT, hue_of(J)), mix(exp(-J.prev / 0.7), exp(-J.since / 0.7), stroke_handover(J)));
}
