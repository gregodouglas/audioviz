// Scope scene uniform (scenes/scope.rs::ScopeParams, 64 bytes).
struct ScopeParams {
    resolution: vec2<f32>,
    gain: f32,
    rotation: f32,
    beam_width: f32,
    beam_intensity: f32,
    decay: f32,
    _pad: f32,
    tint: vec4<f32>,
    point_count: u32,
    _pad2a: u32,
    _pad2b: u32,
    _pad2c: u32,
};
