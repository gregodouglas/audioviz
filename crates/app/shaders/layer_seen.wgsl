// What a visual's light goes through on its way to the screen, for light
// that should hold still there (a backdrop: the Jellyfish's water). A 1x1
// state texture per meter, ping-ponged, written once per frame before the
// visual draws, and handed to the visual (visual_api.wgsl's steady()).
//   r = steady: the slow average of the swing over now's (clamped 1/4..4);
//       multiplying by it undoes the swings of the last ~8 s
//   g, b = log of the slow average of the swing, split in two (the texture
//       holds half floats, too coarse for an 8 s average's small steps)
//   a = 1 + seconds since the average began; 0 before
// The swing is the layer's levelling gain (when it applies) times auto
// exposure, both as of last frame. While a scene arrives both are still
// finding their level, so the average starts quick (0.5 s) and slows to
// its ~8 s over the first ten seconds; begun slow, it held the water as
// bright as the first frames had it, and the water sank for seconds.
// Only the fast part is undone, not the level itself: water fills the
// frame, auto exposure measures it, and water that cancelled the exposure
// outright would chase it to its limit.
// Bindings: 0 = the layer's meter state, 3 = the auto exposure state,
// 4 = previous state. The uniform is the layer's (dt; flags bit0: levelling).

@group(0) @binding(0) var meter_tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> L: LayerParams;
@group(0) @binding(3) var expo_tex: texture_2d<f32>;
@group(0) @binding(4) var prev_state: texture_2d<f32>;

const AVERAGE_S: f32 = 8.0;
const SETTLE_S: f32 = 10.0;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FsIn {
    return fullscreen_vertex(vi);
}

struct Out {
    @location(0) state: vec4<f32>,
    @location(1) copy: vec4<f32>,
};

@fragment
fn fs_main(in: FsIn) -> Out {
    let meter = textureLoad(meter_tex, vec2<i32>(0, 0), 0);
    let expo = textureLoad(expo_tex, vec2<i32>(0, 0), 0);
    let prev = textureLoad(prev_state, vec2<i32>(0, 0), 0);
    var g = 1.0;
    if ((L.flags & 1u) != 0u && meter.a > 0.5 && meter.r > 0.0) {
        g = meter.r;
    }
    let e = select(1.0, expo.r, expo.r > 0.0);
    let now = log(max(g * e, 1e-6));
    let dt = clamp(L.dt, 0.0, 0.1);
    var avg = now;
    var age = 0.0;
    if (prev.a > 0.5) {
        age = prev.a - 1.0 + dt;
        let k = mix(0.5, AVERAGE_S, smoothstep(0.0, SETTLE_S, age));
        avg = mix(prev.g + prev.b, now, 1.0 - exp(-dt / k));
    }
    let hi = quantizeToF16(avg);
    var o: Out;
    o.state = vec4<f32>(clamp(exp(avg - now), 0.25, 4.0), hi, avg - hi, 1.0 + min(age, 1000.0));
    o.copy = o.state;
    return o;
}
