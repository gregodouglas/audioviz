//! Where the bar starts (ADR-0006 step 3): which of four beats is the first,
//! assuming four beats a bar. At each beat the evidence for "a bar starts
//! here" is gathered: the low onsets (a kick) and the bass's attack over its
//! first quarter, and the chords changing (the unsmoothed pitch-class
//! profile over its first half against the one over the second half of the
//! last beat). Each of the four places in the bar keeps a score that fades
//! over about eight bars; the best is the first beat, taken over only when
//! another beats it clearly, and the confidence is how far it leads. Kicks on
//! 1 and 3 alone cannot tell those two apart; the chords usually can.

use crate::follow;

/// Each beat's score keeps this much of itself per beat (about eight bars).
const KEEP_PER_BEAT: f32 = 0.97;
/// A place must beat the current first beat by this much to take over.
const TAKE_OVER: f32 = 1.2;
/// Weights of the evidence.
const W_LOW: f32 = 1.0;
const W_CHORDS: f32 = 3.0;
const W_BASS: f32 = 0.5;

pub struct BarTracker {
    scores: [f32; 4],
    /// The place in the bar (beat count mod 4) taken as the first beat.
    first: u32,
    last_count: u32,
    evidence: f32,
    gathered: bool,
    chroma_before: [f32; 12],
    chroma_after: [f32; 12],
    /// This beat's second half, becoming `chroma_before` at the next.
    chroma_late: [f32; 12],
    /// 0..1 through the bar of four beats.
    pub bar_phase: f32,
    /// 0..3: which beat of the bar this is (0 = the first).
    pub beat_in_bar: u32,
    /// 0..1: how sure the first beat is.
    pub confidence: f32,
}

fn similarity(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na <= 1e-6 || nb <= 1e-6 { 1.0 } else { dot / (na * nb) }
}

impl Default for BarTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl BarTracker {
    pub fn new() -> Self {
        Self {
            scores: [0.0; 4],
            first: 0,
            last_count: 0,
            evidence: 0.0,
            gathered: true,
            chroma_before: [0.0; 12],
            chroma_after: [0.0; 12],
            chroma_late: [0.0; 12],
            bar_phase: 0.0,
            beat_in_bar: 0,
            confidence: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.scores = [0.0; 4];
        self.confidence = 0.0;
    }

    /// Once per hop. `low` is the low onset strength (0..1.5), `bass_attack`
    /// the bass's attack, `chroma` this hop's pitch-class profile, unsmoothed.
    #[allow(clippy::too_many_arguments)]
    pub fn process(&mut self, beat_count: u32, beat_phase: f32, beat_confidence: f32, low: f32, bass_attack: f32, chroma: &[f32; 12], dt: f32, silent: bool) {
        if beat_count != self.last_count {
            self.last_count = beat_count;
            for s in self.scores.iter_mut() {
                *s *= KEEP_PER_BEAT;
            }
            self.evidence = 0.0;
            self.gathered = false;
            self.chroma_after = [0.0; 12];
            self.chroma_before = std::mem::take(&mut self.chroma_late);
        }
        if !silent {
            if beat_phase < 0.25 {
                // The beat's first quarter: what arrived with it.
                self.evidence += (W_LOW * low + W_BASS * bass_attack) * dt;
            }
            let half = if beat_phase < 0.5 { &mut self.chroma_after } else { &mut self.chroma_late };
            for (a, c) in half.iter_mut().zip(chroma) {
                *a += c * dt;
            }
            if !self.gathered && beat_phase >= 0.5 {
                self.gathered = true;
                let change = (1.0 - similarity(&self.chroma_before, &self.chroma_after)).max(0.0);
                let slot = (beat_count % 4) as usize;
                self.scores[slot] += self.evidence * 10.0 + W_CHORDS * change;
                // Take over only on a clear lead.
                let best = (0..4).max_by(|a, b| self.scores[*a].partial_cmp(&self.scores[*b]).unwrap()).unwrap_or(0);
                if best as u32 != self.first && self.scores[best] > TAKE_OVER * self.scores[self.first as usize] {
                    self.first = best as u32;
                }
                let mut sorted = self.scores;
                sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
                let lead = if sorted[0] > 1e-6 { ((sorted[0] - sorted[1]) / sorted[0]).clamp(0.0, 1.0) } else { 0.0 };
                self.confidence = follow(self.confidence, (lead * 2.0).min(1.0) * beat_confidence, dt.max(0.25), 1.0, 1.0);
            }
        }
        self.beat_in_bar = (beat_count + 4 - self.first) % 4;
        self.bar_phase = (self.beat_in_bar as f32 + beat_phase) / 4.0;
    }
}
