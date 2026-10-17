// Starfield scene uniform (scenes/starfield.rs::StarParams, 128 bytes).
struct StarParams {
    resolution: vec2<f32>,
    time: f32,
    einstein: f32,          // Einstein radius in half-height units
    tint: vec4<f32>,
    bands: array<vec4<f32>, 4>,   // 16 band energies, perturb the lens shape
    onset: f32,
    beat: f32,
    grid: f32,
    density: f32,
    twinkle: f32,
    drift: f32,
    _p0: f32,
    _p1: f32,
};
