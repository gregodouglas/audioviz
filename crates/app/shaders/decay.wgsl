// Phosphor persistence: out = previous * decay. Used by any scene with a
// ScopeParams-compatible uniform at binding 2 (field `decay`).

@group(0) @binding(0) var prev_tex: texture_2d<f32>;
@group(0) @binding(1) var prev_samp: sampler;
@group(0) @binding(2) var<uniform> P: ScopeParams;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let c = textureSample(prev_tex, prev_samp, in.uv);
    // Subtract a tiny floor so the trail actually reaches black instead of
    // lingering as a faint glow forever.
    let v = max(c.rgb * P.decay - vec3<f32>(0.0005), vec3<f32>(0.0));
    return vec4<f32>(v, 1.0);
}
