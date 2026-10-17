# Phase 2 — analysis core

Status: **landed 2026-10-17.** The analyzer now produces the full v1 feature set from `docs/research/03-audio-analysis.md`, capture and analysis run on their own engine thread, and the panel shows every feature live. Validation so far is synthetic tests plus two 30 s Spotify recordings; the "20 varied tracks" bar is open and has a tool (below).

## What the `FeatureFrame` carries

| Group | Fields | Source |
|---|---|---|
| Level | `rms`, `rms_slow`, `peak`, `lufs_m` (400 ms), `lufs_s` (3 s), `quiet`, `silent` | Time domain; K-weighting per ITU-R BS.1770 at 48 kHz |
| Bands | `bands[32]` log-spaced 30 Hz–16 kHz, per-band auto-gain, attack 10 ms / release 200 ms | Bands under 150 Hz from the 8192-point FFT, the rest from 2048 |
| MilkDrop baseline | `bass/mid/treb` (1.0 = running average) and `_att` smoothed versions | 2048-point FFT |
| Shape | `centroid_hz`, `stereo_width` (mid/side) | 2048-point FFT, time domain |
| Onsets | `onset_strength/hit/pulse[4]` for low, mid, high, all | Low from 2048 (bass needs resolution), mid/high from 512 (transients need time), "all" from 36 log bands of the 2048 spectrum. SuperFlux-style flux, median + 4·MAD threshold, causal peak pick (one hop late), 40 ms refractory |
| Beat clock | `bpm`, `beat_confidence`, `beat_phase` (0..1, predictive), `beat_count`, `beat_pulse` | Autocorrelation of the "all" onset envelope every 0.5 s over 70–170 BPM with harmonic reinforcement and a mild 120 BPM prior; lock needs three agreeing estimates; octave flips are ignored while locked; phase aligned by comb scoring and slewed, never snapped |
| Harmony | `chroma[12]`, `chroma_slow[12]`, `key_class` | 8192-point FFT, 55 Hz–5 kHz, high bins tapered |
| Structure | `energy_trend` (-1..1, 1 s vs 16 s energy) | Time domain |

Everything is sample-clocked at the hop (512 samples, 93.75 Hz), so results do not depend on frame rate.

## Engine thread

`crates/app/src/engine.rs`: owns the capture handle, the delay line (the panel's "audio delay"), and the analyzer. Publishes the latest frame behind a mutex and streams raw (L, R) points to the renderer over a bounded channel (dropped if the window is stalled rather than blocking capture). This is the seam where a feature-frame WebSocket tap would attach if the browser sketchbook is ever wanted (ADR-0001).

## Validation loop (use this for the 20-track pass)

```
cargo build --workspace --all-targets
./target/debug/examples/record.exe 30 target/recording.wav      # records whatever Spotify is playing
./target/debug/examples/analyze_wav.exe target/recording.wav    # one line per second + summary
```

The summary line reports the median BPM, how many seconds agreed with it within 3%, and mean confidence. A good result is most seconds agreeing after the lock-in period with confidence above ~0.5.

Results so far (2026-10-17, tracks unknown, Spotify normalization on):

| Sample | Median BPM | Seconds within 3% | Mean conf | Notes |
|---|---|---|---|---|
| recording.wav (before low-group fix) | 140.8 | 13/24 | 0.22 | Low-group onsets 12–16/s: 512-pt FFT gave bass two bins |
| recording.wav (after) | 140.0 | 17/24 | 0.54 | Locked from t≈13 s; conf 0.6–0.89 thereafter |
| recording2.wav | 130.4 | 16/24 | 0.40 | Lock from t≈12 s; brief 2 s excursion to 86.6 (a 3:2 ratio, not an octave) and to 160 in the last second |

Synthetic tests (`cargo test -p audioviz-analysis`): 1 kHz sine lands in the right band; silence is silent; A-major chord puts A, C#, E on top of the chroma; a 120 BPM click track over a noise bed yields 24–48 onsets in 16 s and a tempo within 3 BPM; the beat tracker locks a pure 128 BPM pulse within 2 BPM and reads near-zero phase at the clicks; white-noise envelopes stay under 0.35 confidence; K-weighted loudness matches the BS.1770 reference level.

## Known tuning items

- **Lock-in time** is ~10–13 s on real music (needs 4 s of history plus three agreeing half-second estimates). Could start estimating at 3 s and accept two agreeing estimates at the cost of more false early locks.
- **Octave ambiguity**: a 70 BPM half-time feel and a 140 BPM track score similarly; the prior leans toward 120. Visuals should use `beat_phase` for motion and avoid anything that looks wrong at the double or half tempo.
- **Tempo range** is fixed at 70–170 BPM.
- **Stereo width** reads 0.9+ on heavily widened mixes; it is a ratio, not a perceptual measure.
- The egui panel renders on an sRGB swapchain, which egui warns about; its colors are slightly off. Cosmetic.
