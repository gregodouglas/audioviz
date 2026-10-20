//! The director view (ADR-0006): what the engine hears, laid out on one time
//! axis for whoever makes visuals (the audience never sees it). Lanes, top to
//! bottom: a log-frequency spectrogram in dB, the 32 bands as the visuals get
//! them, onset strength against its threshold with the hits (low, mid, high,
//! all), the beat clock (phase, confidence, the beats as a grid through every
//! lane) and the level. It reads the engine's history, so it can be frozen
//! (F), dragged back over the last five minutes and zoomed (the wheel).
//! D opens and closes it.

use std::collections::VecDeque;

use audioviz_analysis::history::{HopRecord, RECORD_BANDS, SPEC_FLOOR_DB, SPEC_HI_HZ, SPEC_LO_HZ, SPEC_ROWS};
use audioviz_audio::CaptureHealth;
use egui::{Align2, Color32, ColorImage, Pos2, Rect, Sense, Shape, Stroke, TextureHandle, TextureOptions, pos2, vec2};

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
    /// The end of the view when frozen; live (following the present) when None.
    frozen_at: Option<f64>,
    span_s: f64,
    lut: [Color32; 256],
    spec_img: ColorImage,
    band_img: ColorImage,
    spec_tex: Option<TextureHandle>,
    band_tex: Option<TextureHandle>,
}

impl ViewState {
    pub fn new() -> Self {
        Self {
            // AUDIOVIZ_AUDIO_VIEW=1 opens it at start (for snapshots).
            open: std::env::var("AUDIOVIZ_AUDIO_VIEW").is_ok_and(|v| v == "1"),
            frozen_at: None,
            span_s: 10.0,
            lut: heat_lut(),
            spec_img: ColorImage::filled([COLS, SPEC_ROWS], Color32::BLACK),
            band_img: ColorImage::filled([COLS, RECORD_BANDS], Color32::BLACK),
            spec_tex: None,
            band_tex: None,
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
        let h = (screen.height() * 0.5).clamp(260.0, 560.0);
        let rect = Rect::from_min_max(pos2(screen.left(), screen.bottom() - bottom - h), pos2(screen.right() - right, screen.bottom() - bottom));
        egui::Area::new(egui::Id::unique("audio-view")).fixed_pos(rect.min).order(egui::Order::Foreground).show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 0.0, theme::with_alpha(BG, 235));
            p.line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, HAIRLINE));

            let live_end = history.latest_time();
            let earliest = history.first_time() + self.span_s.min(live_end - history.first_time());
            // Wheel zooms, a drag scrolls back (and freezes); back at the present it follows again.
            if response.hovered() {
                let dy = ui.input(|i| i.smooth_scroll_delta.y);
                if dy != 0.0 {
                    self.span_s = (self.span_s * (-dy as f64 * 0.002).exp()).clamp(2.0, 120.0);
                }
            }
            let plot = Rect::from_min_max(pos2(rect.left() + GUTTER, rect.top() + 24.0), pos2(rect.right() - 8.0, rect.bottom() - 6.0));
            if response.dragged() {
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
            let head = format!(
                "Director view · {state} · {:.0} s across · capture: {} glitches, {} late, {} dropped, {} clipped, {} reopens",
                self.span_s, health.discontinuities, health.late_packets, health.dropped_chunks, health.clipped_samples, health.reopens
            );
            p.text(pos2(rect.left() + 10.0, rect.top() + 12.0), Align2::LEFT_CENTER, head, theme::mono(11.0), TEXT_2);
            p.text(pos2(rect.right() - 10.0, rect.top() + 12.0), Align2::RIGHT_CENTER, "D close · F freeze · drag back · wheel zoom", theme::mono(11.0), TEXT_3);

            // Lanes.
            let lanes = split(plot, &[0.36, 0.14, 0.30, 0.10, 0.10]);
            let (spec_r, band_r, onset_r, beat_r, level_r) = (lanes[0], lanes[1], lanes[2], lanes[3], lanes[4]);
            let x_of = |t: f64| plot.left() + ((t - start) / self.span_s) as f32 * plot.width();
            // The records under each column; a column finer than a hop shows
            // the hop it falls in, and one outside the history shows nothing.
            let cols: Vec<std::ops::Range<usize>> = (0..COLS)
                .map(|c| {
                    let t0 = start + self.span_s * c as f64 / COLS as f64;
                    let t1 = start + self.span_s * (c + 1) as f64 / COLS as f64;
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

            // Spectrogram and bands as images: the loudest record under each column.
            for c in 0..COLS {
                let range = cols[c].clone();
                for row in 0..SPEC_ROWS {
                    let v = range.clone().map(|i| history.records[i].spectrum[row]).max().unwrap_or(0);
                    self.spec_img.pixels[(SPEC_ROWS - 1 - row) * COLS + c] = self.lut[v as usize];
                }
                for b in 0..RECORD_BANDS {
                    let v = range.clone().map(|i| history.records[i].bands[b]).fold(0.0f32, f32::max);
                    self.band_img.pixels[(RECORD_BANDS - 1 - b) * COLS + c] = self.lut[((v / 1.2).clamp(0.0, 1.0) * 255.0) as usize];
                }
            }
            let opts = TextureOptions::LINEAR;
            let spec_tex = self.spec_tex.get_or_insert_with(|| ctx.load_texture("audio-view-spec", self.spec_img.clone(), opts));
            spec_tex.set(self.spec_img.clone(), opts);
            let band_tex = self.band_tex.get_or_insert_with(|| ctx.load_texture("audio-view-bands", self.band_img.clone(), opts));
            band_tex.set(self.band_img.clone(), opts);
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            p.image(spec_tex.id(), spec_r, uv, Color32::WHITE);
            p.image(band_tex.id(), band_r, uv, Color32::WHITE);
            // Frequency guides on the spectrogram.
            for hz in [50.0f32, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
                let f = (hz.ln() - SPEC_LO_HZ.ln()) / (SPEC_HI_HZ.ln() - SPEC_LO_HZ.ln());
                let y = spec_r.bottom() - f * spec_r.height();
                p.line_segment([pos2(spec_r.left(), y), pos2(spec_r.left() + 6.0, y)], Stroke::new(1.0, TEXT_3));
                let label = if hz >= 1000.0 { format!("{}k", hz / 1000.0) } else { format!("{hz}") };
                p.text(pos2(spec_r.left() - 6.0, y), Align2::RIGHT_CENTER, label, theme::mono(9.0), TEXT_3);
            }
            p.text(pos2(spec_r.left() - GUTTER + 8.0, spec_r.bottom()), Align2::LEFT_BOTTOM, format!("{:.0} dB", SPEC_FLOOR_DB), theme::mono(9.0), TEXT_3);
            p.text(pos2(spec_r.left() - GUTTER + 8.0, spec_r.top()), Align2::LEFT_TOP, "0 dB", theme::mono(9.0), TEXT_3);
            lane_label(&p, band_r, "Bands");

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
                    p.line_segment([pos2(x, spec_r.top()), pos2(x, beat_r.bottom())], Stroke::new(1.0, theme::with_alpha(BEAT, a / 3)));
                    p.line_segment([pos2(x, beat_r.top()), pos2(x, beat_r.bottom())], Stroke::new(1.5, theme::with_alpha(BEAT, a)));
                }
            }
            let bpm = history.at(end).map_or(String::new(), |h| format!("{:.1} BPM", h.bpm));
            lane_label(&p, beat_r, &format!("Beat {bpm}"));

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
            p.add(Shape::line(lvl, Stroke::new(1.5, TEXT)));
            lane_label(&p, level_r, "Level");

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
                    let f = (spec_r.bottom() - pt.y) / spec_r.height();
                    let hz = if spec_r.contains(pt) { format!(" · {:.0} Hz", (SPEC_LO_HZ.ln() + f * (SPEC_HI_HZ.ln() - SPEC_LO_HZ.ln())).exp()) } else { String::new() };
                    let read = format!(
                        "{:.2} s back{hz} · onsets {:.2} {:.2} {:.2} {:.2} · {:.1} BPM, conf {:.2}, phase {:.2} · bass {:.2} mid {:.2} treb {:.2} · {:.0} Hz centroid · width {:.2}",
                        live_end - h.time_s, h.onset_strength[0], h.onset_strength[1], h.onset_strength[2], h.onset_strength[3], h.bpm, h.beat_confidence, h.beat_phase, h.bass, h.mid, h.treb, h.centroid_hz, h.stereo_width
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
