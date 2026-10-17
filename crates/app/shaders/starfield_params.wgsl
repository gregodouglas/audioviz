// Black hole visual uniform (scenes/starfield.rs::StarParams, 160 bytes).
struct StarParams {
    resolution: vec2<f32>,
    time: f32,
    rs: f32,                // Schwarzschild radius in world units (camera sits at 16)
    tint: vec4<f32>,        // base colour; w = warm (1 = default tint, disk runs blackbody)
    bands: array<vec4<f32>, 4>,   // 16 band energies: star sparkle, disk ring texture
    onset: f32,             // total onset pulse 0..1: star twinkle, disk turbulence
    beat: f32,              // beat pulse * confidence: photon-ring glow
    energy: f32,            // slewed broadband energy, ~1 = average
    density: f32,           // star density
    twinkle: f32,           // onset twinkle strength
    drift: f32,             // camera orbit drift (unused in the shader; az carries it)
    disk: f32,              // disk brightness
    doppler: f32,           // Doppler beaming strength
    tilt: f32,              // camera elevation above the disk plane, radians
    phase: f32,             // disk rotation phase (CPU-integrated, speeds up with energy)
    turb: f32,              // disk turbulence contrast
    flare: f32,             // low-onset flare 0..1
    az: f32,                // camera azimuth, radians (slow drift)
    _p0: f32,
    _p1: f32,
    _p2: f32,
};
