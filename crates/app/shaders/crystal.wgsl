// Crystal: translucent tetrahedral shards tumbling in a group, drawn as
// additive facets (fs_main / vs_main) and glowing screen-space edges
// (vs_edge / fs_edge). Shard transforms come from the CPU each frame.
// Inspired by the Crystal visualizer in Felix Turner's Uberviz; written fresh.

@group(0) @binding(0) var<uniform> C: CrystalParams;
@group(0) @binding(1) var<storage, read> shards: array<Shard>;

// A tetrahedron: four corners, four faces, six edges.
fn corner(i: u32) -> vec3<f32> {
    switch (i) {
        case 0u: { return vec3<f32>(1.0, 1.0, 1.0); }
        case 1u: { return vec3<f32>(1.0, -1.0, -1.0); }
        case 2u: { return vec3<f32>(-1.0, 1.0, -1.0); }
        default: { return vec3<f32>(-1.0, -1.0, 1.0); }
    }
}

fn face_corner(face: u32, v: u32) -> u32 {
    // Faces (0,1,2) (0,3,1) (0,2,3) (1,3,2).
    switch (face * 3u + v) {
        case 0u: { return 0u; } case 1u: { return 1u; } case 2u: { return 2u; }
        case 3u: { return 0u; } case 4u: { return 3u; } case 5u: { return 1u; }
        case 6u: { return 0u; } case 7u: { return 2u; } case 8u: { return 3u; }
        case 9u: { return 1u; } case 10u: { return 3u; } default: { return 2u; }
    }
}

fn edge_corners(e: u32) -> vec2<u32> {
    switch (e) {
        case 0u: { return vec2<u32>(0u, 1u); }
        case 1u: { return vec2<u32>(0u, 2u); }
        case 2u: { return vec2<u32>(0u, 3u); }
        case 3u: { return vec2<u32>(1u, 2u); }
        case 4u: { return vec2<u32>(1u, 3u); }
        default: { return vec2<u32>(2u, 3u); }
    }
}

fn shard_colour(s: Shard) -> vec3<f32> {
    let pal = crystal_pal(C, s.extra.y + C.palette_shift) * 1.2;
    return mix(s.colour.rgb, pal, C.palette_mix);
}

// ---- facets ----

struct FaceOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) colour: vec3<f32>,
    @location(1) bary: vec3<f32>,
    @location(2) alpha: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> FaceOut {
    let shard = ii / 4u;
    let face = ii % 4u;
    let s = shards[shard];
    let p = corner(face_corner(face, vi));
    // Flat shading: the face's world normal against a fixed key light, so
    // every facet of a shard reads a different brightness, like cut stone.
    let a = (s.model * vec4<f32>(corner(face_corner(face, 0u)), 1.0)).xyz;
    let b = (s.model * vec4<f32>(corner(face_corner(face, 1u)), 1.0)).xyz;
    let c = (s.model * vec4<f32>(corner(face_corner(face, 2u)), 1.0)).xyz;
    let n = normalize(cross(b - a, c - a));
    let shade = 0.45 + 0.55 * abs(dot(n, normalize(vec3<f32>(0.35, 0.7, 1.0))));
    var out: FaceOut;
    out.pos = C.view_proj * s.model * vec4<f32>(p, 1.0);
    out.bary = vec3<f32>(f32(vi == 0u), f32(vi == 1u), f32(vi == 2u));
    if (C.style > 0.5) {
        // Solid: a covering facet, premultiplied by its opacity.
        out.alpha = clamp(s.colour.a * C.facets, 0.0, 1.0);
        out.colour = shard_colour(s) * shade * out.alpha;
    } else {
        out.alpha = 0.0;
        out.colour = shard_colour(s) * shade * s.colour.a * C.facets;
    }
    return out;
}

@fragment
fn fs_main(in: FaceOut) -> @location(0) vec4<f32> {
    // Facets glow a little toward their edges, like glass catching light.
    let e = min(in.bary.x, min(in.bary.y, in.bary.z));
    let rim = 1.0 + 1.2 * exp(-e * 12.0);
    return vec4<f32>(in.colour * rim, in.alpha);
}

// ---- edges ----

struct EdgeOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) seg_len: f32,
    @location(2) colour: vec3<f32>,
};

fn to_px(clip: vec4<f32>) -> vec2<f32> {
    let ndc = clip.xy / max(clip.w, 1e-3);
    return vec2<f32>((ndc.x + 1.0) * 0.5 * C.resolution.x, (1.0 - ndc.y) * 0.5 * C.resolution.y);
}

@vertex
fn vs_edge(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> EdgeOut {
    let shard = ii / 6u;
    let edge = ii % 6u;
    let s = shards[shard];
    let ec = edge_corners(edge);
    let ca = C.view_proj * s.model * vec4<f32>(corner(ec.x), 1.0);
    let cb = C.view_proj * s.model * vec4<f32>(corner(ec.y), 1.0);
    let pa = to_px(ca);
    let pb = to_px(cb);
    var d = pb - pa;
    let len = length(d);
    if (len < 1e-4) { d = vec2<f32>(1.0, 0.0); } else { d = d / len; }
    let n = vec2<f32>(-d.y, d.x);
    let w = C.width * 0.5 + 2.0;
    var along: f32;
    var across: f32;
    switch (vi) {
        case 0u: { along = -w;       across = -w; }
        case 1u: { along = len + w;  across = -w; }
        case 2u: { along = -w;       across =  w; }
        case 3u: { along = -w;       across =  w; }
        case 4u: { along = len + w;  across = -w; }
        default: { along = len + w;  across =  w; }
    }
    let px = pa + d * along + n * across;
    var out: EdgeOut;
    out.pos = vec4<f32>(px.x / C.resolution.x * 2.0 - 1.0, 1.0 - px.y / C.resolution.y * 2.0, 0.0, 1.0);
    // Behind the camera: collapse the quad.
    if (ca.w <= 0.0 || cb.w <= 0.0) { out.pos = vec4<f32>(0.0, 0.0, 2.0, 1.0); }
    out.local = vec2<f32>(along, across);
    out.seg_len = len;
    out.colour = shard_colour(s) * s.extra.x * C.edges;
    return out;
}

@fragment
fn fs_edge(in: EdgeOut) -> @location(0) vec4<f32> {
    let along = clamp(in.local.x, 0.0, in.seg_len);
    let dist = length(vec2<f32>(in.local.x - along, in.local.y));
    let half = max(C.width, 0.5) * 0.5;
    let core = 1.0 - smoothstep(half - 0.5, half + 0.8, dist);
    let halo = exp(-dist * dist / (2.0 * (half + 2.0) * (half + 2.0))) * 0.35;
    return vec4<f32>(in.colour * (core + halo), 1.0);
}
