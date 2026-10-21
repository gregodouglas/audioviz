//! Sections from the track's own history (ADR-0006 step 4): the live stream
//! has no future, but songs repeat themselves. Every bar leaves a summary
//! (the context's description of the sound, averaged over the bar). Two
//! things close the current section: a transition (heard about 1.5 s late,
//! so the section ends at the bar where the change began), and a peak of the
//! bars' own novelty (the four bars after a bar line against the four before
//! it, known four bars later; inside a section whose chords go round every
//! two or four bars it stays low, so chord changes do not cut it up). The
//! finished section gets a label:
//! the label of an earlier section it sounds like, or a new one (A, B, C ...).
//! While a section plays it is compared with the finished ones; if it sounds
//! like one, it is "seen before" and expected to last as long as that one
//! did, which predicts the next change in bars. Otherwise the next change is
//! expected at the next eight-bar phrase, with low confidence.

use std::collections::VecDeque;

use crate::context::{DIMS, distance};

/// Two sections closer than this sound alike (on made-up songs the same
/// section came back within 0.056 to 0.077, different ones 0.097 or more).
const MATCH_DIST: f32 = 0.085;
/// Bars kept.
const KEEP_BARS: usize = 512;
/// A section shorter than this is folded into the next.
const MIN_BARS: usize = 2;
/// The change is heard this long after the bar it started at.
const LATE_S: f64 = 1.5;
const PHRASE_BARS: usize = 8;
/// Bars on each side of a bar line for the bars' novelty.
const SIDE: usize = 4;
/// A bar line is a boundary when its novelty peaks this many times over
/// the lowest of the last eight lines, and by `PEAK_BY` at least.
const PEAK_OVER: f32 = 1.4;
const PEAK_BY: f32 = 0.03;

struct Section {
    label: u32,
    bars: usize,
    /// The first section of a track began before its bars were counted: its
    /// length says nothing.
    partial: bool,
    summary: [f32; DIMS],
}

pub struct Sections {
    /// Finished bars: start time and summary; `first_bar` is the absolute
    /// index of the front.
    bars: VecDeque<(f64, [f32; DIMS])>,
    first_bar: usize,
    acc: [f32; DIMS],
    acc_n: u32,
    bar_start: f64,
    started: bool,
    /// Absolute index of the current section's first bar.
    current_start: usize,
    done: Vec<Section>,
    next_label: u32,
    /// The bars' novelty at the last eight bar lines (absolute bar index, value).
    bar_novelty: VecDeque<(usize, f32)>,
    /// The current section's label (0 = A), once it has one.
    pub label: u32,
    /// 0..1: how much the current section sounds like an earlier one.
    pub seen_before: f32,
    /// Bars since the current section began.
    pub bars_in: u32,
    /// Bars to the next expected change (fractional, counting down through
    /// the bar), and how sure that is.
    pub bars_to_change: f32,
    pub change_confidence: f32,
    /// 0..1 through the current eight-bar phrase, counted from the section's start.
    pub phrase_phase: f32,
}

impl Default for Sections {
    fn default() -> Self {
        Self::new()
    }
}

fn mean_of<'a>(it: impl Iterator<Item = &'a [f32; DIMS]>) -> [f32; DIMS] {
    let mut m = [0.0f32; DIMS];
    let mut n = 0;
    for v in it {
        for (o, x) in m.iter_mut().zip(v) {
            *o += x;
        }
        n += 1;
    }
    m.map(|x| x / n.max(1) as f32)
}

impl Sections {
    pub fn new() -> Self {
        Self {
            bars: VecDeque::new(),
            first_bar: 0,
            acc: [0.0; DIMS],
            acc_n: 0,
            bar_start: 0.0,
            started: false,
            current_start: 0,
            done: Vec::new(),
            next_label: 1,
            bar_novelty: VecDeque::new(),
            label: 0,
            seen_before: 0.0,
            bars_in: 0,
            bars_to_change: PHRASE_BARS as f32,
            change_confidence: 0.0,
            phrase_phase: 0.0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn next_bar(&self) -> usize {
        self.first_bar + self.bars.len()
    }

    /// Once per hop: this hop's description, the time, whether a bar starts
    /// on this hop, and how far through the bar it is.
    pub fn on_hop(&mut self, d: &[f32; DIMS], time_s: f64, bar_started: bool, bar_phase: f32) {
        if bar_started {
            if self.started && self.acc_n > 0 {
                let summary = self.acc.map(|a| a / self.acc_n as f32);
                self.bars.push_back((self.bar_start, summary));
                while self.bars.len() > KEEP_BARS {
                    self.bars.pop_front();
                    self.first_bar += 1;
                }
                self.current_start = self.current_start.max(self.first_bar);
                self.bar_boundaries();
                self.recognise();
            }
            self.started = true;
            self.bar_start = time_s;
            self.acc = [0.0; DIMS];
            self.acc_n = 0;
        }
        if self.started {
            for (a, x) in self.acc.iter_mut().zip(d) {
                *a += x;
            }
            self.acc_n += 1;
        }
        self.bars_in = (self.next_bar() - self.current_start) as u32;
        let left = self.expected_bars().map_or(0.0, |l| l as f32 - self.bars_in as f32 - bar_phase);
        self.bars_to_change = if left > 0.0 { left } else { (PHRASE_BARS - self.bars_in as usize % PHRASE_BARS) as f32 - bar_phase };
        self.phrase_phase = ((self.bars_in as usize % PHRASE_BARS) as f32 + bar_phase) / PHRASE_BARS as f32;
    }

    /// How long the matched earlier section lasted, if the current one matches.
    fn expected_bars(&self) -> Option<usize> {
        self.best_match().filter(|(i, d)| *d < MATCH_DIST && !self.done[*i].partial).map(|(i, _)| self.done[i].bars)
    }

    fn current_summary(&self) -> Option<[f32; DIMS]> {
        let from = self.current_start.saturating_sub(self.first_bar);
        (self.bars.len() > from).then(|| mean_of(self.bars.iter().skip(from).map(|b| &b.1)))
    }

    fn best_match(&self) -> Option<(usize, f32)> {
        let now = self.current_summary()?;
        if self.bars_in < MIN_BARS as u32 {
            return None;
        }
        self.done.iter().enumerate().map(|(i, s)| (i, distance(&now, &s.summary))).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
    }

    fn recognise(&mut self) {
        match self.best_match() {
            Some((i, dist)) => {
                self.seen_before = (1.0 - dist / MATCH_DIST).clamp(0.0, 1.0);
                if dist < MATCH_DIST {
                    self.label = self.done[i].label;
                }
                // Sure of a change when the section is known; a phrase guess is a guess.
                self.change_confidence = if dist < MATCH_DIST { 0.4 + 0.6 * self.seen_before } else { 0.25 };
            }
            None => {
                self.seen_before = 0.0;
                self.change_confidence = 0.25;
            }
        }
    }

    /// The bars' novelty at the bar line `SIDE` bars back; a clear peak (the
    /// line before it lower, the one after it too) closes the section there.
    fn bar_boundaries(&mut self) {
        let n = self.bars.len();
        if n < 2 * SIDE {
            return;
        }
        let line = n - SIDE;
        let before = mean_of(self.bars.range(line - SIDE..line).map(|b| &b.1));
        let after = mean_of(self.bars.range(line..n).map(|b| &b.1));
        let value = distance(&before, &after);
        self.bar_novelty.push_back((self.first_bar + line, value));
        while self.bar_novelty.len() > 8 {
            self.bar_novelty.pop_front();
        }
        let k = self.bar_novelty.len();
        if k >= 3 {
            let (a, (at, b), c) = (self.bar_novelty[k - 3].1, self.bar_novelty[k - 2], self.bar_novelty[k - 1].1);
            let low = self.bar_novelty.iter().map(|v| v.1).fold(f32::MAX, f32::min);
            if b > a && b >= c && b > PEAK_OVER * low && b - low > PEAK_BY {
                self.close_at(at);
            }
        }
    }

    /// A transition was heard at `time_s`: close the current section at the
    /// bar the change began on.
    pub fn on_transition(&mut self, time_s: f64) {
        let target = time_s - LATE_S;
        let mut at = self.next_bar();
        let mut best = (target - self.bar_start).abs();
        for (k, (t, _)) in self.bars.iter().enumerate() {
            if (target - t).abs() < best {
                best = (target - t).abs();
                at = self.first_bar + k;
            }
        }
        self.close_at(at);
    }

    /// Close the current section at absolute bar `at` (unless that is too
    /// soon after its start, or a section already ended within a bar of it).
    fn close_at(&mut self, at: usize) {
        if at < self.current_start + MIN_BARS {
            return;
        }
        let from = self.current_start - self.first_bar;
        let to = (at - self.first_bar).min(self.bars.len());
        let summary = mean_of(self.bars.range(from..to).map(|b| &b.1));
        let label = self.done.iter().map(|s| (s.label, distance(&summary, &s.summary))).filter(|(_, d)| *d < MATCH_DIST).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|(l, _)| l).unwrap_or_else(|| {
            let l = if self.done.is_empty() { 0 } else { self.next_label };
            if !self.done.is_empty() {
                self.next_label += 1;
            }
            l
        });
        let partial = self.done.is_empty();
        self.done.push(Section { label, bars: at - self.current_start, partial, summary });
        self.current_start = at;
        // The new section has no label of its own until it matches one.
        self.label = self.next_label;
        self.seen_before = 0.0;
    }
}
