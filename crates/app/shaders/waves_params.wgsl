// Waves uniform (scenes/waves.rs::Uniform). In the prelude so waves.wgsl sees it.
struct WavesParams {
    resolution: vec2<f32>,
    rows: u32,
    cols: u32,
    head: u32,            // ring index of the newest row
    source: u32,          // 0 waveform, 1 spectrum (CPU-side; here for completeness)
    colour: u32,          // 0 palette by row age, 1 palette by row level
    _pad0: u32,
    height: f32,          // amplitude of the near row, NDC units
    spread: f32,          // half width of the near row, NDC units
    persp: f32,           // 0 flat stack .. 1 strong foreshortening
    near_y: f32,          // baseline of the newest row, 0 bottom .. 1 top
    far_y: f32,           // baseline of the oldest row
    width: f32,           // line width, pixels
    fade: f32,            // how dim the far rows get
    time: f32,
    tint: vec4<f32>,
    palette_mix: f32,
    palette_shift: f32,
    _pad1a: f32,
    _pad1b: f32,
    palette: array<vec4<f32>, 32>,
};

fn waves_pal(wp: WavesParams, t: f32) -> vec3<f32> {
    let x = fract(t) * 32.0;
    let i = u32(floor(x)) % 32u;
    return mix(wp.palette[i].rgb, wp.palette[(i + 1u) % 32u].rgb, fract(x));
}
