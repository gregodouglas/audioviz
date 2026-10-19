// Jelly: bells that swim. A visual opts in with "uses": ["jelly"], which
// brings "creatures" and "luminous" with it. See docs/building-visuals.md,
// "Creatures"; the Jellyfish visual is built on it.
//
// A bell is a profile from its crown (s = 0) to its margin (s = 1), drawn by
// the angle its tangent turns down through and spun round the axis. At rest
// the angle grows toward the margin (`bend`, gathered by `gather`); a stroke
// is the swimming muscle curling the outer bell further in (`curl`), with
// the thin, passive margin following `lag` seconds late, so it whips in and
// then flares out past rest (`flare`) as the springy jelly brings it back.
// Shaping from the angle, not by scaling a fixed shape, is what makes it
// move like jelly; and everything attached to the bell (its two skins,
// tentacle roots, the mouth) reads the same profile, so nothing comes apart.
//
// A visual keeps the profile in prepare items, one per node a frame
// (`bell_node`), and builds the skins from them (`bell_point`). Units are
// bell radii: the margin at rest is 1 out from the axis.

struct Bell {
    bend: f32,        // the tangent's turn at the margin at rest (radians): 1 a saucer, 1.8 a deep dome, 3 an egg
    gather: f32,      // how the turn gathers toward the margin: 1 even (a round cap), 1.7 a flat top with a curled rim
    aspect: f32,      // the profile's width against its height (an egg: 0.55)
    curl: f32,        // the extra turn at the margin at full squeeze
    flare: f32,       // how far the margin flares on the spring back
    lag: f32,         // seconds the margin lags the muscle
    squeeze: f32,     // seconds to close (0: the bell does not pulse)
    spring: f32,      // seconds for the jelly to spring it back past rest
    thick: f32,       // jelly between the skins at the crown (bell radii)
    rim: f32,         // and at the margin
    lobes: f32,       // lobes round the margin (0: an even margin)
    lobe_depth: f32,  // how far each lobe stands out
    notch: f32,       // how deep the notch between two lobes cuts in
    groove: f32,      // a ring groove round the bell (the atolla's coronal groove): its depth
    groove_at: f32,   // and where (0..1 from the crown)
    centre: f32,      // how far up the crown sits from the jelly's position, in depths of the bell
};

// The stroke at s, `since` seconds after it began (0 when the bell rests):
// the muscle squeezes the outer bell at once, the margin follows late.
fn bell_drive(b: Bell, since: f32, s: f32) -> f32 {
    if (b.squeeze <= 0.0) {
        return 0.0;
    }
    return elastic_stroke(since - b.lag * smoothstep(0.55, 1.0, s), b.squeeze, b.spring);
}

fn bell_rest_angle(b: Bell, s: f32) -> f32 {
    return b.bend * pow(s, b.gather);
}

// The tangent's angle at s, the stroke `since` seconds in.
fn bell_angle(b: Bell, since: f32, s: f32) -> f32 {
    let c = bell_drive(b, since, s);
    let w = smoothstep(0.2, 1.0, s);
    // A release overshoots below zero: the margin flares out most.
    let flare = min(c, 0.0) * b.flare * smoothstep(0.6, 1.0, s);
    return bell_rest_angle(b, s) + b.curl * max(c, 0.0) * w * w + flare;
}

// Node n (of nodes, crown 0 to margin nodes - 1) of the profile this frame:
// (out from the axis, height, tangent angle, the stroke there). The angles
// are integrated from the crown; the result is scaled so the widest point
// at rest is 1 out and the crown sits `centre` depths up.
fn bell_node(b: Bell, since: f32, n: u32, nodes: u32) -> vec4<f32> {
    let steps = max(nodes, 2u) - 1u;
    let ds = 1.0 / f32(steps);
    var rr = 0.0;
    var widest = 0.0;
    var depth = 0.0;
    var r = 0.0;
    var y = 0.0;
    for (var m = 0u; m < steps; m++) {
        let s = (f32(m) + 0.5) * ds;
        let a = bell_rest_angle(b, s);
        rr += cos(a) * ds;
        depth += sin(a) * ds;
        widest = max(widest, rr);
        if (m < n) {
            let a2 = bell_angle(b, since, s);
            r += cos(a2) * ds;
            y -= sin(a2) * ds;
        }
    }
    let k = 1.0 / max(widest, 1e-3);
    let s = f32(n) * ds;
    return vec4<f32>(r * k, (y + depth * b.centre) * k, bell_angle(b, since, s), bell_drive(b, since, s));
}

// The jelly between the skins at s: thick in the middle, thin at the margin.
fn bell_thickness(b: Bell, s: f32) -> f32 {
    return mix(b.rim, b.thick, pow(max(1.0 - s * s, 0.0), 1.2));
}

// How far out the margin stands at th (0..1 round): lobes, and a notch
// between each two (where a sense organ sits on many species).
fn bell_lobes(b: Bell, th: f32, s: f32) -> f32 {
    if (b.lobes <= 0.0) {
        return 1.0;
    }
    let x = fract(th * b.lobes);
    let lobe = 0.5 - 0.5 * cos(x * TAU);
    let d = min(x, 1.0 - x) * 8.0 / b.lobes;
    let notch = exp(-(d * d) / (0.035 * 0.035));
    return 1.0 + (b.lobe_depth * lobe - b.notch * notch) * smoothstep(0.8, 1.0, s);
}

// A point of a bell's skin and its normal (outward and up), in the bell's
// own units: profile node `node` (from `bell_node`) at s, th round (0..1).
// `inner` is the skin under the jelly (the subumbrella); `ripple` lifts the
// margin a little (a slow wave round it, so it never moves quite as one).
struct BellPoint {
    pos: vec3<f32>,
    normal: vec3<f32>,
};

fn bell_point(b: Bell, node: vec4<f32>, s: f32, th: f32, inner: bool, ripple: f32) -> BellPoint {
    let ang = th * TAU;
    let ca = cos(ang);
    let sa = sin(ang);
    var r = node.x * bell_lobes(b, th, s);
    if (b.groove > 0.0) {
        let g = (s - b.groove_at) / 0.06;
        r *= 1.0 - b.groove * exp(-g * g);
    }
    r *= b.aspect;
    let n2 = normalize(vec2<f32>(sin(node.z), cos(node.z) * b.aspect));
    var p: BellPoint;
    p.normal = vec3<f32>(n2.x * ca, n2.y, n2.x * sa);
    p.pos = vec3<f32>(r * ca, node.y + ripple * s * s * s, r * sa);
    if (inner) {
        p.pos -= p.normal * bell_thickness(b, s);
    }
    return p;
}

// Which way the bell's skin runs at a node, outward along the profile (for
// what hangs from the margin: it carries on that way near its root).
fn bell_tangent(b: Bell, node: vec4<f32>, th: f32) -> vec3<f32> {
    let ang = th * TAU;
    let t = normalize(vec2<f32>(cos(node.z) * b.aspect, -sin(node.z)));
    return vec3<f32>(t.x * cos(ang), t.y, t.x * sin(ang));
}
