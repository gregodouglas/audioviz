//! Swiss-grid typographic overlay: track title, artist, album, art and a
//! progress hairline, laid on a 12-column grid. Drawn with egui on the
//! background layer so it stays crisp (not bloomed) and under the panel.

use audioviz_analysis::FeatureFrame;
use audioviz_metadata::NowPlaying;

pub use crate::theme::{FAMILY_BOLD, FAMILY_REGULAR};

pub struct Overlay {
    art: Option<(u64, egui::TextureHandle)>,
}

impl Overlay {
    pub fn new() -> Self {
        Self { art: None }
    }

    /// `top`, `bottom`, `right`: space taken by the chrome bar, the transport strip and the panel.
    pub fn draw(&mut self, ctx: &egui::Context, np: Option<&NowPlaying>, f: &FeatureFrame, tint: [f32; 3], scale: f32, top: f32, bottom: f32, right: f32) {
        let Some(np) = np else { return };
        if np.title.is_empty() && np.artist.is_empty() {
            return;
        }
        // Album art texture, refreshed on track change.
        if let Some(art) = &np.art {
            if self.art.as_ref().map(|(id, _)| *id) != Some(np.track_id) {
                let img = egui::ColorImage::from_rgba_unmultiplied([art.width as usize, art.height as usize], &art.rgba);
                let tex = ctx.load_texture("album-art", img, egui::TextureOptions::LINEAR);
                self.art = Some((np.track_id, tex));
            }
        } else {
            self.art = None;
        }

        let full = ctx.content_rect();
        let screen = egui::Rect::from_min_max(full.min + egui::vec2(0.0, top), full.max - egui::vec2(right, bottom));
        let painter = ctx.layer_painter(egui::LayerId::background());
        let min_dim = screen.width().min(screen.height());
        let margin = min_dim * 0.06;
        let grid = egui::Rect::from_min_max(screen.min + egui::vec2(margin, margin), screen.max - egui::vec2(margin, margin));
        let col_w = grid.width() / 12.0;
        let gutter = col_w * 0.12;

        let accent = egui::Color32::from_rgb((tint[0] * 255.0) as u8, (tint[1] * 255.0) as u8, (tint[2] * 255.0) as u8);
        let ink = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 225);
        let ink_dim = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 140);

        let title_size = (min_dim * 0.065 * scale).max(18.0);
        let meta_size = (min_dim * 0.022 * scale).max(11.0);
        let small_size = (min_dim * 0.016 * scale).max(9.0);
        let bold = |size: f32| egui::FontId::new(size, egui::FontFamily::Name(FAMILY_BOLD.into()));
        let regular = |size: f32| egui::FontId::new(size, egui::FontFamily::Name(FAMILY_REGULAR.into()));

        // Bottom-left block: rule, title (max 2 lines, 8 columns), artist / album.
        let text_w = col_w * 8.0 - gutter;
        let title = painter.layout(np.title.clone(), bold(title_size), ink, text_w);
        let artist_line = if np.album.is_empty() { np.artist.to_uppercase() } else { format!("{}   ·   {}", np.artist.to_uppercase(), np.album) };
        let meta = painter.layout(artist_line, regular(meta_size), ink_dim, text_w);

        let bottom = grid.max.y;
        let meta_pos = egui::pos2(grid.min.x, bottom - meta.size().y);
        let title_pos = egui::pos2(grid.min.x, meta_pos.y - title.size().y - meta_size * 0.6);
        let rule_y = title_pos.y - meta_size * 0.9;

        // Hairline rule across the grid; its weight pulses gently with the beat.
        let beat = f.beat_pulse * f.beat_confidence;
        let rule_h = 1.0 + 1.5 * beat;
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(grid.min.x, rule_y), egui::pos2(grid.max.x, rule_y + rule_h)), 0.0, ink_dim);
        // Progress: the rule fills in accent up to the track position.
        let prog = np.progress();
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(grid.min.x, rule_y), egui::pos2(grid.min.x + grid.width() * prog, rule_y + rule_h)), 0.0, accent);

        painter.galley(title_pos, title, ink);
        painter.galley(meta_pos, meta, ink_dim);

        // Top-left: small caps status + time, column-aligned.
        let status = if np.playing { "NOW PLAYING" } else { "PAUSED" };
        let t = format!("{}  {:02}:{:02} / {:02}:{:02}", status, (np.position_s / 60.0) as u32, (np.position_s % 60.0) as u32, (np.duration_s / 60.0) as u32, (np.duration_s % 60.0) as u32);
        painter.text(grid.min, egui::Align2::LEFT_TOP, t, regular(small_size), ink_dim);

        // Bottom-right: album art, two columns wide, aligned to the rule.
        if let Some((_, tex)) = &self.art {
            let side = col_w * 2.0 - gutter;
            let rect = egui::Rect::from_min_size(egui::pos2(grid.max.x - side, bottom - side), egui::vec2(side, side));
            painter.image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::from_rgba_unmultiplied(255, 255, 255, 230));
            // Column rule beside the art.
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.min.x - gutter, rule_y), egui::pos2(rect.min.x - gutter + 1.0, bottom)), 0.0, ink_dim);
        }
    }
}
