// Kaleidoscope object cell uniform (scenes/kaleido.rs::CellParams, 80 bytes).
struct CellParams {
    resolution: vec2<f32>,
    mix_t: f32,             // crossfade a -> b
    gain: f32,
    scale_a: vec2<f32>,     // screen units -> texture units per image
    scale_b: vec2<f32>,
    center_a: vec2<f32>,    // window centre in texture space
    center_b: vec2<f32>,
    angle_a: f32,
    angle_b: f32,
    energy: f32,
    onset: f32,
    tint: vec4<f32>,        // w = how far to pull the picture toward the tint
};
