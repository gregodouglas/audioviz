// Bug eyed: eyes that all look the same way. Style picks the variant:
//   cartoon/  the original bug eyes (cartoon.wgsl), after vizz.fm's.
//   real/     eyes traced like real ones: eyeball, cornea, iris, wet
//             white, lids (eye.wgsl, iris.wgsl, light.wgsl).
// This file holds what both share.

fn rand(k: u32, salt: f32) -> f32 {
    var n = k * 747796405u + bitcast<u32>(salt) * 2891336453u + 1u;
    n = ((n >> ((n >> 28u) + 4u)) ^ n) * 277803737u;
    n = (n >> 22u) ^ n;
    return f32(n & 0xffffffu) / 16777215.0;
}
