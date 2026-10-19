//! Pictures for visuals: a folder of photos decoded on a background thread,
//! one at a time, in a shuffled order (the Kaleidoscope's slideshow, and any
//! loaded visual that asks for pictures).

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

/// Longest edge a decoded photo is kept at; plenty for a folded window.
const MAX_EDGE: u32 = 1536;

/// A decoded RGBA8 picture ready for upload.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Background decoder for a folder of photos: one request, one picture.
pub struct PhotoDeck {
    pub folder: String,
    pub files: usize,
    req: Sender<()>,
    rx: Receiver<Picture>,
    pending: bool,
}

impl PhotoDeck {
    pub fn spawn(folder: &str) -> Self {
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

    pub fn request(&mut self) {
        if self.files > 0 && !self.pending && self.req.send(()).is_ok() {
            self.pending = true;
        }
    }

    pub fn poll(&mut self) -> Option<Picture> {
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

