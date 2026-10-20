// The six species: what each one is made of, how its bell is shaped and
// moves (a Bell, from the jelly library), its colours, and how its light
// answers the sound (the light itself is in swim.wgsl, the patterns in
// looks.wgsl).

struct Species {
    scale: f32,        // bell size
    strands: f32,      // tentacles or hairs
    strand_len: f32,   // their length, in bell radii
    strand_px: f32,    // their width, in pixels near the middle of the tank
    arm_len: f32,      // oral arms' length, in bell radii (0: none)
    arm_w: f32,        // and width
    period: f32,       // seconds between strokes on its own (0: no strokes)
    swim: f32,         // push per stroke
    glass: f32,        // how milky its jelly is (light per unit looked through)
    density: f32,      // how soon its jelly saturates, whitening (scatter): low clear, high milky
    edge: f32,         // how bright its skin seen edge on
    stroke_light: f32, // how much a stroke lights it
    follow: f32,       // how far its tentacles carry on the way the margin points
    upright: f32,      // how hard it keeps upright (1.5: within about 35 degrees)
};

fn species_of(kind: u32) -> Species {
    var s: Species;
    switch (kind) {
        case 1u: { s = Species(0.9, 24.0, 5.0, 1.4, 3.5, 0.3, 1.1, 1.0, 0.08, 1.8, 0.25, 0.35, 0.45, 1.2); }
        case 2u: { s = Species(0.95, 80.0, 1.8, 0.25, 0.25, 0.1, 2.0, 0.7, 0.01, 1.2, 0.35, 0.3, 0.45, 1.2); }
        case 3u: { s = Species(0.7, 22.0, 0.8, 1.2, 0.0, 0.0, 0.9, 1.1, 0.16, 2.5, 0.2, 0.3, 0.7, 1.0); }
        case 4u: { s = Species(0.85, 2.0, 3.5, 0.9, 0.0, 0.0, 0.0, 0.6, 0.012, 1.2, 0.3, 0.0, 0.3, 0.0); }
        case 5u: { s = Species(1.9, 160.0, 6.0, 0.5, 3.2, 0.9, 2.2, 0.6, 0.12, 1.2, 0.25, 0.3, 0.35, 1.2); }
        default: { s = Species(1.3, 160.0, 0.6, 0.32, 0.95, 0.22, 1.7, 0.8, 0.14, 2.2, 0.3, 0.5, 0.45, 1.5); }
    }
    return s;
}

// The bell: shape at rest, how a stroke bends it, the jelly in it (see
// Bell in the jelly library for what each number does).
fn bell_of(kind: u32) -> Bell {
    var b: Bell;
    switch (kind) {
        // A deep golden dome with a scalloped margin of thirty-two lappets
        // (painted: see LAPPET_DEPTH).
        case 1u: { b = Bell(1.8, 1.15, 1.0, 0.75, 1.0, 0.1, 0.26, 0.34, 0.12, 0.01, 32.0, 0.0, 0.0, 0.0, 0.0, 0.62); }
        // A wide, flat lens of clear jelly, slow and gentle.
        case 2u: { b = Bell(0.9, 1.5, 1.0, 0.9, 1.4, 0.12, 0.36, 0.5, 0.16, 0.01, 0.0, 0.0, 0.0, 0.0, 0.0, 0.75); }
        // A deep crown, thick jelly, twenty lappets (painted), the coronal groove.
        case 3u: { b = Bell(1.55, 1.1, 1.0, 0.7, 1.0, 0.08, 0.24, 0.32, 0.26, 0.015, 20.0, 0.0, 0.0, 0.07, 0.5, 0.62); }
        // An egg, pole to pole (a turn of pi closes it at both), that never pulses.
        case 4u: { b = Bell(3.1416, 1.0, 0.55, 0.0, 0.0, 0.0, 0.0, 1.0, 0.06, 0.04, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5); }
        // A broad, flat-topped disc, thick in the middle, its margin
        // turning down into a skirt of lobes round the mass (the photos'
        // side view), cut by deep clefts into eight pairs of lobes (the
        // notch within each pair painted); slow and heavy.
        case 5u: { b = Bell(1.8, 1.9, 1.0, 0.65, 0.95, 0.18, 0.5, 0.65, 0.4, 0.06, 8.0, 0.07, 0.13, 0.0, 0.0, 0.7); }
        // The moon jelly: a saucer, thick in the middle, eight lobes with a
        // rhopalium in each notch, a margin that whips and flares.
        default: { b = Bell(1.05, 1.7, 1.0, 1.1, 1.8, 0.12, 0.32, 0.4, 0.22, 0.012, 8.0, 0.035, 0.05, 0.0, 0.0, 0.73); }
    }
    return b;
}

// The parts a colour is asked for.
const BODY: u32 = 0u;      // the jelly of the bell
const ORGANS: u32 = 1u;    // what shows inside: gonads, stripes, canals
const STRANDS_C: u32 = 2u; // tentacles and hair
const ARMS_C: u32 = 3u;    // oral arms
const LIGHT: u32 = 4u;     // its own light: the stroke, photophores, its glow

// A palette's colour lifted to be seen: most palettes start near black,
// and a jelly whose place on it is there would vanish. Below a floor of
// brightness it is brightened keeping its hue, and black becomes a grey.
fn lifted(c: vec3<f32>) -> vec3<f32> {
    let floor_ = 0.3;
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    if (l >= floor_) {
        return c;
    }
    return mix(vec3<f32>(floor_), min(c * floor_ / max(l, 1e-3), vec3<f32>(1.0)), smoothstep(0.0, 0.04, l));
}

// The colour of a part. Natural: the species' own. Palette: the layer's
// palette, each part a step along it from the jelly's hue (drifting with
// the music). Palette light: natural bodies, glowing in the palette's
// colours (organs and light).
fn tone(part: u32, hue: f32) -> vec3<f32> {
    let m = params().colours;
    let lit = part == ORGANS || part == LIGHT;
    if (m < 0.5 || (m > 1.5 && lit)) {
        return lifted(colour(hue + f32(part) * 0.17));
    }
    var c = array<vec3<f32>, 5>(vec3<f32>(0.5, 0.7, 1.0), vec3<f32>(0.95, 0.42, 1.0), vec3<f32>(0.62, 0.8, 1.0), vec3<f32>(0.68, 0.78, 1.0), vec3<f32>(0.55, 0.85, 1.0));
    switch (kind()) {
        case 1u: { c = array<vec3<f32>, 5>(vec3<f32>(1.0, 0.55, 0.18), vec3<f32>(1.0, 0.25, 0.06), vec3<f32>(1.0, 0.38, 0.1), vec3<f32>(1.0, 0.85, 0.65), vec3<f32>(1.0, 0.62, 0.22)); }
        case 2u: { c = array<vec3<f32>, 5>(vec3<f32>(0.7, 0.85, 1.0), vec3<f32>(0.8, 0.9, 1.0), vec3<f32>(0.75, 0.9, 1.0), vec3<f32>(0.6, 1.0, 0.7), vec3<f32>(0.2, 1.0, 0.4)); }
        case 3u: { c = array<vec3<f32>, 5>(vec3<f32>(0.75, 0.03, 0.06), vec3<f32>(0.9, 0.08, 0.12), vec3<f32>(0.8, 0.1, 0.12), vec3<f32>(0.8, 0.1, 0.1), vec3<f32>(0.15, 0.45, 1.0)); }
        case 4u: { c = array<vec3<f32>, 5>(vec3<f32>(0.6, 0.75, 1.0), vec3<f32>(0.9, 0.9, 1.0), vec3<f32>(0.85, 0.85, 1.0), vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(0.8, 0.9, 1.0)); }
        // An amber bell round a red-orange middle; deep red organs; pale
        // hair; a cream mass. (Redder than they look: the water between
        // takes red first, and amber arrived as mustard.)
        case 5u: { c = array<vec3<f32>, 5>(vec3<f32>(1.0, 0.42, 0.12), vec3<f32>(0.9, 0.14, 0.05), vec3<f32>(1.0, 0.86, 0.72), vec3<f32>(1.0, 0.68, 0.36), vec3<f32>(1.0, 0.7, 0.3)); }
        default: {}
    }
    return c[part];
}
