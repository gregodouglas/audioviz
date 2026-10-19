//! Kaleidoscope object cell: a drifting, turning window into a picture (the
//! album art, or photos from a folder shown as a slideshow with a crossfade).
//! The layer's fold (director) supplies the mirrors; without it this is a
//! music-driven slideshow.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

use bytemuck::{Pod, Zeroable};

use crate::scene::{self, FrameCtx, Scene, fullscreen_pass, hdr_texture, HDR_FORMAT};
use crate::shaders::ShaderSet;
use crate::settings::{ImageSource, Layer, KaleidoParams};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct CellParams {
    pub resolution: [f32; 2],
    pub mix_t: f32,
    pub gain: f32,
    pub scale_a: [f32; 2],
    pub scale_b: [f32; 2],
    pub center_a: [f32; 2],
    pub center_b: [f32; 2],
    pub angle_a: f32,
    pub angle_b: f32,
    pub energy: f32,
    pub onset: f32,
    pub tint: [f32; 4],
}

/// Longest edge a decoded photo is kept at; plenty for a folded window.
const MAX_EDGE: u32 = 1536;
const FADE_S: f32 = 1.5;

/// A decoded RGBA8 picture ready for upload.
struct Picture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Background decoder for a folder of photos: one request, one picture.
struct PhotoDeck {
    folder: String,
    files: usize,
    req: Sender<()>,
    rx: Receiver<Picture>,
    pending: bool,
}

impl PhotoDeck {
    fn spawn(folder: &str) -> Self {
        let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png")))
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        // Shuffle with a small LCG so every run starts somewhere else.
        let mut seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
        for i in (1..files.len()).rev() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let j = (seed >> 33) as usize % (i + 1);
            files.swap(i, j);
        }
        let n = files.len();
        let (req, req_rx) = mpsc::channel::<()>();
        let (tx, rx) = mpsc::channel::<Picture>();
        if n > 0 {
            std::thread::Builder::new()
                .name("photo deck".into())
                .spawn(move || {
                    let mut i = 0usize;
                    while req_rx.recv().is_ok() {
                        // Try up to n files so one bad picture does not stall the deck.
                        for _ in 0..n {
                            let path = &files[i % n];
                            i += 1;
                            match decode(path) {
                                Some(p) => {
                                    if tx.send(p).is_err() {
                                        return;
                                    }
                                    break;
                                }
                                None => tracing::warn!("photo deck: could not decode {}", path.display()),
                            }
                        }
                    }
                })
                .ok();
        }
        tracing::info!("photo deck: {n} pictures in {folder}");
        Self { folder: folder.to_string(), files: n, req, rx, pending: false }
    }

    fn request(&mut self) {
        if self.files > 0 && !self.pending && self.req.send(()).is_ok() {
            self.pending = true;
        }
    }

    fn poll(&mut self) -> Option<Picture> {
        let p = self.rx.try_recv().ok()?;
        self.pending = false;
        Some(p)
    }
}

fn decode(path: &std::path::Path) -> Option<Picture> {
    let img = image::open(path).ok()?;
    let (w, h) = (img.width(), img.height());
    let img = if w.max(h) > MAX_EDGE {
        let k = MAX_EDGE as f32 / w.max(h) as f32;
        img.resize((w as f32 * k) as u32, (h as f32 * k) as u32, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    Some(Picture { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
}

/// One of the two crossfade slots.
struct Slot {
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    angle: f32,
    phase: f32,
}

pub struct Kaleido {
    device: wgpu::Device,
    out: wgpu::TextureView,
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    slots: [Slot; 2],
    /// Slot currently shown; the other receives the next picture.
    active: usize,
    /// Crossfade progress toward the inactive slot, when one is running.
    fade: Option<f32>,
    deck: Option<PhotoDeck>,
    next_at: f32,
    last_art: u64,
    energy: f32,
    kick: f32,
    last_onset: f32,
}

fn picture_texture(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, width: u32, height: u32, rgba: &[u8]) -> wgpu::TextureView {
    let size = wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * size.width), rows_per_image: Some(size.height) },
        size,
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

impl Kaleido {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Self, String> {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("kaleido mirror"),
            address_mode_u: wgpu::AddressMode::MirrorRepeat,
            address_mode_v: wgpu::AddressMode::MirrorRepeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params_buf = scene::uniform_buffer(device, "kaleido params", std::mem::size_of::<CellParams>() as u64);
        let layout = scene::tex_layout(device, "kaleido", 1);
        let out = hdr_texture(device, "kaleido out", width, height);
        let pipeline = build(device, shaders, &layout)?;
        // Placeholder slots: a dim grey cell until a picture arrives.
        let blank = |label: &str| {
            let q = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            Slot { view: q.create_view(&wgpu::TextureViewDescriptor::default()), width: 1, height: 1, angle: 0.0, phase: 0.0 }
        };
        Ok(Self {
            device: device.clone(),
            out,
            sampler,
            params_buf,
            layout,
            pipeline,
            slots: [blank("kaleido slot a"), blank("kaleido slot b")],
            active: 0,
            fade: None,
            deck: None,
            next_at: 0.0,
            last_art: 0,
            energy: 1.0,
            kick: 0.0,
            last_onset: 0.0,
        })
    }

    /// Put a picture in the inactive slot and start the crossfade to it.
    fn incoming(&mut self, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8], time: f32) {
        let i = 1 - self.active;
        let view = picture_texture(&self.device, queue, "kaleido picture", width, height, rgba);
        self.slots[i] = Slot { view, width, height, angle: self.slots[self.active].angle, phase: time * 0.1 };
        self.fade = Some(0.0);
    }
}

/// Registry id: the layer's `visual`, its parameter key, the presets folder.
pub const ID: &str = "kaleido";

fn make(device: &wgpu::Device, width: u32, height: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
    Ok(Box::new(Kaleido::new(device, width, height, shaders)?))
}

pub fn def() -> crate::visuals::VisualDef {
    crate::visuals::VisualDef {
        id: ID,
        name: "Kaleidoscope",
        short: "Kaleidoscope",
        summary: "A drifting window into the album art or your photos; pair it with a Kaleidoscope effect for mirrors.",
        make,
        params: crate::visuals::typed::<KaleidoParams>,
        band_waveform: false,
    }
}

impl Scene for Kaleido {
    fn id(&self) -> &'static str {
        ID
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.out = hdr_texture(device, "kaleido out", width, height);
    }

    fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) -> Result<(), String> {
        self.pipeline = build(device, shaders, &self.layout)?;
        Ok(())
    }

    fn render(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &FrameCtx, layer: &Layer) {
        let k = &layer.get::<KaleidoParams>(ID);
        let f = ctx.features;
        let dt = ctx.dt.clamp(0.0, 0.1);
        let time = ctx.time;

        // ---- source: photos from a folder, else the album art ----
        let want_photos = k.source == ImageSource::Photos && !ctx.photos_folder.trim().is_empty();
        if want_photos {
            if self.deck.as_ref().is_none_or(|d| d.folder != ctx.photos_folder) {
                let mut d = PhotoDeck::spawn(ctx.photos_folder);
                d.request();
                self.deck = Some(d);
                self.next_at = time;
            }
        } else {
            self.deck = None;
        }
        let photos_live = self.deck.as_ref().is_some_and(|d| d.files > 0);
        if photos_live {
            let deck = self.deck.as_mut().unwrap();
            if let Some(p) = deck.poll() {
                self.incoming(queue, p.width, p.height, &p.rgba, time);
                self.next_at = time + k.dwell_s.max(2.0);
            } else if time >= self.next_at {
                deck.request();
            }
        } else if let Some(art) = ctx.art {
            if ctx.art_id != self.last_art {
                self.last_art = ctx.art_id;
                self.incoming(queue, art.width, art.height, &art.rgba, time);
            }
        }

        // ---- motion ----
        let energy = ((f.bass_att + f.mid_att + f.treb_att) / 3.0).clamp(0.0, 2.5);
        self.energy += (energy - self.energy) * (1.0 - (-dt / 0.4).exp());
        let onset = f.onset_pulse[3].min(1.0);
        if onset > self.last_onset + 0.25 {
            self.kick += 0.9 * k.tumble;
        }
        self.last_onset = onset;
        self.kick *= (-dt / 0.3).exp();
        let spin = dt * (0.03 + 0.10 * self.energy) * k.drift;
        for (i, s) in self.slots.iter_mut().enumerate() {
            let dir = if i == 0 { 1.0 } else { -1.0 };
            s.angle += dir * (spin + self.kick * dt);
            s.phase += dt * 0.12 * k.drift * (0.5 + 0.5 * self.energy);
        }
        if let Some(t) = &mut self.fade {
            *t += dt / FADE_S;
            if *t >= 1.0 {
                self.active = 1 - self.active;
                self.fade = None;
            }
        }

        let aspect = ctx.width as f32 / ctx.height.max(1) as f32;
        let zoom = (k.zoom * (1.0 + 0.06 * (f.bass_att - 1.0).clamp(-1.0, 1.5))).clamp(0.1, 1.5);
        let place = |s: &Slot| {
            let ia = s.width as f32 / s.height.max(1) as f32;
            // A window `zoom` of the picture's height tall, aspect-correct.
            let scale = [zoom * 0.5 / ia.max(0.01), zoom * 0.5];
            let room = (1.0 - zoom).max(0.0) * 0.5;
            let center = [0.5 + room * (s.phase * 0.37).sin() * 0.9, 0.5 + room * (s.phase * 0.23 + 1.3).cos() * 0.9];
            let _ = aspect;
            (scale, center, s.angle)
        };
        let (sa, ca, aa) = place(&self.slots[self.active]);
        let (sb, cb, ab) = place(&self.slots[1 - self.active]);
        let mix_t = self.fade.map(|t| t * t * (3.0 - 2.0 * t)).unwrap_or(0.0);
        let params = CellParams {
            resolution: [ctx.width as f32, ctx.height as f32],
            mix_t,
            gain: 0.6,
            scale_a: sa,
            scale_b: sb,
            center_a: ca,
            center_b: cb,
            angle_a: aa,
            angle_b: ab,
            energy: self.energy,
            onset: onset * k.tumble.min(1.0),
            tint: [ctx.tint[0], ctx.tint[1], ctx.tint[2], k.tint_mix],
        };
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        let group = scene::tex_group(&self.device, &self.layout, &[&self.slots[self.active].view, &self.slots[1 - self.active].view], &self.sampler, &self.params_buf);
        fullscreen_pass(encoder, "kaleido", &self.out, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipeline, &group);
    }

    fn output(&self) -> &wgpu::TextureView {
        &self.out
    }

    fn ui(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        let mut params = layer.get::<KaleidoParams>(ID);
        let k = &mut params;
        let d = KaleidoParams::default();
        use crate::widgets::{ghost_button, hint, segmented, slider_def};
        if let Some(i) = segmented(ui, &["Album art", "My photos"], (k.source == ImageSource::Photos) as usize) {
            k.source = if i == 1 { ImageSource::Photos } else { ImageSource::Art };
        }
        if k.source == ImageSource::Photos {
            slider_def(ui, "Seconds per photo", &mut k.dwell_s, Some(d.dwell_s), 3.0..=120.0, |v| format!("{v:.0} s"), true);
        }
        slider_def(ui, "Window", &mut k.zoom, Some(d.zoom), 0.15..=1.0, |v| format!("{:.0}%", v * 100.0), false);
        slider_def(ui, "Drift", &mut k.drift, Some(d.drift), 0.0..=4.0, |v| format!("{v:.2}"), false);
        slider_def(ui, "Tumble on onsets", &mut k.tumble, Some(d.tumble), 0.0..=2.0, |v| format!("{v:.2}"), false);
        slider_def(ui, "Pull to tint", &mut k.tint_mix, Some(d.tint_mix), 0.0..=1.0, |v| format!("{v:.2}"), false);
        hint(ui, "A window into the picture drifts and turns with the music; the layer's kaleidoscope modifier makes the mirrors. Photos fall back to the album art when the folder is empty.");
        ui.add_space(6.0);
        if ghost_button(ui, "Reset kaleidoscope", false).clicked() {
            *k = Default::default();
        }
        layer.put(ID, &params);
    }
}

fn build(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "kaleido.wgsl");
        scene::pipeline(device, "kaleido", layout, &m, HDR_FORMAT, None)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn params_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<CellParams>(), 80);
        assert_eq!(offset_of!(CellParams, scale_a), 16);
        assert_eq!(offset_of!(CellParams, center_a), 32);
        assert_eq!(offset_of!(CellParams, angle_a), 48);
        assert_eq!(offset_of!(CellParams, tint), 64);
    }
}
