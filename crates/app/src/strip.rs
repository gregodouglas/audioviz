//! Bottom transport strip: album art, track, previous / play-pause / next
//! (through the OS media session), progress, tint-from-art switch. Shown
//! together with the chrome bar; hidden otherwise.

use audioviz_metadata::{MediaCommand, NowPlaying};
use egui::{Align2, Color32, Rect, Sense, pos2, vec2};

use crate::theme::{self, BG, HAIRLINE, STRIP_H, TEXT, TEXT_2, TEXT_3};

#[derive(Default)]
pub struct StripActions {
    pub command: Option<MediaCommand>,
    pub shortcuts: bool,
    pub seek: Option<f32>,
}

pub struct Strip {
    art: Option<(u64, egui::TextureHandle)>,
}

impl Strip {
    pub fn new() -> Self {
        Self { art: None }
    }

    /// Keep the art texture fresh; returns its id if present.
    pub fn art_texture(&mut self, ctx: &egui::Context, np: Option<&NowPlaying>) -> Option<egui::TextureId> {
        let np = np?;
        let art = np.art.as_ref()?;
        if self.art.as_ref().map(|(id, _)| *id) != Some(np.track_id) {
            let img = egui::ColorImage::from_rgba_unmultiplied([art.width as usize, art.height as usize], &art.rgba);
            let tex = ctx.load_texture("strip-art", img, egui::TextureOptions::LINEAR);
            self.art = Some((np.track_id, tex));
        }
        self.art.as_ref().map(|(_, t)| t.id())
    }

    /// `t` is the animated visibility. `right_inset` keeps the strip clear of the panel.
    /// `identity`: show art, title and artist (the "show what's playing" setting).
    pub fn draw(&mut self, ctx: &egui::Context, np: Option<&NowPlaying>, tint_from_art: &mut bool, identity: bool, accent: Color32, t: f32, right_inset: f32) -> StripActions {
        let mut a = StripActions::default();
        if t <= 0.001 {
            return a;
        }
        let art_id = self.art_texture(ctx, np);
        let screen = ctx.content_rect();
        let y0 = screen.max.y - STRIP_H * t;
        let rect = Rect::from_min_max(pos2(screen.min.x, y0), pos2(screen.max.x - right_inset, screen.max.y));

        egui::Area::new(egui::Id::unique("strip")).fixed_pos(rect.min).order(egui::Order::Foreground).show(ctx, |ui| {
            ui.set_min_size(rect.size());
            let p = ui.painter().clone();
            p.rect_filled(rect, 0.0, theme::with_alpha(BG, 225));
            p.hline(rect.x_range(), rect.min.y + 0.5, egui::Stroke::new(1.0, HAIRLINE));
            // Swallow clicks on the strip background so they don't reach the scene.
            ui.interact(rect, egui::Id::unique("strip-bg"), Sense::click());

            // Art + text (only when the user wants to see what is playing).
            let art_r = Rect::from_min_size(pos2(rect.min.x + 16.0, rect.min.y + 12.0), vec2(48.0, 48.0));
            if !identity {
                p.text(pos2(rect.min.x + 16.0, rect.min.y + 31.0), Align2::LEFT_CENTER, "Now playing hidden", theme::sans(12.0), TEXT_3);
                p.text(pos2(rect.min.x + 16.0, rect.min.y + 49.0), Align2::LEFT_CENTER, "Press N to show", theme::sans(11.0), TEXT_3);
            }
            match art_id.filter(|_| identity) {
                Some(id) => {
                    p.image(id, art_r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                None if identity => {
                    p.rect_filled(art_r, 4.0, HAIRLINE);
                }
                None => {}
            }
            let (title, sub) = match np {
                _ if !identity => (String::new(), String::new()),
                Some(n) if !n.title.is_empty() => (n.title.clone(), if n.album.is_empty() { n.artist.clone() } else { format!("{}  ·  {}", n.artist, n.album) }),
                _ => ("Nothing playing".to_string(), String::new()),
            };
            let text_w = (rect.width() * 0.5 - 240.0 - 90.0).max(80.0);
            let tg = p.layout(title, theme::bold(15.0), TEXT, text_w);
            p.galley(pos2(rect.min.x + 78.0, rect.min.y + 20.0), tg, TEXT);
            let sg = p.layout(sub, theme::sans(12.0), TEXT_2, text_w);
            p.galley(pos2(rect.min.x + 78.0, rect.min.y + 42.0), sg, TEXT_2);

            // Controls, centred.
            let cx = rect.center().x;
            let cy = rect.min.y + 30.0;
            let playing = np.is_some_and(|n| n.playing);
            let btn = |ui: &mut egui::Ui, id: &str, c: egui::Pos2, r: f32| ui.interact(Rect::from_center_size(c, vec2(r * 2.0, r * 2.0)), egui::Id::unique(("strip", id)), Sense::click());
            let prev = btn(ui, "prev", pos2(cx - 40.0, cy), 12.0);
            let play = btn(ui, "play", pos2(cx, cy), 15.0);
            let next = btn(ui, "next", pos2(cx + 40.0, cy), 12.0);
            let col = |r: &egui::Response| if r.hovered() { TEXT } else { TEXT_2 };
            // prev: triangle + bar
            tri(&p, pos2(cx - 36.0, cy), -1.0, col(&prev));
            p.rect_filled(Rect::from_min_size(pos2(cx - 47.0, cy - 6.0), vec2(2.0, 12.0)), 0.0, col(&prev));
            tri(&p, pos2(cx + 36.0, cy), 1.0, col(&next));
            p.rect_filled(Rect::from_min_size(pos2(cx + 45.0, cy - 6.0), vec2(2.0, 12.0)), 0.0, col(&next));
            p.circle_filled(pos2(cx, cy), 15.0, if play.hovered() { Color32::WHITE } else { TEXT });
            if playing {
                p.rect_filled(Rect::from_min_size(pos2(cx - 5.0, cy - 5.5), vec2(3.0, 11.0)), 0.0, BG);
                p.rect_filled(Rect::from_min_size(pos2(cx + 2.0, cy - 5.5), vec2(3.0, 11.0)), 0.0, BG);
            } else {
                p.add(egui::Shape::convex_polygon(vec![pos2(cx - 4.0, cy - 6.0), pos2(cx + 6.0, cy), pos2(cx - 4.0, cy + 6.0)], BG, egui::Stroke::NONE));
            }
            if prev.clicked() {
                a.command = Some(MediaCommand::Previous);
            }
            if play.clicked() {
                a.command = Some(MediaCommand::TogglePlayPause);
            }
            if next.clicked() {
                a.command = Some(MediaCommand::Next);
            }

            // Progress.
            let half = (rect.width() * 0.5 - 300.0).clamp(120.0, 220.0);
            let (px0, px1) = (cx - half, cx + half);
            let (pos, dur, prog) = np.map(|n| (n.position_s, n.duration_s, n.progress())).unwrap_or((0.0, 0.0, 0.0));
            let fmt = |s: f64| format!("{:02}:{:02}", (s / 60.0) as u32, (s % 60.0) as u32);
            p.text(pos2(px0 - 10.0, rect.min.y + 55.0), Align2::RIGHT_CENTER, fmt(pos), theme::mono(11.0), TEXT_2);
            p.text(pos2(px1 + 10.0, rect.min.y + 55.0), Align2::LEFT_CENTER, fmt(dur), theme::mono(11.0), TEXT_2);
            let bar = Rect::from_min_size(pos2(px0, rect.min.y + 54.0), vec2(px1 - px0, 2.0));
            p.rect_filled(bar, 0.0, HAIRLINE);
            p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * prog, 2.0)), 0.0, accent);
            let seek = ui.interact(bar.expand2(vec2(0.0, 6.0)), egui::Id::unique("strip-seek"), Sense::click());
            if seek.clicked() {
                if let Some(pp) = seek.interact_pointer_pos() {
                    a.seek = Some(((pp.x - bar.min.x) / bar.width()).clamp(0.0, 1.0));
                }
            }

            // Right: tint from art, shortcuts.
            let rx = rect.max.x - 16.0;
            let sw = Rect::from_min_size(pos2(rx - 150.0, rect.min.y + 19.0), vec2(28.0, 16.0));
            let sw_resp = ui.interact(sw.expand(4.0), egui::Id::unique("strip-tint"), Sense::click());
            if sw_resp.clicked() {
                *tint_from_art = !*tint_from_art;
            }
            let tt = ctx.animate_bool_with_time(sw_resp.id, *tint_from_art, theme::ANIM_S);
            p.rect_filled(sw, 8.0, crate::widgets::lerp_color(HAIRLINE, accent, tt));
            p.circle_filled(pos2(sw.min.x + 8.0 + 12.0 * tt, sw.center().y), 6.0, crate::widgets::lerp_color(TEXT_2, BG, tt));
            p.text(pos2(rx, rect.min.y + 27.0), Align2::RIGHT_CENTER, "Tint from album art", theme::sans(11.0), TEXT_2);
            let sc = ui.interact(Rect::from_min_size(pos2(rx - 80.0, rect.min.y + 40.0), vec2(80.0, 18.0)), egui::Id::unique("strip-shortcuts"), Sense::click());
            p.text(pos2(rx, rect.min.y + 49.0), Align2::RIGHT_CENTER, "?  Shortcuts", theme::sans(11.0), if sc.hovered() { TEXT_2 } else { TEXT_3 });
            if sc.clicked() {
                a.shortcuts = true;
            }
        });
        a
    }
}

fn tri(p: &egui::Painter, c: egui::Pos2, dir: f32, color: Color32) {
    p.add(egui::Shape::convex_polygon(vec![pos2(c.x - 5.0 * dir, c.y - 6.0), pos2(c.x + 5.0 * dir, c.y), pos2(c.x - 5.0 * dir, c.y + 6.0)], color, egui::Stroke::NONE));
}
