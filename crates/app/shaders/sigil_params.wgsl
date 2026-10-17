// Sigil visual uniform (scenes/sigil.rs::GlyphParams, 1152 + 512 bytes).
struct GlyphParams {
    resolution: vec2<f32>,
    time: f32,
    scale: f32,                   // glyph half-extent as a fraction of the short half-side
    tint: vec4<f32>,              // layer tint; w = how far colours take the palette
    bands: array<vec4<f32>, 4>,   // x0..x5: six grouped bands (sub..air), one per ring
    rings: array<vec4<f32>, 6>,   // per ring: radius, tube width, style (0 solid, >0 dash count, -1 double), rotation
    rays: array<vec4<f32>, 24>,   // per ray slot: angle, strength (0 = free), length, palette position
    chroma: array<vec4<f32>, 3>,  // twelve pitch classes, C first: orb lighting
    orbit: vec4<f32>,             // orbit radius, orbit angle, orb radius, orb gain
    build: vec4<f32>,             // lit 0..1 (elements light in order), core gain, drips gain, key class
    music: vec4<f32>,             // energy (~1 average), onset, beat, flare (low onsets: drops)
    style: vec4<f32>,             // glow, ground, ray gain, palette shift
    misc: vec4<f32>,              // trail keep (per frame), ring gain, spin angle, react
    shape: vec4<f32>,             // polygram: sides, skip (1 = plain polygon), radius, rotation
    tri: vec4<f32>,               // cardinal triangles / diamonds: radius, size, rotation, gain
    trace: array<vec4<f32>, 4>,   // sigil trace: up to eight points (xy pairs), glyph units
    trace_meta: vec4<f32>,        // point count, pen position (segments), trace gain, nesting depth
    pen: vec4<f32>,               // pen position last frame (segments), hot core 0..1, line width multiplier, polygram shrink per level
    drips: array<vec4<f32>, 16>,  // per drip: x, top y, head y, palette position
    drip_s: array<vec4<f32>, 4>,  // sixteen drip strengths
    palette: array<vec4<f32>, 32>,
};
