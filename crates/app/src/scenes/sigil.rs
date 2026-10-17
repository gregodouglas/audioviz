//! Sigil visual: a neon sacred-geometry glyph (see `sigil.wgsl`). Centre rings
//! one per band, onset rays at the angle of the triggering pitch class,
//! twelve chroma-lit orbs on an orbit, a painterly ground. The arrangement
//! (ring radii and styles, ray symmetry, orbit) is seeded by the track: a
//! track change extinguishes the elements in order and relights a new glyph.
//! Rays and flares are kept in a ping-pong persistence texture for afterimages.

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, Scene, fullscreen_pass2, hdr_texture, HDR_FORMAT};
use crate::shaders::ShaderSet;
use crate::settings::{Layer, VisualKind};

pub const N_RAYS: usize = 24;
pub const N_RINGS: usize = 6;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct GlyphParams {
    pub resolution: [f32; 2],
    pub time: f32,
    pub scale: f32,
    pub tint: [f32; 4],
    pub bands: [[f32; 4]; 4],
    pub rings: [[f32; 4]; N_RINGS],
    pub rays: [[f32; 4]; N_RAYS],
    pub chroma: [[f32; 4]; 3],
    pub orbit: [f32; 4],
    pub build: [f32; 4],
    pub music: [f32; 4],
    pub style: [f32; 4],
    pub misc: [f32; 4],
    pub shape: [f32; 4],
    pub tri: [f32; 4],
    pub trace: [[f32; 4]; 4],
    pub trace_meta: [f32; 4],
    pub palette: crate::color::Table,
}

/// One centre ring of the arrangement.
#[derive(Clone, Copy, Debug)]
struct Ring {
    radius: f32,
    width: f32,
    /// 0 = solid, >0 = dash count, -1 = double line.
    style: f32,
    /// Rotation speed, radians per second at average energy.
    speed: f32,
    rot: f32,
}

/// The glyph's layout, drawn from the track seed.
#[derive(Clone, Debug)]
struct Arrangement {
    rings: [Ring; N_RINGS],
    /// Rays per onset, spread evenly around the glyph.
    symmetry: u32,
    orbit_radius: f32,
    orb_radius: f32,
    ray_len: f32,
    /// Direction of the glyph's slow spin.
    spin_dir: f32,
    /// Nesting depth of the polygram and the cardinal triangles.
    nest: u32,
    /// Centre star polygon {n/k} and its circumradius.
    poly_n: u32,
    poly_k: u32,
    poly_r: f32,
    /// Cardinal triangles / diagonal diamonds: centre radius and size.
    tri_r: f32,
    tri_size: f32,
    /// Sigil trace: orbit positions (pitch classes) it passes through.
    trace: [u8; 8],
    trace_n: usize,
    seed: u64,
}

/// splitmix64 step, mapped to 0..1.
fn rnd(state: &mut u64) -> f32 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

impl Arrangement {
    fn from_seed(seed: u64) -> Self {
        let mut s = seed ^ 0xA5A5_5A5A_1234_5678;
        let orbit_radius = 0.6 + 0.26 * rnd(&mut s);
        let inner = 0.11 + 0.1 * rnd(&mut s);
        // Six radii between the inner ring and the edge, kept clear of the orbit.
        let mut radii = [0.0f32; N_RINGS];
        radii[0] = inner;
        for r in radii.iter_mut().skip(1) {
            *r = inner + (0.92 - inner) * rnd(&mut s);
        }
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for r in radii.iter_mut() {
            let d = *r - orbit_radius;
            if d.abs() < 0.1 {
                *r = orbit_radius + if d < 0.0 { -0.1 } else { 0.1 };
            }
        }
        // Re-sort after the push and spread near-coincident rings apart.
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for i in 1..N_RINGS {
            if radii[i] - radii[i - 1] < 0.03 {
                radii[i] = radii[i - 1] + 0.03;
            }
        }
        let mut rings = [Ring { radius: 0.0, width: 0.0, style: 0.0, speed: 0.0, rot: 0.0 }; N_RINGS];
        for (i, ring) in rings.iter_mut().enumerate() {
            let u = rnd(&mut s);
            let style = if u < 0.45 {
                0.0
            } else if u < 0.8 {
                [12.0, 24.0, 36.0, 48.0, 72.0][(rnd(&mut s) * 4.99) as usize]
            } else {
                -1.0
            };
            let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
            ring.radius = radii[i];
            ring.width = 0.0045 + 0.005 * rnd(&mut s) * (if i == 0 { 1.4 } else { 1.0 });
            ring.style = style;
            ring.speed = dir * (0.03 + 0.08 * rnd(&mut s));
            ring.rot = rnd(&mut s) * std::f32::consts::TAU;
        }
        let symmetry = [1u32, 1, 2, 2, 3, 4][(rnd(&mut s) * 5.99) as usize];
        let orb_radius = 0.035 + 0.03 * rnd(&mut s);
        // Centre star polygon: {3/1} {4/1} {5/2} {6/2} {7/3} {8/3}; sits between
        // the inner rings and the orbit.
        let (poly_n, poly_k) = [(3u32, 1u32), (4, 1), (5, 2), (6, 2), (7, 3), (8, 3), (6, 2), (5, 2)][(rnd(&mut s) * 7.99) as usize];
        let poly_r = orbit_radius * (0.5 + 0.3 * rnd(&mut s));
        // Triangles inside the orbit or beyond it.
        let outside = rnd(&mut s) < 0.5;
        let tri_r = if outside { orbit_radius + orb_radius + 0.12 } else { (poly_r + orbit_radius) * 0.5 };
        let tri_size = if outside { 0.075 } else { ((orbit_radius - poly_r) * 0.32).clamp(0.035, 0.08) };
        // Trace: four to seven distinct orbit points.
        let trace_n = 4 + (rnd(&mut s) * 3.99) as usize;
        let mut trace = [0u8; 8];
        let mut n = 0;
        while n < trace_n {
            let c = (rnd(&mut s) * 11.99) as u8;
            if n == 0 || (trace[n - 1] != c && (n < 2 || trace[n - 2] != c)) {
                trace[n] = c;
                n += 1;
            }
        }
        Self {
            rings,
            symmetry,
            orbit_radius,
            orb_radius,
            ray_len: 0.3 + 0.35 * rnd(&mut s),
            spin_dir: if rnd(&mut s) < 0.5 { -1.0 } else { 1.0 },
            nest: 1 + (rnd(&mut s) * 2.99) as u32,
            poly_n,
            poly_k,
            poly_r,
            tri_r,
            tri_size,
            trace,
            trace_n,
            seed,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Ray {
    angle: f32,
    strength: f32,
    len: f32,
    pos: f32,
}

/// Seconds for the elements to go out, and to light back up.
const OUT_S: f32 = 1.6;
const IN_S: f32 = 3.2;
/// Ray life (strength e-fold), seconds.
const RAY_LIFE: f32 = 0.9;
/// Least time between ray strikes, seconds.
const RAY_GAP: f32 = 0.12;

pub struct Sigil {
    out: wgpu::TextureView,
    trail: [wgpu::TextureView; 2],
    current: usize,
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    groups: [wgpu::BindGroup; 2],
    pipeline: wgpu::RenderPipeline,
    arrangement: Arrangement,
    /// Seed waiting to be applied once the glyph has gone out.
    pending: Option<u64>,
    /// 0 = dark, 1 = whole glyph lit; moves at OUT_S / IN_S.
    lit: f32,
    going_out: bool,
    last_track: u64,
    rays: [Ray; N_RAYS],
    next_ray: usize,
    bands: [f32; N_RINGS],
    energy: f32,
    spin: f32,
    orbit_angle: f32,
    last_onset: bool,
    since_ray: f32,
    poly_rot: f32,
    /// Cardinal triangles: current and bar-stepped target rotation.
    tri_rot: f32,
    tri_target: f32,
    last_bar: u32,
    /// Pen position along the trace, in segments.
    pen: f32,
}

impl Sigil {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let sampler = scene::linear_sampler(device);
        let params_buf = scene::uniform_buffer(device, "sigil params", std::mem::size_of::<GlyphParams>() as u64);
        let layout = scene::tex_layout(device, "sigil", 0);
        let out = hdr_texture(device, "sigil out", width, height);
        let trail = [hdr_texture(device, "sigil trail A", width, height), hdr_texture(device, "sigil trail B", width, height)];
        let groups = [scene::tex_group(device, &layout, &[&trail[0]], &sampler, &params_buf), scene::tex_group(device, &layout, &[&trail[1]], &sampler, &params_buf)];
        let pipeline = build(device, shaders, &layout)?;
        Ok(Self {
            out,
            trail,
            current: 0,
            sampler,
            params_buf,
            layout,
            groups,
            pipeline,
            arrangement: Arrangement::from_seed(0x5161_1),
            pending: None,
            lit: 0.0,
            going_out: false,
            last_track: 0,
            rays: [Ray::default(); N_RAYS],
            next_ray: 0,
            bands: [0.0; N_RINGS],
            energy: 1.0,
            spin: 0.0,
            orbit_angle: 0.0,
            last_onset: false,
            since_ray: 1.0,
            poly_rot: 0.0,
            tri_rot: 0.0,
            tri_target: 0.0,
            last_bar: 0,
            pen: 0.0,
        })
    }

    fn spawn_rays(&mut self, class: usize, strength: f32) {
        let sym = self.arrangement.symmetry.max(1);
        let base = class as f32 / 12.0 * std::f32::consts::TAU;
        for k in 0..sym {
            // Take the weakest slot after the cursor so live rays survive.
            let mut best = self.next_ray % N_RAYS;
            for j in 0..N_RAYS {
                let idx = (self.next_ray + j) % N_RAYS;
                if self.rays[idx].strength < 0.02 {
                    best = idx;
                    break;
                }
                if self.rays[idx].strength < self.rays[best].strength {
                    best = idx;
                }
            }
            self.next_ray = best + 1;
            self.rays[best] = Ray {
                angle: base + k as f32 * std::f32::consts::TAU / sym as f32,
                strength,
                len: self.arrangement.ray_len * (0.8 + 0.4 * strength),
                pos: class as f32 / 12.0,
            };
        }
    }
}

/// Six band groups (sub, bass, low mid, mid, presence, air) from the 32
/// log-spaced analysis bands, by maximum.
fn group_bands(bands: &[f32]) -> [f32; N_RINGS] {
    const EDGES: [usize; 7] = [0, 4, 9, 15, 21, 27, 32];
    let mut out = [0.0f32; N_RINGS];
    for (i, o) in out.iter_mut().enumerate() {
        *o = bands.iter().skip(EDGES[i]).take(EDGES[i + 1] - EDGES[i]).copied().fold(0.0f32, f32::max).clamp(0.0, 1.5);
    }
    out
}

impl Scene for Sigil {
    fn kind(&self) -> VisualKind {
        VisualKind::Sigil
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.out = hdr_texture(device, "sigil out", width, height);
        self.trail = [hdr_texture(device, "sigil trail A", width, height), hdr_texture(device, "sigil trail B", width, height)];
        self.groups = [scene::tex_group(device, &self.layout, &[&self.trail[0]], &self.sampler, &self.params_buf), scene::tex_group(device, &self.layout, &[&self.trail[1]], &self.sampler, &self.params_buf)];
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipeline = build(device, shaders, &self.layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let s = &layer.sigil;
        let f = ctx.features;
        let dt = ctx.dt.clamp(0.0, 0.1);

        // Track change: queue a rebuild (or reseed at once while still dark).
        if ctx.art_id != 0 && ctx.art_id != self.last_track {
            let first = self.last_track == 0;
            self.last_track = ctx.art_id;
            if s.rebuild {
                if first && self.lit < 0.3 {
                    self.arrangement = Arrangement::from_seed(ctx.art_id);
                } else if !first {
                    self.pending = Some(ctx.art_id);
                    self.going_out = true;
                }
            }
        }
        if self.going_out {
            self.lit -= dt / OUT_S;
            if self.lit <= 0.0 {
                self.lit = 0.0;
                self.going_out = false;
                if let Some(seed) = self.pending.take() {
                    self.arrangement = Arrangement::from_seed(seed);
                }
                self.rays = [Ray::default(); N_RAYS];
            }
        } else if self.lit < 1.0 {
            self.lit = (self.lit + dt / IN_S).min(1.0);
        }

        // Music.
        let energy = ((f.bass_att + f.mid_att + f.treb_att) / 3.0).clamp(0.0, 2.5);
        self.energy += (energy - self.energy) * (1.0 - (-dt / 0.4).exp());
        let grouped = group_bands(&f.bands);
        for (b, g) in self.bands.iter_mut().zip(grouped) {
            let k = if g > *b { 0.04 } else { 0.18 };
            *b += (g - *b) * (1.0 - (-dt / k).exp());
        }
        let onset = f.onset_pulse[3].min(1.0);
        let flare = f.onset_pulse[0].min(1.0);
        let beat = f.beat_pulse * f.beat_confidence;

        // Rays: one strike per onset, at the loudest pitch class.
        self.since_ray += dt;
        let hit = f.onset_hit[3];
        if hit && !self.last_onset && self.since_ray > RAY_GAP && self.lit > 0.3 && !self.going_out && !f.silent {
            self.since_ray = 0.0;
            let class = f.chroma.iter().enumerate().fold((0usize, -1.0f32), |a, (i, v)| if *v > a.1 { (i, *v) } else { a }).0;
            let strength = (0.55 + 0.7 * f.onset_strength[3].min(1.0) + 0.5 * flare).min(1.5);
            self.spawn_rays(class, strength);
        }
        self.last_onset = hit;
        let decay = (-dt / RAY_LIFE).exp();
        for r in self.rays.iter_mut() {
            r.strength *= decay;
            if r.strength < 0.004 {
                r.strength = 0.0;
            }
        }

        // Motion: the glyph spins slowly, the orbit counter-rotates, rings turn.
        let pace = (0.5 + 0.5 * self.energy) * s.spin;
        self.spin += dt * 0.025 * self.arrangement.spin_dir * pace;
        self.orbit_angle -= dt * 0.06 * self.arrangement.spin_dir * pace;
        for ring in self.arrangement.rings.iter_mut() {
            ring.rot += dt * ring.speed * pace;
        }
        self.poly_rot -= dt * 0.04 * self.arrangement.spin_dir * pace;
        // Triangles step a twelfth of a turn every bar (four beats) and glide there.
        let bar = f.beat_count / 4;
        if bar != self.last_bar && f.beat_confidence > 0.3 {
            self.last_bar = bar;
            self.tri_target += std::f32::consts::TAU / 12.0;
        }
        self.tri_target += dt * 0.02 * pace;
        self.tri_rot += (self.tri_target - self.tri_rot) * (1.0 - (-dt / 0.25).exp());
        // The pen walks one trace segment per beat, or at a steady pace without a beat.
        let segs = (self.arrangement.trace_n.max(2) - 1) as f32;
        if f.beat_confidence > 0.3 {
            let want = ((f.beat_count % (segs as u32).max(1)) as f32 + f.beat_phase.clamp(0.0, 1.0)).min(segs);
            let mut diff = want - self.pen;
            if diff < -segs * 0.5 {
                diff += segs;
            }
            self.pen = (self.pen + diff * (1.0 - (-dt / 0.08).exp())).rem_euclid(segs);
        } else {
            self.pen = (self.pen + dt * 0.7 * pace).rem_euclid(segs);
        }

        let mut bands = [[0.0f32; 4]; 4];
        for (i, b) in self.bands.iter().enumerate() {
            bands[i / 4][i % 4] = *b;
        }
        let mut rings = [[0.0f32; 4]; N_RINGS];
        for (i, ring) in self.arrangement.rings.iter().enumerate() {
            rings[i] = [ring.radius, ring.width, ring.style, ring.rot];
        }
        let mut rays = [[0.0f32; 4]; N_RAYS];
        for (i, r) in self.rays.iter().enumerate() {
            rays[i] = [r.angle, r.strength, r.len, r.pos];
        }
        let mut chroma = [[0.0f32; 4]; 3];
        for i in 0..12 {
            // Lift the slow profile so chords read, with the fast one on top.
            chroma[i / 4][i % 4] = (0.6 * f.chroma_slow[i] + 0.6 * f.chroma[i]).clamp(0.0, 1.2);
        }
        let a = &self.arrangement;
        let mut trace = [[0.0f32; 4]; 4];
        for i in 0..a.trace_n.min(8) {
            let ang = self.orbit_angle + a.trace[i] as f32 * std::f32::consts::TAU / 12.0;
            trace[i / 2][(i % 2) * 2] = a.orbit_radius * ang.cos();
            trace[i / 2][(i % 2) * 2 + 1] = a.orbit_radius * ang.sin();
        }
        let params = GlyphParams {
            resolution: [ctx.width as f32, ctx.height as f32],
            time: ctx.time,
            scale: s.scale.max(0.1),
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], ctx.palette_amount],
            bands,
            rings,
            rays,
            chroma,
            orbit: [a.orbit_radius, self.orbit_angle, a.orb_radius, s.orbs],
            build: [self.lit, (a.seed % 1000) as f32, a.nest as f32, f.key_class as f32],
            music: [self.energy, onset, beat, flare],
            style: [s.glow, s.ground, s.rays, ctx.palette_shift],
            misc: [(-dt / s.persistence.max(0.02)).exp(), s.rings, self.spin, s.react],
            shape: [a.poly_n as f32, a.poly_k as f32, a.poly_r, self.poly_rot],
            tri: [a.tri_r, a.tri_size, self.tri_rot, s.shapes],
            trace,
            trace_meta: [a.trace_n as f32, self.pen, s.trace, a.nest as f32],
            palette: ctx.palette.copied().unwrap_or([[0.0; 4]; crate::color::ENTRIES]),
        };
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));

        let prev = self.current;
        let cur = 1 - prev;
        fullscreen_pass2(encoder, "sigil", &self.out, &self.trail[cur], &self.pipeline, &self.groups[prev]);
        self.current = cur;
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.out
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        let s = &mut layer.sigil;
        use crate::widgets::{ghost_button, hint, slider_row, toggle_row};
        slider_row(ui, "Size", &mut s.scale, 0.3..=1.6, |v| format!("{v:.2}"), false);
        slider_row(ui, "Glow", &mut s.glow, 0.0..=3.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Rings", &mut s.rings, 0.0..=2.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Rays", &mut s.rays, 0.0..=2.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Orbs", &mut s.orbs, 0.0..=2.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Shapes", &mut s.shapes, 0.0..=2.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Trace", &mut s.trace, 0.0..=2.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Ground", &mut s.ground, 0.0..=2.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Afterimage", &mut s.persistence, 0.05..=4.0, |v| format!("{v:.2} s"), true);
        slider_row(ui, "Spin", &mut s.spin, 0.0..=4.0, |v| format!("{v:.2}"), false);
        slider_row(ui, "Reacts", &mut s.react, 0.0..=2.0, |v| format!("{v:.2}"), false);
        toggle_row(ui, "Rebuild on track change", &mut s.rebuild);
        hint(ui, "Rings breathe with their band (sub to air, inside out), rays strike on onsets at the angle of the pitch class, the orbs light with the chord, the star and triangles turn with energy and step on bars, and a pen writes the sigil trace in time with the beat. Every track draws a new glyph. Give the layer the neon or rave palette.");
        ui.add_space(6.0);
        if ghost_button(ui, "Reset sigil", false).clicked() {
            *s = Default::default();
        }
        if ghost_button(ui, "New arrangement", false).clicked() {
            self.pending = Some(self.arrangement.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407));
            self.going_out = true;
        }
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "sigil.wgsl");
        scene::pipeline2(device, "sigil", layout, &m, HDR_FORMAT)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn params_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<GlyphParams>(), 816 + 512);
        assert_eq!(offset_of!(GlyphParams, tint), 16);
        assert_eq!(offset_of!(GlyphParams, bands), 32);
        assert_eq!(offset_of!(GlyphParams, rings), 96);
        assert_eq!(offset_of!(GlyphParams, rays), 192);
        assert_eq!(offset_of!(GlyphParams, chroma), 576);
        assert_eq!(offset_of!(GlyphParams, orbit), 624);
        assert_eq!(offset_of!(GlyphParams, build), 640);
        assert_eq!(offset_of!(GlyphParams, misc), 688);
        assert_eq!(offset_of!(GlyphParams, shape), 704);
        assert_eq!(offset_of!(GlyphParams, trace), 736);
        assert_eq!(offset_of!(GlyphParams, trace_meta), 800);
        assert_eq!(offset_of!(GlyphParams, palette), 816);
    }

    #[test]
    fn arrangement_is_sane_for_many_seeds() {
        for seed in 0..500u64 {
            let a = Arrangement::from_seed(seed.wrapping_mul(0x9E37_79B9));
            for w in a.rings.windows(2) {
                assert!(w[1].radius > w[0].radius, "seed {seed}: rings not ascending {:?}", a.rings);
            }
            for r in &a.rings {
                assert!(r.radius > 0.05 && r.radius <= 1.2, "seed {seed}: radius {}", r.radius);
            }
            assert!((1..=6).contains(&a.symmetry));
            assert!(a.trace_n >= 4 && a.trace_n <= 7);
            for w in a.trace[..a.trace_n].windows(2) {
                assert_ne!(w[0], w[1], "seed {seed}: trace repeats a point");
            }
        }
    }
}
