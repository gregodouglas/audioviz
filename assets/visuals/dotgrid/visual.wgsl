// Dot grid: dots on a square grid that swell with the sound. In Waves a dot
// shows the sound as it was a moment ago, the delay its distance from the
// middle over the wave speed, so a kick spreads out as a travelling ring:
// the history (bass, mids, highs, all on one scale) is kept 60 times a
// second for 4 s. In Spectrum a dot's distance from the middle is its
// frequency on a log axis, live.
//
// Items: 0 .. HIST-1 the history ring (bass, mids, highs, 0);
// CLOCK: (seconds owed to the next sample, samples taken, 0, 0).

const HIST: u32 = 240u;
const CLOCK: u32 = 240u;
const RATE: f32 = 60.0;

// The sound now, 0..1: bass, mids and highs above their own averages, plus
// their punch, so a kick stands out of a loud mix.
fn sound_now() -> vec3<f32> {
    let g = params().gain;
    let bass = (max(broad_rel(SUB), broad_rel(BASS)) - 0.75) * 0.9 + 0.9 * broad_attack(BASS);
    let mids = (broad_rel(MID) - 0.75) * 0.8 + 0.6 * broad_attack(MID);
    let highs = (max(broad_rel(PRESENCE), broad_rel(AIR)) - 0.75) * 0.8 + 0.8 * broad_attack(AIR);
    return clamp(vec3<f32>(bass, mids, highs) * g, vec3<f32>(0.0), vec3<f32>(1.0));
}

// Whether a sample is taken this frame (every item works it out the same way).
fn sample_due() -> vec2<f32> {
    let c = item(CLOCK);
    let owed = c.x + clamp(V.dt, 0.0, 0.1);
    let take = owed >= 1.0 / RATE;
    return vec2<f32>(select(owed, owed - 1.0 / RATE, take), select(c.y, c.y + 1.0, take));
}

fn prepare(i: u32) -> vec4<f32> {
    let due = sample_due();
    if (i == CLOCK) {
        return vec4<f32>(min(due.x, 0.1), due.y % (f32(HIST) * 1000.0), 0.0, 0.0);
    }
    let taken = item(CLOCK).y;
    if (due.y != taken && i == u32(taken) % HIST) {
        return vec4<f32>(sound_now(), 0.0);
    }
    return item(i);
}

// The sound `ago` seconds back (0 beyond the history).
fn sound_ago(ago: f32) -> vec3<f32> {
    let k = u32(round(ago * RATE));
    let taken = u32(item(CLOCK).y);
    if (k + 1u >= HIST || k + 1u > taken) {
        return vec3<f32>(0.0);
    }
    return item((taken - 1u - k) % HIST).xyz;
}

fn shade(uv: vec2<f32>) -> vec3<f32> {
    let P = params();
    let px = pixel();
    let p = centred(uv);
    let s = max(P.spacing, 0.01);
    let cell = round(p / s) * s;
    let local = p - cell;
    let d = length(cell);
    var v: vec3<f32>;
    if (P.mode > 0.5) {
        // Distance is frequency, the lows in the middle.
        let ext = length(V.resolution / min(V.resolution.x, V.resolution.y));
        let t = clamp(d / ext, 0.0, 1.0);
        let lv = clamp((spectrum_log(t) - 0.25) / 0.55 * P.gain, 0.0, 1.0);
        v = vec3<f32>(lv, lv, t);
    } else {
        v = sound_ago(d / max(P.speed, 0.05));
    }
    let amount = pow(clamp(v.x * 0.75 + v.y * 0.25, 0.0, 1.0), 1.4);
    let radius = 0.5 * s * P.size * (P.rest + (1.0 - P.rest) * clamp(amount, 0.0, 1.0));
    var dist: f32;
    if (P.shape > 1.5) {
        dist = (abs(local.x) + abs(local.y)) * 0.7071 - radius * 0.85;
    } else if (P.shape > 0.5) {
        dist = max(abs(local.x), abs(local.y)) - radius * 0.85;
    } else {
        dist = length(local) - radius;
    }
    var c: vec3<f32>;
    if (P.mode > 0.5) {
        c = colour(0.1 + 0.8 * v.z);
    } else {
        // Colour from the distance, nudged by the highs then.
        c = colour(fract(d * 0.35 + 0.35 * v.z));
    }
    c *= 0.4 + 0.9 * amount;
    let cover = clamp(0.5 - dist / px, 0.0, 1.0);
    let glow = max(exp(-max(dist, 0.0) / (0.12 * s)) - 0.05, 0.0) * 0.3 * P.glow * amount;
    return c * (cover + glow);
}
