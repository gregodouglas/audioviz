//! The director view (ADR-0006): what the engine hears, laid out on one time
//! axis for whoever makes visuals (the audience never sees it). Lanes, top to
//! bottom: a log-frequency spectrogram in dB, the 32 bands' levels (one
//! scale for all) and their attacks, onset strength against its threshold with the hits (low, mid, high,
//! all), the beat clock (phase, confidence, the beats as a grid through every
//! lane) and the level. It reads the engine's history, so it can be frozen
//! (F), dragged back over the last five minutes and zoomed (the wheel).
//! D opens and closes it. The spectrogram's frequency axis is linear (the
//! default: the highs in detail, where pictures hidden in a track's spectrum
//! live) or logarithmic (L, or click the scale's name); the wheel over its
//! frequency gutter zooms the range, a drag there moves it, a double click
//! resets it. Dragging the view's top edge sets its height, and dragging the
//! line under the spectrogram its share.

use std::collections::VecDeque;

use audioviz_analysis::history::{HopRecord, LIN_HI_HZ, LIN_ROWS, RECORD_BANDS, SPEC_FLOOR_DB, SPEC_HI_HZ, SPEC_LO_HZ, SPEC_ROWS};
use audioviz_audio::CaptureHealth;
use egui::{Align2, Color32, ColorImage, Pos2, Rect, Sense, Shape, Stroke, TextureHandle, TextureOptions, pos2, vec2};

use crate::settings::DirectorView;
use crate::theme::{self, BG, HAIRLINE, TEXT, TEXT_2, TEXT_3};

/// Seconds per analysis hop (512 samples at 48 kHz).
const HOP_S: f64 = 512.0 / 48_000.0;
/// How far back the history reaches.
const KEEP_S: f64 = 300.0;
/// Columns the lanes are drawn at, whatever the span.
const COLS: usize = 1024;
const GUTTER: f32 = 64.0;
const GROUP_NAMES: [&str; 4] = ["Low", "Mid", "High", "All"];
const GROUP_COLOURS: [Color32; 4] = [
    Color32::from_rgb(0xff, 0x8a, 0x5c),
    Color32::from_rgb(0xff, 0xd2, 0x6b),
    Color32::from_rgb(0x6f, 0xc8, 0xff),
    Color32::from_rgb(0xd9, 0xdf, 0xdd),
];
const BEAT: Color32 = Color32::from_rgb(0x66, 0xff, 0x8c);
const NOISY: Color32 = Color32::from_rgb(0xc7, 0x8b, 0xff);
const BUILD: Color32 = Color32::from_rgb(0xff, 0x9f, 0x43);
const DROP: Color32 = Color32::from_rgb(0xff, 0x5c, 0x7a);

/// The engine's recent past, as records per hop, and the track changes in it.
pub struct History {
    records: VecDeque<HopRecord>,
    /// (time_s, label) of each track change.
    marks: VecDeque<(f64, String)>,
}

impl History {
    pub fn new() -> Self {
        Self { records: VecDeque::with_capacity((KEEP_S / HOP_S) as usize + 64), marks: VecDeque::new() }
    }

    pub fn extend(&mut self, new: Vec<HopRecord>) {
        for r in new {
            // The analyzer starts again (a new sample rate): so does the history.
            if self.records.back().is_some_and(|b| r.time_s < b.time_s) {
                self.records.clear();
                self.marks.clear();
            }
            self.records.push_back(r);
        }
        let cap = (KEEP_S / HOP_S) as usize;
        while self.records.len() > cap {
            self.records.pop_front();
        }
        let first = self.first_time();
        while self.marks.front().is_some_and(|(t, _)| *t < first) {
            self.marks.pop_front();
        }
    }

    /// The track changes kept, as (analysis time, label).
    pub fn marks(&self) -> Vec<(f64, String)> {
        self.marks.iter().cloned().collect()
    }

    /// Marks the present, e.g. a new track.
    pub fn mark(&mut self, label: String) {
        self.marks.push_back((self.latest_time(), label));
    }

    pub fn latest_time(&self) -> f64 {
        self.records.back().map_or(0.0, |r| r.time_s)
    }

    fn first_time(&self) -> f64 {
        self.records.front().map_or(0.0, |r| r.time_s)
    }

    /// Records whose time lies in `t0..t1`, as an index range.
    fn range(&self, t0: f64, t1: f64) -> std::ops::Range<usize> {
        let n = self.records.len();
        if n == 0 {
            return 0..0;
        }
        let first = self.first_time();
        let i = |t: f64| (((t - first) / HOP_S).ceil().max(0.0) as usize).min(n);
        i(t0)..i(t1)
    }

    /// The index of the record nearest `t` (0 when empty).
    fn index(&self, t: f64) -> usize {
        (((t - self.first_time()) / HOP_S).round().max(0.0) as usize).min(self.records.len().saturating_sub(1))
    }

    /// The record nearest `t`.
    fn at(&self, t: f64) -> Option<&HopRecord> {
        self.records.get(self.index(t))
    }
}

pub struct ViewState {
    pub open: bool,
    /// Layout and scale, saved with the settings.
    pub prefs: DirectorView,
    /// A line for the header for a few seconds (a session saved, or why not).
    pub notice: Option<(String, std::time::Instant)>,
    /// The end of the view when frozen; live (following the present) when None.
    frozen_at: Option<f64>,
    span_s: f64,
    lut: [Color32; 256],
    spec_img: ColorImage,
    lin_img: ColorImage,
    band_img: ColorImage,
    attack_img: ColorImage,
    spec_tex: Option<TextureHandle>,
    lin_tex: Option<TextureHandle>,
    band_tex: Option<TextureHandle>,
    attack_tex: Option<TextureHandle>,
    /// The images are rings of columns pinned to time: column k holds the
    /// records in k * ring_cw .. (k + 1) * ring_cw, in image column k mod
    /// COLS (ring_cols says which k each holds), so a frame works out and
    /// uploads only the columns that came into view and the newest two.
    ring_cw: f64,
    ring_cols: Vec<i64>,
    ring_latest: f64,
}

impl ViewState {
    pub fn new(prefs: DirectorView) -> Self {
        // AUDIOVIZ_AUDIO_VIEW=1 opens it at start (for snapshots); a number
        // of seconds from 2 opens it that wide.
        let env = std::env::var("AUDIOVIZ_AUDIO_VIEW").ok().and_then(|v| v.parse::<f64>().ok());
        Self {
            open: env.is_some_and(|v| v >= 1.0),
            prefs,
            notice: None,
            frozen_at: None,
            span_s: env.filter(|v| *v >= 2.0).map_or(10.0, |v| v.min(120.0)),
            lut: heat_lut(),
            spec_img: ColorImage::filled([COLS, SPEC_ROWS], Color32::BLACK),
            lin_img: ColorImage::filled([COLS, LIN_ROWS], Color32::BLACK),
            band_img: ColorImage::filled([COLS, RECORD_BANDS], Color32::BLACK),
            attack_img: ColorImage::filled([COLS, RECORD_BANDS], Color32::BLACK),
            spec_tex: None,
            lin_tex: None,
            band_tex: None,
            attack_tex: None,
            ring_cw: 0.0,
            ring_cols: vec![i64::MIN; COLS],
            ring_latest: 0.0,
        }
    }

    /// Works out image column `slot` from the records in time column `k`
    /// (`cw` seconds wide): the loudest record under it, or the hop it falls
    /// in when it is finer than a hop, or black outside the history.
    fn fill_column(&mut self, history: &History, slot: usize, k: i64, cw: f64, live_end: f64) {
        let (t0, t1) = (k as f64 * cw, (k + 1) as f64 * cw);
        let range = if history.records.is_empty() || t1 < history.first_time() - HOP_S || t0 > live_end {
            0..0
        } else {
            let r = history.range(t0, t1);
            if r.is_empty() {
                let i = history.index(0.5 * (t0 + t1));
                i..i + 1
            } else {
                r
            }
        };
        for row in 0..SPEC_ROWS {
            let v = range.clone().map(|i| history.records[i].spectrum[row]).max().unwrap_or(0);
            self.spec_img.pixels[(SPEC_ROWS - 1 - row) * COLS + slot] = self.lut[v as usize];
        }
        for row in 0..LIN_ROWS {
            let v = range.clone().map(|i| history.records[i].linear[row]).max().unwrap_or(0);
            self.lin_img.pixels[(LIN_ROWS - 1 - row) * COLS + slot] = self.lut[v as usize];
        }
        for b in 0..RECORD_BANDS {
            let row = (RECORD_BANDS - 1 - b) * COLS + slot;
            let v = range.clone().map(|i| history.records[i].levels[b]).fold(0.0f32, f32::max);
            self.band_img.pixels[row] = self.lut[(v.clamp(0.0, 1.0) * 255.0) as usize];
            let a = range.clone().map(|i| history.records[i].attacks[b]).fold(0.0f32, f32::max);
            self.attack_img.pixels[row] = self.lut[(a.clamp(0.0, 1.0) * 255.0) as usize];
        }
    }

    pub fn toggle_freeze(&mut self, history: &History) {
        self.frozen_at = match self.frozen_at {
            Some(_) => None,
            None => Some(history.latest_time()),
        };
    }

    /// Draws the view across the bottom of the window, `right` points short
    /// of the right edge (the panel) and `bottom` above it (the strip).
    pub fn draw(&mut self, ctx: &egui::Context, history: &History, health: &CaptureHealth, right: f32, bottom: f32) {
        let screen = ctx.content_rect();
        let floor = screen.bottom() - bottom;
        let h = (screen.height() * self.prefs.height).clamp(200.0, (floor - screen.top() - 40.0).max(200.0));
        let rect = Rect::from_min_max(pos2(screen.left(), floor - h), pos2(screen.right() - right, floor));
        egui::Area::new(egui::Id::unique("audio-view")).fixed_pos(rect.min).order(egui::Order::Foreground).show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 0.0, theme::with_alpha(BG, 235));
            p.line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, HAIRLINE));

            let live_end = history.latest_time();
            let earliest = history.first_time() + self.span_s.min(live_end - history.first_time());
            let plot = Rect::from_min_max(pos2(rect.left() + GUTTER, rect.top() + 24.0), pos2(rect.right() - 8.0, rect.bottom() - 6.0));
            // Lanes. A link in focus gets a lane of its own at the bottom.
            let has_probe = crate::links::PROBE.lock().ok().is_some_and(|p| p.is_some());
            let lanes = lanes(plot, self.prefs.spec_share, has_probe);
            let (spec_r, band_r, attack_r, onset_r, beat_r, level_r, context_r) = (lanes[0], lanes[1], lanes[2], lanes[3], lanes[4], lanes[5], lanes[6]);

            // The view's top edge sets its height; the line under the spectrogram its share.
            let top_handle = Rect::from_min_max(rect.left_top(), pos2(rect.right(), rect.top() + 6.0));
            let top = ui.interact(top_handle, ui.scope_id().with("height"), Sense::drag());
            if top.hovered() || top.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                p.line_segment([rect.left_top(), rect.right_top()], Stroke::new(2.0, theme::ACCENT));
            }
            if let (true, Some(pt)) = (top.dragged(), top.interact_pointer_pos()) {
                self.prefs.height = ((floor - pt.y) / screen.height()).clamp(0.2, 0.92);
            }
            let split_handle = Rect::from_min_max(pos2(plot.left(), spec_r.bottom() - 2.0), pos2(plot.right(), spec_r.bottom() + 4.0));
            let share = ui.interact(split_handle, ui.scope_id().with("spec-share"), Sense::drag());
            if share.hovered() || share.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                p.line_segment([pos2(plot.left(), spec_r.bottom() + 1.0), pos2(plot.right(), spec_r.bottom() + 1.0)], Stroke::new(2.0, theme::ACCENT));
            }
            if let (true, Some(pt)) = (share.dragged(), share.interact_pointer_pos()) {
                self.prefs.spec_share = ((pt.y - plot.top()) / plot.height()).clamp(0.12, 0.8);
            }
            // The scale's name, at the top of the frequency gutter, switches it.
            let scale_r = Rect::from_min_size(pos2(rect.left() + 4.0, spec_r.top()), vec2(GUTTER - 10.0, 14.0));
            let scale = ui.interact(scale_r, ui.scope_id().with("scale"), Sense::click());
            if scale.clicked() {
                self.prefs.linear = !self.prefs.linear;
            }
            let scale = scale.on_hover_text("Linear: every hertz the same height (the highs in detail). Log: every octave the same. L switches.");
            // The frequency gutter: the wheel zooms the range, a drag moves it, a double click resets it.
            let gutter_r = Rect::from_min_max(pos2(rect.left(), spec_r.top() + 15.0), pos2(spec_r.left(), spec_r.bottom()));
            let gutter = ui.interact(gutter_r, ui.scope_id().with("freq"), Sense::click_and_drag());
            let axis = FreqAxis::of(&self.prefs, spec_r);
            if gutter.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                let dy = ui.input(|i| i.smooth_scroll_delta.y);
                if dy != 0.0 {
                    if let Some(pt) = gutter.hover_pos() {
                        axis.zoom(&mut self.prefs, pt.y, (-dy * 0.003).exp());
                    }
                }
            }
            if gutter.dragged() {
                axis.pan(&mut self.prefs, gutter.drag_delta().y);
            }
            if gutter.double_clicked() {
                axis.reset(&mut self.prefs);
            }
            let gutter_busy = gutter.hovered() || gutter.dragged();
            let axis = FreqAxis::of(&self.prefs, spec_r);

            // Wheel zooms time, a drag scrolls back (and freezes); back at the present it follows again.
            if response.hovered() && !gutter_busy {
                let dy = ui.input(|i| i.smooth_scroll_delta.y);
                if dy != 0.0 {
                    self.span_s = (self.span_s * (-dy as f64 * 0.002).exp()).clamp(2.0, 120.0);
                }
            }
            if response.dragged() && !gutter_busy && !top.dragged() && !share.dragged() {
                let dx = response.drag_delta().x as f64;
                let end = self.frozen_at.unwrap_or(live_end) - dx / plot.width() as f64 * self.span_s;
                self.frozen_at = if end >= live_end { None } else { Some(end.max(earliest)) };
            }
            let end = self.frozen_at.unwrap_or(live_end);
            let start = end - self.span_s;

            // Header.
            let state = match self.frozen_at {
                None => "Live".to_string(),
                Some(t) => format!("Frozen {:.1} s back", live_end - t),
            };
            let groove = history.at(end).map_or(String::new(), |h| format!(" · {:.1} BPM, sure {:.2}, steady {:.2}, bar {:.2} · section {}{} · change in {:.1} bars ({:.2})", h.bpm, h.beat_confidence, h.pulse_stability, h.bar_confidence, (b'A' + h.section_label.min(25)) as char, if h.seen_before > 0.0 { format!(" seen {:.2}", h.seen_before) } else { String::new() }, h.bars_to_change, h.change_confidence));
            let head = format!(
                "Director view · {state} · {:.0} s across{groove} · capture: {} glitches, {} late, {} dropped, {} clipped, {} reopens",
                self.span_s, health.discontinuities, health.late_packets, health.dropped_chunks, health.clipped_samples, health.reopens
            );
            let head_w = p.text(pos2(rect.left() + 10.0, rect.top() + 12.0), Align2::LEFT_CENTER, head, theme::mono(11.0), TEXT_2).width();
            // The keys, where the header leaves room for them.
            if let Some((text, _)) = self.notice.as_ref().filter(|(_, at)| at.elapsed().as_secs_f32() < 6.0) {
                // A notice shows whatever the header's length, over its end.
                let g = p.layout_no_wrap(text.clone(), theme::mono(11.0), theme::ACCENT);
                let at = pos2(rect.right() - 10.0 - g.size().x, rect.top() + 12.0 - g.size().y * 0.5);
                p.rect_filled(Rect::from_min_size(at - vec2(8.0, 2.0), g.size() + vec2(12.0, 4.0)), 3.0, BG);
                p.galley(at, g, theme::ACCENT);
            } else {
                let keys = p.layout_no_wrap("D close · F freeze · S save session · L lin/log · drag back · wheel zoom".into(), theme::mono(11.0), TEXT_3);
                if head_w + keys.size().x + 40.0 < rect.width() {
                    p.galley(pos2(rect.right() - 10.0 - keys.size().x, rect.top() + 12.0 - keys.size().y * 0.5), keys, TEXT_3);
                }
            }

            let probe = crate::links::PROBE.lock().ok().and_then(|p| p.as_ref().map(|p| (p.label.clone(), p.low, p.high, p.samples.iter().filter(|(t, _)| *t >= start - 1.0).copied().collect::<Vec<_>>())));
            let span = self.span_s;
            let x_of = |t: f64| plot.left() + ((t - start) / span) as f32 * plot.width();
            // The records under each column; a column finer than a hop shows
            // the hop it falls in, and one outside the history shows nothing.
            let cols: Vec<std::ops::Range<usize>> = (0..COLS)
                .map(|c| {
                    let t0 = start + span * c as f64 / COLS as f64;
                    let t1 = start + span * (c + 1) as f64 / COLS as f64;
                    if history.records.is_empty() || t1 < history.first_time() - HOP_S || t0 > live_end {
                        return 0..0;
                    }
                    let r = history.range(t0, t1);
                    if !r.is_empty() {
                        return r;
                    }
                    let i = history.index(0.5 * (t0 + t1));
                    i..i + 1
                })
                .collect();
            let col_x = |c: usize| plot.left() + (c as f32 + 0.5) / COLS as f32 * plot.width();
            let col_record = |c: usize| -> Option<&HopRecord> { history.records.get(cols[c].start).filter(|_| !cols[c].is_empty()) };
            let col_max = |c: usize, f: &dyn Fn(&HopRecord) -> f32| -> f32 { cols[c].clone().map(|i| f(&history.records[i])).fold(0.0f32, f32::max) };

            // Spectrogram and bands as images: the loudest record under each
            // column, kept as rings (see ring_cw). A span of COLS - 2 columns
            // leaves the partly shown columns at both ends their own slots.
            let cw = self.span_s / (COLS - 2) as f64;
            if cw != self.ring_cw || live_end < self.ring_latest {
                // Zoomed, or the history started again: draw every column afresh.
                self.ring_cw = cw;
                self.ring_cols.fill(i64::MIN);
            }
            self.ring_latest = live_end;
            let (k_first, k_end, k_live) = ((start / cw).floor() as i64, (end / cw).floor() as i64, (live_end / cw).floor() as i64);
            let mut dirty: Vec<usize> = Vec::new();
            for k in k_first..=k_end {
                let slot = k.rem_euclid(COLS as i64) as usize;
                // The newest two columns are still filling.
                if self.ring_cols[slot] == k && k < k_live - 1 {
                    continue;
                }
                self.ring_cols[slot] = k;
                self.fill_column(history, slot, k, cw, live_end);
                dirty.push(slot);
            }
            let spec_tex = if self.prefs.linear { upload(ctx, &mut self.lin_tex, "audio-view-linear", &self.lin_img, &dirty) } else { upload(ctx, &mut self.spec_tex, "audio-view-spec", &self.spec_img, &dirty) };
            // The other spectrogram keeps up too, so switching scales shows it at once.
            if self.prefs.linear {
                upload(ctx, &mut self.spec_tex, "audio-view-spec", &self.spec_img, &dirty);
            } else {
                upload(ctx, &mut self.lin_tex, "audio-view-linear", &self.lin_img, &dirty);
            }
            let band_tex = upload(ctx, &mut self.band_tex, "audio-view-bands", &self.band_img, &dirty);
            let attack_tex = upload(ctx, &mut self.attack_tex, "audio-view-attacks", &self.attack_img, &dirty);
            // Time t sits at u = t / cw / COLS (the texture repeats); a whole
            // number of turns is taken off so the numbers stay small.
            let base = (k_first - k_first.rem_euclid(COLS as i64)) as f64;
            let u = |t: f64| ((t / cw - base) / COLS as f64) as f32;
            let uv = Rect::from_min_max(pos2(u(start), 0.0), pos2(u(end), 1.0));
            // The spectrogram shows the axis's frequency range of its image.
            let (v0, v1) = axis.image_v();
            p.image(spec_tex, spec_r, Rect::from_min_max(pos2(uv.min.x, v0), pos2(uv.max.x, v1)), Color32::WHITE);
            p.image(band_tex, band_r, uv, Color32::WHITE);
            p.image(attack_tex, attack_r, uv, Color32::WHITE);
            // Frequency guides on the spectrogram, the scale's name and the dB range.
            for hz in axis.guides() {
                let y = axis.y_of(hz);
                p.line_segment([pos2(spec_r.left(), y), pos2(spec_r.left() + 6.0, y)], Stroke::new(1.0, TEXT_3));
                let label = if hz >= 1000.0 { format!("{}k", hz / 1000.0) } else { format!("{hz}") };
                p.text(pos2(spec_r.left() - 6.0, y), Align2::RIGHT_CENTER, label, theme::mono(9.0), TEXT_3);
            }
            let scale_c = if scale.hovered() { TEXT } else { theme::ACCENT };
            p.text(scale_r.left_top(), Align2::LEFT_TOP, if self.prefs.linear { "Linear" } else { "Log" }, theme::sans(10.5), scale_c);
            p.text(pos2(spec_r.left() - GUTTER + 8.0, spec_r.bottom()), Align2::LEFT_BOTTOM, format!("{:.0}..0 dB", SPEC_FLOOR_DB), theme::mono(9.0), TEXT_3);
            lane_label(&p, band_r, "Levels");
            lane_label(&p, attack_r, "Attacks");

            // Onsets: strength (bright) against threshold (dim), hits as ticks.
            let sub = split(onset_r, &[0.25; 4]);
            for g in 0..4 {
                let r = sub[g];
                p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, HAIRLINE));
                let y = |v: f32| r.bottom() - (v / 1.5).clamp(0.0, 1.0) * (r.height() - 2.0);
                let curve = |f: &dyn Fn(&HopRecord) -> f32| -> Vec<Pos2> { (0..COLS).filter(|c| !cols[*c].is_empty()).map(|c| pos2(col_x(c), y(col_max(c, f)))).collect() };
                p.add(Shape::line(curve(&|h: &HopRecord| h.onset_threshold[g]), Stroke::new(1.0, theme::with_alpha(GROUP_COLOURS[g], 70))));
                p.add(Shape::line(curve(&|h: &HopRecord| h.onset_strength[g]), Stroke::new(1.0, GROUP_COLOURS[g])));
                for i in history.range(start, end) {
                    let h = &history.records[i];
                    if h.onset_hits & (1 << g) != 0 {
                        let x = x_of(h.time_s);
                        p.line_segment([pos2(x, r.top() + 1.0), pos2(x, r.top() + r.height() * 0.35)], Stroke::new(2.0, GROUP_COLOURS[g]));
                    }
                }
                lane_label(&p, r, GROUP_NAMES[g]);
            }

            // Beat clock: phase as a sawtooth, confidence as a line, beats as a grid.
            let yb = |v: f32| beat_r.bottom() - v.clamp(0.0, 1.0) * (beat_r.height() - 2.0);
            let mut saw: Vec<Pos2> = Vec::new();
            let mut conf: Vec<Pos2> = Vec::new();
            for c in 0..COLS {
                if let Some(h) = col_record(c) {
                    let x = col_x(c);
                    if saw.last().is_some_and(|l| yb(h.beat_phase) > l.y + beat_r.height() * 0.5) {
                        p.add(Shape::line(std::mem::take(&mut saw), Stroke::new(1.0, theme::with_alpha(BEAT, 110))));
                    }
                    saw.push(pos2(x, yb(h.beat_phase)));
                    conf.push(pos2(x, yb(h.beat_confidence)));
                }
            }
            p.add(Shape::line(saw, Stroke::new(1.0, theme::with_alpha(BEAT, 110))));
            p.add(Shape::line(conf, Stroke::new(1.5, TEXT)));
            for i in history.range(start, end) {
                let h = &history.records[i];
                if h.beat {
                    let x = x_of(h.time_s);
                    let a = (40.0 + 120.0 * h.beat_confidence) as u8;
                    if h.beat_in_bar == 0 && h.bar_confidence > 0.2 {
                        // The bar's first beat: a brighter line, and its number.
                        p.line_segment([pos2(x, spec_r.top()), pos2(x, beat_r.bottom())], Stroke::new(1.0, theme::with_alpha(BEAT, (40.0 + 160.0 * h.bar_confidence) as u8)));
                        p.text(pos2(x + 2.0, beat_r.top()), Align2::LEFT_TOP, "1", theme::mono(9.0), theme::with_alpha(BEAT, (80.0 + 170.0 * h.bar_confidence) as u8));
                    }
                    p.line_segment([pos2(x, spec_r.top()), pos2(x, beat_r.bottom())], Stroke::new(1.0, theme::with_alpha(BEAT, a / 3)));
                    p.line_segment([pos2(x, beat_r.top()), pos2(x, beat_r.bottom())], Stroke::new(1.5, theme::with_alpha(BEAT, a)));
                }
            }
            lane_label(&p, beat_r, "Beat");

            // Level: RMS in dB (-60..0) and the energy trend about the middle.
            let yl = |v: f32| level_r.bottom() - v.clamp(0.0, 1.0) * (level_r.height() - 2.0);
            let mut lvl = Vec::new();
            let mut trend = Vec::new();
            for c in 0..COLS {
                if let Some(h) = col_record(c) {
                    let x = col_x(c);
                    lvl.push(pos2(x, yl((20.0 * h.rms.max(1e-6).log10() + 60.0) / 60.0)));
                    trend.push(pos2(x, yl(0.5 + 0.5 * h.energy_trend)));
                }
            }
            p.line_segment([pos2(level_r.left(), yl(0.5)), pos2(level_r.right(), yl(0.5))], Stroke::new(1.0, HAIRLINE));
            p.add(Shape::line(trend, Stroke::new(1.0, theme::with_alpha(GROUP_COLOURS[1], 140))));
            // Noisiness and activity, 0..1 across the lane.
            let mut noisy = Vec::new();
            let mut busy = Vec::new();
            for c in 0..COLS {
                if let Some(h) = col_record(c) {
                    noisy.push(pos2(col_x(c), yl(h.noisiness)));
                    busy.push(pos2(col_x(c), yl(h.activity)));
                }
            }
            p.add(Shape::line(noisy, Stroke::new(1.0, theme::with_alpha(NOISY, 170))));
            p.add(Shape::line(busy, Stroke::new(1.0, theme::with_alpha(GROUP_COLOURS[2], 170))));
            p.add(Shape::line(lvl, Stroke::new(1.5, TEXT)));
            lane_label(&p, level_r, "Level");

            // Context: build intensity filled, novelty as a line, drops and
            // transitions as markers through every lane.
            let yc = |v: f32| context_r.bottom() - v.clamp(0.0, 1.0) * (context_r.height() - 2.0);
            let mut novelty = Vec::new();
            // The build as one mesh of column-wide bars (a shape per column cost a lot to tessellate).
            let mut build = egui::Mesh::default();
            let half = 0.5 * plot.width() / COLS as f32 + 0.25;
            for c in 0..COLS {
                if let Some(h) = col_record(c) {
                    let x = col_x(c);
                    if h.build > 0.01 {
                        build.add_colored_rect(Rect::from_min_max(pos2(x - half, yc(h.build)), pos2(x + half, yc(0.0))), theme::with_alpha(BUILD, 150));
                    }
                    novelty.push(pos2(x, yc(h.transition / 1.5)));
                }
            }
            p.add(Shape::mesh(build));
            p.add(Shape::line(novelty, Stroke::new(1.0, TEXT_2)));
            let range = history.range(start, end);
            for i in range.clone().skip(1) {
                let (a, h) = (&history.records[i - 1], &history.records[i]);
                let x = x_of(h.time_s);
                if h.transitions != a.transitions {
                    p.line_segment([pos2(x, spec_r.top()), pos2(x, context_r.bottom())], Stroke::new(1.0, theme::with_alpha(TEXT, 90)));
                    p.text(pos2(x + 3.0, context_r.top()), Align2::LEFT_TOP, "change", theme::sans(10.0), TEXT_2);
                }
                if h.section_label != a.section_label {
                    p.text(pos2(x + 3.0, context_r.bottom() - 2.0), Align2::LEFT_BOTTOM, ((b'A' + h.section_label.min(25)) as char).to_string(), theme::mono(10.0), theme::with_alpha(TEXT, 200));
                }
                if h.drops != a.drops {
                    p.line_segment([pos2(x, spec_r.top()), pos2(x, context_r.bottom())], Stroke::new(2.0, theme::with_alpha(DROP, 220)));
                    p.text(pos2(x + 3.0, context_r.top() + 12.0), Align2::LEFT_TOP, format!("Drop {:.0}%", h.drop_confidence * 100.0), theme::sans(10.0), DROP);
                }
            }
            lane_label(&p, context_r, "Context");

            // The link: the signal as it came (dim), past the gate and the
            // threshold (mid), shaped and smoothed (bright), and the value.
            if let (Some((label, low, high, samples)), Some(link_r)) = (&probe, lanes.get(7).copied()) {
                let y = |v: f32| link_r.bottom() - v.clamp(0.0, 1.0) * (link_r.height() - 2.0);
                let line = |f: &dyn Fn(&crate::links::Step) -> f32| -> Vec<Pos2> { samples.iter().filter(|(t, _)| *t >= start && *t <= end).map(|(t, s)| pos2(x_of(*t), y(f(s)))).collect() };
                p.add(Shape::line(line(&|s| s.raw), Stroke::new(1.0, theme::with_alpha(TEXT, 60))));
                p.add(Shape::line(line(&|s| s.passed), Stroke::new(1.0, theme::with_alpha(TEXT, 130))));
                p.add(Shape::line(line(&|s| s.shaped), Stroke::new(1.5, theme::ACCENT)));
                lane_label(&p, link_r, "Link");
                let last = samples.iter().rev().find(|(t, _)| *t <= end).map_or(String::new(), |(_, s)| format!(" = {:.2}", s.value));
                p.text(pos2(link_r.left() + 4.0, link_r.top() + 1.0), Align2::LEFT_TOP, format!("{label} ({low:.2} .. {high:.2}){last}"), theme::sans(10.5), TEXT);
                p.text(pos2(link_r.left() - GUTTER + 8.0, link_r.bottom()), Align2::LEFT_BOTTOM, "in · gated · out", theme::sans(9.0), TEXT_3);
            }
            p.text(pos2(context_r.left() - GUTTER + 8.0, context_r.bottom()), Align2::LEFT_BOTTOM, "build · novelty", theme::sans(9.0), TEXT_3);
            p.text(pos2(level_r.left() - GUTTER + 8.0, level_r.bottom()), Align2::LEFT_BOTTOM, "noise · busy", theme::sans(9.0), TEXT_3);

            // Track changes.
            for (t, label) in &history.marks {
                if (start..=end).contains(t) {
                    let x = x_of(*t);
                    p.line_segment([pos2(x, plot.top()), pos2(x, plot.bottom())], Stroke::new(1.0, TEXT_2));
                    p.text(pos2(x + 4.0, plot.top() + 2.0), Align2::LEFT_TOP, label, theme::sans(11.0), TEXT);
                }
            }

            // Hover: a cursor and what every lane read there.
            if let Some(pt) = response.hover_pos().filter(|pt| plot.contains(*pt)) {
                let t = start + ((pt.x - plot.left()) / plot.width()) as f64 * self.span_s;
                if let Some(h) = history.at(t) {
                    p.line_segment([pos2(pt.x, plot.top()), pos2(pt.x, plot.bottom())], Stroke::new(1.0, theme::with_alpha(TEXT, 120)));
                    let mut hz = if spec_r.contains(pt) { format!(" · {:.0} Hz", axis.hz_at(pt.y)) } else { String::new() };
                    // Over the band lanes: that band's three values.
                    for r in [band_r, attack_r] {
                        if r.contains(pt) {
                            let b = (((r.bottom() - pt.y) / r.height() * RECORD_BANDS as f32) as usize).min(RECORD_BANDS - 1);
                            hz = format!(" · band {b}: relative {:.2}, level {:.2}, attack {:.2}", h.bands[b], h.levels[b], h.attacks[b]);
                        }
                    }
                    let read = format!(
                        "{:.2} s back{hz} · onsets {:.2} {:.2} {:.2} {:.2} · {:.1} BPM, conf {:.2}, phase {:.2} · sub {:.2} bass {:.2} low mid {:.2} mid {:.2} presence {:.2} air {:.2} · {:.0} Hz centroid · noise {:.2} · busy {:.2} · width {:.2}, balance {:+.2} · build {:.2}, bass {:.2}, contrast {:.2}",
                        live_end - h.time_s, h.onset_strength[0], h.onset_strength[1], h.onset_strength[2], h.onset_strength[3], h.bpm, h.beat_confidence, h.beat_phase, h.broad_level[0], h.broad_level[1], h.broad_level[2], h.broad_level[3], h.broad_level[4], h.broad_level[5], h.centroid_hz, h.noisiness, h.activity, h.stereo_width, h.balance, h.build, h.bass_presence, h.contrast
                    );
                    let galley = p.layout_no_wrap(read, theme::mono(11.0), TEXT);
                    let at = pos2((pt.x + 10.0).min(rect.right() - galley.size().x - 8.0), plot.top() + 4.0);
                    p.rect_filled(Rect::from_min_size(at - vec2(4.0, 2.0), galley.size() + vec2(8.0, 4.0)), 3.0, theme::with_alpha(BG, 220));
                    p.galley(at, galley, TEXT);
                }
            }
        });
    }
}

/// Sends an image's changed columns to its texture (all of it the first
/// time, or when much of it changed), repeating, and returns its id.
fn upload(ctx: &egui::Context, tex: &mut Option<TextureHandle>, name: &str, img: &ColorImage, dirty: &[usize]) -> egui::TextureId {
    let opts = TextureOptions::LINEAR_REPEAT;
    match tex {
        None => *tex = Some(ctx.load_texture(name, img.clone(), opts)),
        Some(t) if dirty.len() > COLS / 4 => t.set(img.clone(), opts),
        Some(t) => {
            let mut cols = dirty.to_vec();
            cols.sort_unstable();
            let (w, h) = (img.size[0], img.size[1]);
            let mut i = 0;
            while i < cols.len() {
                // A run of neighbouring columns goes as one piece.
                let mut j = i + 1;
                while j < cols.len() && cols[j] == cols[j - 1] + 1 {
                    j += 1;
                }
                let (x0, n) = (cols[i], j - i);
                let pixels = (0..h).flat_map(|y| img.pixels[y * w + x0..y * w + x0 + n].iter().copied()).collect();
                t.set_partial([x0, 0], ColorImage::new([n, h], pixels), opts);
                i = j;
            }
        }
    }
    tex.as_ref().map_or(egui::TextureId::default(), TextureHandle::id)
}

/// The lanes: the spectrogram's share of the plot first, the others sharing
/// the rest as they always have, a link lane last when one is in focus.
fn lanes(plot: Rect, spec_share: f32, probe: bool) -> Vec<Rect> {
    let rest: &[f32] = if probe { &[0.09, 0.06, 0.23, 0.08, 0.08, 0.09, 0.11] } else { &[0.10, 0.07, 0.26, 0.09, 0.09, 0.10] };
    let total: f32 = rest.iter().sum();
    let s = spec_share.clamp(0.05, 0.95);
    let fractions: Vec<f32> = std::iter::once(s).chain(rest.iter().map(|f| f / total * (1.0 - s))).collect();
    split(plot, &fractions)
}

/// The spectrogram's frequency axis: its scale and range and the lane it
/// is drawn in.
struct FreqAxis {
    linear: bool,
    lo: f32,
    hi: f32,
    lane: Rect,
}

impl FreqAxis {
    fn of(prefs: &DirectorView, lane: Rect) -> Self {
        let (lo, hi) = if prefs.linear { (prefs.lin_lo_hz, prefs.lin_hi_hz) } else { (prefs.log_lo_hz, prefs.log_hi_hz) };
        Self { linear: prefs.linear, lo, hi, lane }
    }

    /// Hertz to the scale's own measure (linear, or ln).
    fn m(&self, hz: f32) -> f32 {
        if self.linear { hz } else { hz.max(1.0).ln() }
    }

    fn unm(&self, m: f32) -> f32 {
        if self.linear { m } else { m.exp() }
    }

    fn y_of(&self, hz: f32) -> f32 {
        let f = (self.m(hz) - self.m(self.lo)) / (self.m(self.hi) - self.m(self.lo)).max(1e-6);
        self.lane.bottom() - f * self.lane.height()
    }

    fn hz_at(&self, y: f32) -> f32 {
        let f = (self.lane.bottom() - y) / self.lane.height();
        self.unm(self.m(self.lo) + f * (self.m(self.hi) - self.m(self.lo)))
    }

    /// The image's v range for the range shown (the image's top row is the
    /// highest frequency).
    fn image_v(&self) -> (f32, f32) {
        if self.linear {
            (1.0 - self.hi / LIN_HI_HZ, 1.0 - self.lo / LIN_HI_HZ)
        } else {
            let span = SPEC_HI_HZ.ln() - SPEC_LO_HZ.ln();
            let v = |hz: f32| 1.0 - (hz.ln() - SPEC_LO_HZ.ln()) / span;
            (v(self.hi), v(self.lo))
        }
    }

    /// The scale's limits and the narrowest range it shows.
    fn limits(&self) -> (f32, f32, f32) {
        if self.linear { (0.0, LIN_HI_HZ, 500.0) } else { (SPEC_LO_HZ.ln(), SPEC_HI_HZ.ln(), 1.5f32.ln()) }
    }

    fn set(&self, prefs: &mut DirectorView, m_lo: f32, m_hi: f32) {
        let (min, max, narrowest) = self.limits();
        let width = (m_hi - m_lo).clamp(narrowest, max - min);
        let lo = m_lo.clamp(min, max - width);
        let (lo, hi) = (self.unm(lo), self.unm(lo + width));
        if self.linear {
            (prefs.lin_lo_hz, prefs.lin_hi_hz) = (lo, hi);
        } else {
            (prefs.log_lo_hz, prefs.log_hi_hz) = (lo, hi);
        }
    }

    /// Zooms by `factor` (under 1 narrows) about the frequency at `y`.
    fn zoom(&self, prefs: &mut DirectorView, y: f32, factor: f32) {
        let at = self.m(self.hz_at(y));
        let (lo, hi) = (self.m(self.lo), self.m(self.hi));
        self.set(prefs, at - (at - lo) * factor, at + (hi - at) * factor);
    }

    /// Moves the range by a drag of `dy` points (down shows higher).
    fn pan(&self, prefs: &mut DirectorView, dy: f32) {
        let per_point = (self.m(self.hi) - self.m(self.lo)) / self.lane.height().max(1.0);
        self.set(prefs, self.m(self.lo) + dy * per_point, self.m(self.hi) + dy * per_point);
    }

    fn reset(&self, prefs: &mut DirectorView) {
        let d = DirectorView::default();
        if self.linear {
            (prefs.lin_lo_hz, prefs.lin_hi_hz) = (d.lin_lo_hz, d.lin_hi_hz);
        } else {
            (prefs.log_lo_hz, prefs.log_hi_hz) = (d.log_lo_hz, d.log_hi_hz);
        }
    }

    /// Where to put labelled guides: round numbers, about five to nine of them.
    fn guides(&self) -> Vec<f32> {
        if self.linear {
            let span = self.hi - self.lo;
            let step = [50.0f32, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 2500.0, 5000.0].into_iter().find(|s| span / s <= 9.0).unwrap_or(5000.0);
            let first = (self.lo / step).ceil() as i32;
            let last = (self.hi / step).floor() as i32;
            (first..=last).map(|k| k as f32 * step).filter(|hz| *hz > self.lo + 1.0).collect()
        } else {
            let all: Vec<f32> = [10.0f32, 100.0, 1000.0, 10_000.0].iter().flat_map(|d| [1.0, 2.0, 3.0, 5.0].map(|k| k * d)).filter(|hz| *hz >= self.lo && *hz <= self.hi).collect();
            // Thin to at most nine, keeping the 1-2-5 ones first.
            if all.len() <= 9 {
                all
            } else {
                all.into_iter().filter(|hz| { let m = hz / 10f32.powf(hz.log10().floor()); (m - 3.0).abs() > 0.01 }).collect()
            }
        }
    }
}

/// Splits a rect into horizontal lanes by fractions of its height, 3 px apart.
fn split(r: Rect, fractions: &[f32]) -> Vec<Rect> {
    let gap = 3.0;
    let total = r.height() - gap * (fractions.len() - 1) as f32;
    let mut y = r.top();
    fractions
        .iter()
        .map(|f| {
            let lane = Rect::from_min_size(pos2(r.left(), y), vec2(r.width(), total * f));
            y += total * f + gap;
            lane
        })
        .collect()
}

fn lane_label(p: &egui::Painter, r: Rect, label: &str) {
    p.text(pos2(r.left() - GUTTER + 8.0, r.top() + 2.0), Align2::LEFT_TOP, label, theme::sans(11.0), TEXT_2);
}

/// Black through violet and orange to pale yellow, for dB and band heat.
fn heat_lut() -> [Color32; 256] {
    let stops: [(f32, [f32; 3]); 6] = [
        (0.0, [0.0, 0.0, 0.02]),
        (0.25, [0.12, 0.04, 0.30]),
        (0.5, [0.55, 0.10, 0.45]),
        (0.7, [0.92, 0.30, 0.25]),
        (0.87, [0.99, 0.65, 0.20]),
        (1.0, [0.99, 0.98, 0.75]),
    ];
    std::array::from_fn(|i| {
        let t = i as f32 / 255.0;
        let k = stops.iter().rposition(|(s, _)| *s <= t).unwrap_or(0).min(stops.len() - 2);
        let (t0, c0) = stops[k];
        let (t1, c1) = stops[k + 1];
        let f = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
        let ch = |j: usize| ((c0[j] + (c1[j] - c0[j]) * f) * 255.0) as u8;
        Color32::from_rgb(ch(0), ch(1), ch(2))
    })
}
