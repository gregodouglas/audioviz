// Jellyfish: a swarm of one species in a deep tank, in 3D. Six species,
// each with its own body and its own light, and each light answering a
// different part of the sound:
//   moon jelly     a saucer of clear jelly; four horseshoe gonads breathe with
//                  the bass, the rhopalia sparkle on the hi-hats; a fine fringe
//   sea nettle     a deep golden dome whose sixteen stripes pulse with the
//                  mids and flash on onsets; long tentacles, long spiralling arms
//   crystal jelly  a clear lens of a hundred canals with a ring of green
//                  lights round its rim that flash on strong low onsets
//   atolla         a deep red crown with a coronal groove; on a drop its blue
//                  alarm pinwheel spins round the rim
//   comb jelly     a glassy egg that glides while rainbows race along its eight
//                  comb rows with the highs
//   lion's mane    a big lobed bell and hundreds of fine hairs down which waves
//                  of light run with the sustained low end
// Every bell also lights as it squeezes (the light rides the stroke from
// the crown to the margin and down the tentacles), and glows in the water
// round it, lighting the plankton. They live without the music (strokes on
// their own rhythm, a slow current, each heading for its own wandering spot
// and keeping apart); the Music control decides how much the music sways them.
//
// Built on the "jelly" library (bells), which brings "creatures" (camera,
// current, strokes, chains, steering) and "luminous" (glass, light, tissue).
//
// The parts, in order after this file:
//   species.wgsl   the six species: body, bell, colours, how they light
//   swim.wgsl      a jelly's state and its frame: strokes, swimming, steering,
//                  its light, its bell's profile, its glow in the water
//   body.wgsl      tentacles, hair and oral arms (chains), the tentacle line
//   mesh.wgsl      the triangles: the bell's two skins and the arms
//   looks.wgsl     how every part looks (paint): the shared look, and each
//                  species' own organs and light
//   tank.wgsl      the water behind them: depth, plankton, their glow in it
//
// Prepare items:
//   0                   (1 once started, seconds since a high onset, high
//                       onsets so far, seconds since a low onset)
//   1                   smoothed sound: (lows, mids, highs, sustained energy)
//   2                   (last any onset hit, last low hit, drop envelope, 0)
//   3                   the close-up camera: (where it looks, how far away)
//   JELLY0 + 5j + f     jelly j: (position, seconds into its stroke),
//                       (velocity, 0), (axis, spin), (size, hue, frames
//                       alive, last beat), (glow, flash, wave phase, hue drift)
//   STRAND0 + (j * STRANDS + s) * SEG + k
//                       strand s (tentacle or hair) of jelly j, link k: (x, y, z, 0)
//   ARM0 + (j * ARMS + a) * ARM_SEG + k
//                       oral arm a of jelly j, link k: (x, y, z, 0)
//   PROFILE0 + j * NODES + n
//                       jelly j's bell profile at node n (crown 0 to margin
//                       RINGS): (out, height, tangent angle, stroke), from bell_node
//   HALO0 + 2j          jelly j's glow in the water: (on screen x, y, size,
//                       level), (colour, how lately it stroked)
//   SNOW0 + l           snow layer l: (where it is on screen, its cell size,
//                       how out of focus)
// Line point i is strand link STRAND0 + i. Triangles per jelly: the bell's
// outer skin (exumbrella) and inner skin (subumbrella), each RINGS x SECTORS
// quads, then the arms (ARM_SEG - 1 links, ARM_ACROSS quads across each,
// folded into a V), then its glow in the water (one quad facing the eye).

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
const HALO0: u32 = 31852u;          // PROFILE0 + MAX_J * NODES
const SNOW0: u32 = 31876u;          // HALO0 + 2 * MAX_J
const RINGS: u32 = 24u;
const NODES: u32 = 25u;             // RINGS + 1
const SECTORS: u32 = 64u;
const SHELL_CORNERS: u32 = 9216u;   // RINGS * SECTORS * 2 * 3
const BELL_CORNERS: u32 = 18432u;   // two skins
const ARM_CORNERS: u32 = 2880u;     // ARMS * (ARM_SEG - 1) * ARM_ACROSS * 2 * 3
const HALO_AT: u32 = 21312u;        // BELL_CORNERS + ARM_CORNERS
const CORNERS_J: u32 = 21318u;      // HALO_AT + one quad
// A bell's radius at size 1 (world units).
const BELL_R: f32 = 0.2;

const MOON: u32 = 0u;
const NETTLE: u32 = 1u;
const CRYSTAL: u32 = 2u;
const ATOLLA: u32 = 3u;
const COMB: u32 = 4u;
const LION: u32 = 5u;

fn kind() -> u32 {
    return min(u32(params().species), 5u);
}

fn jelly_count() -> u32 {
    return clamp(u32(params().count), 1u, MAX_J);
}

// How strongly the music sways them: Natural, Reactive, Performance.
fn music() -> f32 {
    let m = params().music;
    return select(select(1.8, 1.0, m < 1.5), 0.15, m < 0.5);
}

// 1 when the species' own colours apply to bodies (Natural, Palette light).
fn natural_bodies() -> f32 {
    return select(0.0, 1.0, params().colours > 0.5);
}

// The tank's half size in world units: as wide and tall as the picture, so
// a portrait screen gets a tall tank, not a wide one seen through a slot.
fn tank() -> vec3<f32> {
    let a = V.resolution / max(min(V.resolution.x, V.resolution.y), 1.0);
    return vec3<f32>(1.7 * a.x - 0.3, 1.25 * a.y - 0.15, 1.3);
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

// How far from the eye the water starts to dim things (the camera's
// distance to what it looks at, less a unit), without working out the camera.
fn eye_near() -> f32 {
    return select(4.6, item(3u).w, params().view > 0.5) - 1.0;
}

fn prepare(i: u32) -> vec4<f32> {
    if (i >= SNOW0) {
        return snow_step(i);
    }
    if (i >= HALO0) {
        return halo_step(i);
    }
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
        case 2u: { return events_step(); }
        case 3u: { return follow_step(); }
        default: { return clock_step(); }
    }
}
