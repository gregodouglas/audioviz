//! Links (ADR-0006 step 5): a visual's parameter driven by a music signal,
//! set by whoever makes the scene. The raw signal goes through a gate (a
//! confidence it must have), a threshold with hysteresis, a response curve
//! and attack and release smoothing, then sets the parameter between two
//! values, its value at nothing and at full. Links live in the visual's
//! parameter block of the layer (`"links": [...]`), so they are saved with
//! the scene and kept per visual when the layer switches away and back.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use audioviz_analysis::{FeatureFrame, follow};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A signal a parameter can follow, 0..1, with how sure the analysis is.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    Sub,
    Bass,
    LowMid,
    Mid,
    Presence,
    Air,
    SubAttack,
    BassAttack,
    LowMidAttack,
    MidAttack,
    PresenceAttack,
    AirAttack,
    LowOnsets,
    MidOnsets,
    HighOnsets,
    AnyOnset,
    Beat,
    FirstBeat,
    BeatPhase,
    BarPhase,
    Loudness,
    Brightness,
    Noisiness,
    Activity,
    Width,
    Balance,
    Build,
    BassPresence,
    Contrast,
    Drop,
    Change,
    PhrasePhase,
}

impl Signal {
    /// Every signal, in the order the picker lists them, with its group.
    pub const ALL: [(Signal, &'static str); 32] = [
        (Signal::Sub, "Bands"),
        (Signal::Bass, "Bands"),
        (Signal::LowMid, "Bands"),
        (Signal::Mid, "Bands"),
        (Signal::Presence, "Bands"),
        (Signal::Air, "Bands"),
        (Signal::SubAttack, "Attacks"),
        (Signal::BassAttack, "Attacks"),
        (Signal::LowMidAttack, "Attacks"),
        (Signal::MidAttack, "Attacks"),
        (Signal::PresenceAttack, "Attacks"),
        (Signal::AirAttack, "Attacks"),
        (Signal::LowOnsets, "Onsets"),
        (Signal::MidOnsets, "Onsets"),
        (Signal::HighOnsets, "Onsets"),
        (Signal::AnyOnset, "Onsets"),
        (Signal::Beat, "Beat"),
        (Signal::FirstBeat, "Beat"),
        (Signal::BeatPhase, "Beat"),
        (Signal::BarPhase, "Beat"),
        (Signal::Loudness, "Character"),
        (Signal::Brightness, "Character"),
        (Signal::Noisiness, "Character"),
        (Signal::Activity, "Character"),
        (Signal::Width, "Character"),
        (Signal::Balance, "Character"),
        (Signal::Build, "Context"),
        (Signal::BassPresence, "Context"),
        (Signal::Contrast, "Context"),
        (Signal::Drop, "Context"),
        (Signal::Change, "Context"),
        (Signal::PhrasePhase, "Context"),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Signal::Sub => "Sub level",
            Signal::Bass => "Bass level",
            Signal::LowMid => "Low mid level",
            Signal::Mid => "Mid level",
            Signal::Presence => "Presence level",
            Signal::Air => "Air level",
            Signal::SubAttack => "Sub attack",
            Signal::BassAttack => "Bass attack",
            Signal::LowMidAttack => "Low mid attack",
            Signal::MidAttack => "Mid attack",
            Signal::PresenceAttack => "Presence attack",
            Signal::AirAttack => "Air attack",
            Signal::LowOnsets => "Low onsets",
            Signal::MidOnsets => "Mid onsets",
            Signal::HighOnsets => "High onsets",
            Signal::AnyOnset => "Any onset",
            Signal::Beat => "Beat pulse",
            Signal::FirstBeat => "First beat pulse",
            Signal::BeatPhase => "Beat phase",
            Signal::BarPhase => "Bar phase",
            Signal::Loudness => "Loudness",
            Signal::Brightness => "Brightness",
            Signal::Noisiness => "Noisiness",
            Signal::Activity => "Activity",
            Signal::Width => "Stereo width",
            Signal::Balance => "Left to right",
            Signal::Build => "Build",
            Signal::BassPresence => "Bass presence",
            Signal::Contrast => "Contrast",
            Signal::Drop => "Drop pulse",
            Signal::Change => "Change pulse",
            Signal::PhrasePhase => "Phrase phase",
        }
    }

    /// Whether the signal is an inference with a confidence worth gating on.
    pub fn has_confidence(self) -> bool {
        matches!(self, Signal::Beat | Signal::FirstBeat | Signal::BeatPhase | Signal::BarPhase | Signal::Drop | Signal::PhrasePhase)
    }

    /// The signal now, 0..1, and how sure the analysis is of it.
    pub fn value(self, f: &FeatureFrame) -> (f32, f32) {
        let b = |k: usize| f.broad_level[k];
        let a = |k: usize| f.broad_attack[k];
        let v = match self {
            Signal::Sub => b(0),
            Signal::Bass => b(1),
            Signal::LowMid => b(2),
            Signal::Mid => b(3),
            Signal::Presence => b(4),
            Signal::Air => b(5),
            Signal::SubAttack => a(0),
            Signal::BassAttack => a(1),
            Signal::LowMidAttack => a(2),
            Signal::MidAttack => a(3),
            Signal::PresenceAttack => a(4),
            Signal::AirAttack => a(5),
            Signal::LowOnsets => f.onset_pulse[0],
            Signal::MidOnsets => f.onset_pulse[1],
            Signal::HighOnsets => f.onset_pulse[2],
            Signal::AnyOnset => f.onset_pulse[3],
            Signal::Beat => f.beat_pulse,
            Signal::FirstBeat => if f.beat_in_bar == 0 { f.beat_pulse } else { 0.0 },
            Signal::BeatPhase => f.beat_phase,
            Signal::BarPhase => f.bar_phase,
            Signal::Loudness => ((f.lufs_m + 40.0) / 35.0).clamp(0.0, 1.0),
            Signal::Brightness => f.brightness,
            Signal::Noisiness => f.noisiness,
            Signal::Activity => f.activity,
            Signal::Width => f.stereo_width,
            Signal::Balance => 0.5 + 0.5 * f.balance,
            Signal::Build => f.build,
            Signal::BassPresence => f.bass_presence,
            Signal::Contrast => f.contrast,
            Signal::Drop => f.drop_pulse,
            Signal::Change => f.transition_pulse,
            Signal::PhrasePhase => f.phrase_phase,
        };
        let sure = match self {
            Signal::Beat | Signal::BeatPhase => f.beat_confidence,
            Signal::FirstBeat | Signal::BarPhase => f.beat_confidence.min(f.bar_confidence),
            Signal::Drop => f.drop_confidence,
            Signal::PhrasePhase => f.change_confidence,
            _ => 1.0,
        };
        (v.clamp(0.0, 1.0), sure)
    }
}

fn one() -> f32 {
    1.0
}

fn release_default() -> f32 {
    150.0
}

/// One parameter following one signal.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Link {
    pub param: String,
    pub signal: Signal,
    /// The parameter's value when the signal is at nothing, and at full.
    pub low: f32,
    pub high: f32,
    /// The response: 1 straight, under 1 rises early (sensitive), over 1 late.
    #[serde(default = "one")]
    pub curve: f32,
    /// Smoothing, ms: how fast it follows a rise and a fall.
    #[serde(default)]
    pub attack_ms: f32,
    #[serde(default = "release_default")]
    pub release_ms: f32,
    /// Under the threshold the signal counts as nothing; once over, it stays
    /// on until it falls under the threshold less the hysteresis.
    #[serde(default)]
    pub threshold: f32,
    #[serde(default)]
    pub hysteresis: f32,
    /// Under this confidence (beat, bar, drop, phrase signals) the link rests.
    #[serde(default)]
    pub gate: f32,
}

impl Link {
    pub fn new(param: &str, low: f32, high: f32) -> Self {
        Self { param: param.into(), signal: Signal::Bass, low, high, curve: 1.0, attack_ms: 0.0, release_ms: release_default(), threshold: 0.0, hysteresis: 0.0, gate: 0.0 }
    }
}

/// A link's running state.
#[derive(Default, Clone, Copy)]
pub struct LinkRun {
    smoothed: f32,
    on: bool,
}

/// What one frame of a link did, for the director view.
#[derive(Clone, Copy, Debug, Default)]
pub struct Step {
    /// The signal as it came, after the gate and threshold, and after the
    /// curve and the smoothing (each 0..1), and the parameter's value.
    pub raw: f32,
    pub passed: f32,
    pub shaped: f32,
    pub value: f32,
}

impl Link {
    /// One frame. `test`, when given, stands in for the signal (a manual input).
    pub fn step(&self, f: &FeatureFrame, test: Option<f32>, run: &mut LinkRun, dt: f32) -> Step {
        let (raw, sure) = match test {
            Some(t) => (t.clamp(0.0, 1.0), 1.0),
            None => self.signal.value(f),
        };
        let gated = if sure < self.gate { 0.0 } else { raw };
        run.on = if self.threshold <= 0.0 { true } else if run.on { gated > self.threshold - self.hysteresis } else { gated > self.threshold };
        let passed = if run.on { gated } else { 0.0 };
        let curved = passed.powf(self.curve.max(0.05));
        run.smoothed = follow(run.smoothed, curved, dt, self.attack_ms / 1000.0, self.release_ms / 1000.0);
        Step { raw, passed, shaped: run.smoothed, value: self.low + (self.high - self.low) * run.smoothed }
    }
}

/// The link in focus (its editor open last), and what it did frame by
/// frame, for the director view's link lane.
pub struct Probe {
    pub owner: u64,
    pub param: String,
    /// "Glow ← Bass attack".
    pub label: String,
    pub low: f32,
    pub high: f32,
    /// (seconds of audio analysed, the step).
    pub samples: VecDeque<(f64, Step)>,
}

/// About five minutes at 100 frames a second.
const PROBE_KEEP: usize = 30_000;

pub static PROBE: Mutex<Option<Probe>> = Mutex::new(None);

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

/// A fresh id for whatever runs links (one per layer's visual).
pub fn new_owner() -> u64 {
    NEXT_OWNER.fetch_add(1, Ordering::Relaxed)
}

/// Puts a link in focus (keeping its samples if it already was).
pub fn focus(owner: u64, link: &Link, label: &str) {
    if let Ok(mut p) = PROBE.lock() {
        let same = p.as_ref().is_some_and(|p| p.owner == owner && p.param == link.param);
        if same {
            if let Some(p) = p.as_mut() {
                (p.label, p.low, p.high) = (format!("{label} \u{2190} {}", link.signal.label()), link.low, link.high);
            }
        } else {
            *p = Some(Probe { owner, param: link.param.clone(), label: format!("{label} \u{2190} {}", link.signal.label()), low: link.low, high: link.high, samples: VecDeque::new() });
        }
    }
}

/// Records a frame of a link, if it is the one in focus.
pub fn record(owner: u64, param: &str, time_s: f64, step: Step) {
    if let Ok(mut p) = PROBE.lock() {
        if let Some(p) = p.as_mut().filter(|p| p.owner == owner && p.param == param) {
            if p.samples.back().is_some_and(|(t, _)| time_s < *t) {
                p.samples.clear();
            }
            p.samples.push_back((time_s, step));
            while p.samples.len() > PROBE_KEEP {
                p.samples.pop_front();
            }
        }
    }
}

/// The links stored in a visual's parameter block.
pub fn read(block: Option<&Value>) -> Vec<Link> {
    block.and_then(|b| b.get("links")).and_then(|l| serde_json::from_value(l.clone()).ok()).unwrap_or_default()
}

/// Writes the links into a visual's parameter block (removing the key when there are none).
pub fn write(block: &mut serde_json::Map<String, Value>, links: &[Link]) {
    if links.is_empty() {
        block.remove("links");
    } else if let Ok(v) = serde_json::to_value(links) {
        block.insert("links".into(), v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(bass: f32) -> FeatureFrame {
        let mut f = FeatureFrame::default();
        f.broad_level[1] = bass;
        f
    }

    #[test]
    fn a_link_maps_its_signal_between_low_and_high() {
        let l = Link { release_ms: 0.0, ..Link::new("glow", 0.2, 1.0) };
        let mut run = LinkRun::default();
        assert!((l.step(&frame(0.0), None, &mut run, 0.016).value - 0.2).abs() < 1e-5);
        assert!((l.step(&frame(1.0), None, &mut run, 0.016).value - 1.0).abs() < 1e-5);
        assert!((l.step(&frame(0.5), None, &mut run, 0.016).value - 0.6).abs() < 1e-5);
    }

    #[test]
    fn the_threshold_holds_with_hysteresis() {
        let l = Link { release_ms: 0.0, threshold: 0.5, hysteresis: 0.2, ..Link::new("x", 0.0, 1.0) };
        let mut run = LinkRun::default();
        assert_eq!(l.step(&frame(0.45), None, &mut run, 0.016).passed, 0.0);
        assert!(l.step(&frame(0.6), None, &mut run, 0.016).passed > 0.0);
        // Under the threshold but over threshold - hysteresis: still on.
        assert!(l.step(&frame(0.4), None, &mut run, 0.016).passed > 0.0);
        assert_eq!(l.step(&frame(0.25), None, &mut run, 0.016).passed, 0.0);
    }

    #[test]
    fn links_round_trip_through_a_parameter_block() {
        let mut block = serde_json::Map::new();
        block.insert("glow".into(), Value::from(0.5));
        let links = vec![Link { signal: Signal::BassAttack, ..Link::new("glow", 0.1, 0.9) }];
        write(&mut block, &links);
        assert_eq!(read(Some(&Value::Object(block.clone()))), links);
        write(&mut block, &[]);
        assert!(!block.contains_key("links"));
    }
}
