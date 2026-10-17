# 03 — Real-time audio analysis (the "music understanding" layer)

> Research brief, 2026-10-16. Produced by a Sonnet research subagent that ran only a few web searches; most of this is well-established DSP literature from model knowledge. Items marked **[unverified]** need a check before committing. Library maintenance status in particular is from memory.

Constraint: input is a live PCM stream (loopback), no lookahead beyond ~50-100 ms, no access to the full track.

## 1. Fundamentals

**FFT size and windowing at 44.1/48 kHz.**
- Frequency resolution is fs/N. N=2048 at 48 kHz gives 23 Hz bins and a 43 ms window: too coarse below ~200 Hz, where a semitone is only 5-10 Hz wide.
- N=4096 gives 11.7 Hz bins and 85 ms: fine for bass, smears transients.
- Hann for general use; Blackman-Harris for less leakage in tonal/chroma work.
- Hop size sets visual smoothness. 512 samples at 48 kHz is ~10.7 ms, fine for 60-144 fps. Overlap does not buy resolution, only faster updates.
- Because 85 ms sits at the edge of the lookahead budget, use **multi-resolution analysis**: a long window (4096-8192) for bass and chroma, medium (2048) for mids, short (512-1024) for transients and onsets. Parallel STFTs are simpler in Rust than a constant-Q transform and good enough.

**Banding.**
- Linear bins mislead the eye: bass gets 2-3 bins, treble hundreds.
- Aggregate into log-spaced bands (1/3 octave, ~24-32 bands, 30 Hz to 16 kHz) or Bark/mel (~24-40).
- Sum power, not magnitude; then dB or power-law compression (x^0.3-0.5) so quiet detail shows. Apply equal-loudness tilt (+3 dB/oct) or the display is bass-dominated.

**Per-band smoothing (envelope follower).**
- Asymmetric one-pole: fast attack, slow release. Roughly attack 5-20 ms, release 150-400 ms for bars; 80-150 ms release for kick-like bands, 300-600 ms for pads.
- Per frame: `y += (x - y) * (1 - exp(-dt/tau))`, tau chosen by whether x > y. Compute from dt so it is frame-rate independent.
- Peak-hold with slow fall for the "gravity" look.

**Loudness normalization and auto-gain.**
- ITU-R BS.1770 K-weighting (high-shelf + high-pass) with 400 ms momentary and 3 s short-term windows is the standard.
- For visuals a running RMS/peak envelope with a long time constant (5-15 s) and slow-attack AGC usually suffices. Normalize **per band** by a decayed 95th-percentile estimate, or bass dominates.
- Noise-floor gate so silence between tracks does not blow up the gain. Handle track changes explicitly.
- Spotify's own normalization (about -14 LUFS when enabled) acts before loopback, which helps. Whether system volume scales loopback depends on capture mode [unverified; see 01].

**Spectral shape descriptors.** Centroid (brightness: hue, color temperature), flatness (noisiness: smooth vs grainy), rolloff 85-95% (bandwidth, "air"). All noisy frame to frame; smooth at 200 ms to 1 s and treat as slow parameters. Compute on the normalized, log-compressed spectrum; gate below the noise floor.

**MilkDrop baseline.** `bass`, `mid`, `treb` normalized so 1.0 means "average" volume, plus `bass_att`/`mid_att`/`treb_att` heavily smoothed versions, from 512-sample spectra in thirds, self-normalized against a long-term average. Presets use the fast value for pulses and the `_att` value for steady motion. projectM is the reference implementation. **Minimum bar: 3 bands × (fast + slow), relative to running average.**

## 2. Onsets and beats in real time

**Spectral flux onset detection.** Half-wave-rectified difference of log-compressed magnitude spectra; SuperFlux (Böck & Widmer 2013) adds a max-filter over frequency to suppress vibrato false positives. Adaptive threshold: local median plus k × MAD over the past 0.5-1 s, minimum inter-onset interval 30-50 ms. Run per band group (low/mid/high) so kick and hi-hat trigger separately. Latency one or two windows (10-20 ms); causal peak-picking needs a frame of delay, or fire speculatively. Cost well under 1% of a core.

**Online beat trackers.**

| Tracker | Notes |
|---|---|
| BTrack (Stark, Davies, Plumbley 2009; C++, GPL) | Online, causal, cumulative score + autocorrelation tempo. ~1 hop latency, low CPU. Good on steady 4/4 (EDM, pop); weak on rubato, ambient, classical. No downbeats. |
| aubio tempo (C, GPL) | Onset-driven, causal, very cheap. Fair on steady music. Sporadic maintenance; aubio-rs bindings exist [unverified]. |
| madmom online (RNN + DBN; Böck) | Best accuracy of the classic causal set. CPU-feasible in Python. Project largely unmaintained. Online downbeat variant exists. |
| BeatNet / BeatNet+ (Heydari et al.) | Causal CRNN + particle filter; beats, downbeats, tempo jointly. BeatNet+ (TISMIR Dec 2024) reports better F1 with "competitive latency" ([abstract](https://doaj.org/article/393c7ce2039d4e8dad5843c4c498a6d7)). pip-installable. Worth a Python prototype. |
| Real-time PLP (Meier, Chiu, Müller, TISMIR 2024) | Causal predominant-local-pulse; reports zero latency plus lookahead to *predict* upcoming beats ([paper](https://transactions.ismir.net/articles/189), [project page](https://audiolabs-erlangen.com/resources/MIR/2024-TISMIR-RealTimePLP)). Best fit for "predict, don't react". |
| Ellis DP (librosa beat_track) | Offline. Only usable for rolling-buffer tempo estimation. |

All degrade on ambient, classical and rubato. Expose confidence and let visuals degrade gracefully.

**Beat clock.** Do not drive visuals from raw onsets (jittery). Run a PLL or the tracker's phase to produce a continuous 0..1 phase advanced at the estimated BPM, gently corrected toward detected beats. Expose phase, beat count, bar position, confidence. Because it free-runs it **predicts** the next beat and so cancels output and render latency. When confidence drops, cross-fade to onset/energy-driven motion rather than snapping.

**Tempo stability.** Fold candidates into 70-170 BPM, check octave errors, require 4-8 consistent estimates before relocking, hysteresis on lock/unlock, confidence from autocorrelation peak sharpness.

**Downbeats.** madmom online DBN, BeatNet, or a heuristic (count beats, pick among 4 phase hypotheses by low-band energy at beat positions). Online downbeat accuracy is meaningfully worse than beat accuracy; use bar position to schedule bigger events, never to break if wrong.

## 3. Higher-level descriptors

- **Chroma and key.** Fold FFT peaks into 12 pitch classes from a 4096-8192 window with tuning correction; smooth 2-4 s. Key via Krumhansl-Schmuckler or Temperley profiles on long-term chroma; modest accuracy, slow parameter only. Mapping key to hue via the circle of fifths gives coherent color shifts; mapping chroma directly to a 12-spoke color wheel is more responsive and needs no key detection.
- **HPSS.** Fitzgerald median-filter HPSS made causal (time-median looks back only, 8-17 frame buffer). Moderate cost. Simpler proxy for "how percussive right now": onset-strength to sustained-energy ratio.
- **Section changes (drops, builds, quiet).** Long-window features: loudness envelope on 1, 4 and 16 s windows, centroid trend, low-band energy ratio. Detect via a Foote novelty curve on a rolling self-similarity matrix or thresholded short-vs-long loudness differences. Build = rising centroid + energy slope + onset density. Drop = sudden low-band jump after reduced bass. Necessarily lags ~1 s; drives scene-level decisions. No published real-time benchmark; tune by ear.
- **Stereo width / mid-side.** Per band, mid=(L+R)/2, side=(L-R)/2, width = side/mid energy, plus correlation and pan estimate. Very cheap, robust, perfect for horizontal spread, depth, fog. Spotify is stereo only.
- **ML live.** CLAP (LAION) and MERT embeddings are feasible on a 2070 Super on a 5-10 s window every few seconds (tens of ms GPU, FP16) [unverified timing]. Use for zero-shot mood tags ("calm ambient", "aggressive electronic") or nearest-neighbor scene lookup. Essentia's TensorFlow models (MusiCNN, Discogs-EffNet, mood heads) in ONNX run on CPU.
- **Real-time source separation.** Stock Demucs / HT-Demucs are offline chunked; not usable under 100 ms. HS-TasNet (L-Acoustics, ICASSP 2024) is causal: SDR 4.65 at 23 ms latency ([arXiv 2402.17701](https://arxiv.org/pdf/2402.17701)). GPU Audio announced an HS-TasNet-based real-time module Feb 2026 ([Sonic State](https://www.sonicstate.com/news/2026/02/02/real-time-source-separation-on-gpu/)), vendor claims. No confirmed open pretrained causal weights. **Verdict: v3 research item.** For visuals, a drum-vs-rest split via HPSS + band-split onsets approximates acceptably.
- **What Spotify's deprecated Audio Analysis gave** (the target to approximate live): bars, beats, tatums with start/duration/confidence; sections with loudness, tempo, key, mode, time signature; segments (note-level) with loudness, 12-dim chroma, 12-dim timbre; track-level tempo, key, mode, loudness. Live substitutes: onsets for segments, beat clock for beats/tatums, downbeat heuristic for bars, novelty heuristics for sections, chroma for pitches, band-energy shape or MFCC for timbre.

## 4. Libraries

**Rust** (check crates.io dates): `rustfft`/`realfft` stable and SIMD-accelerated, use them. `dasp` for sample conversion. `fundsp` for biquads (K-weighting) and envelope followers. `aubio-rs` needs a C build step, low activity. No mature Essentia binding. `ort` (ONNX Runtime with CUDA/DirectML) is the practical path for CLAP/MERT/MusiCNN; `candle` and `tract` are alternatives. Capture: `wasapi` crate on Windows (see 01).

**C/C++**: aubio (mature, slow-moving), Essentia (streaming mode, actively maintained, **AGPL** unless licensed), BTrack (small, GPL), Gist, libxtract.

**Python prototyping**: librosa is offline (emulate with rolling buffers), madmom is stale with Python/NumPy issues, BeatNet is pip-installable, torchaudio for GPU features. Python 3.12+ compatibility is the usual failure point; we have 3.14, so expect friction.

## 5. Latency and mapping

**Perceptual thresholds.** Audiovisual synchrony tolerance is asymmetric: people tolerate video leading audio more than the reverse. Dixon & Spitz (1980): ~75 ms audio-leading to ~188 ms audio-lagging for a hammer; wider for speech. van Eijk et al. (2008): ~111 ms (static) and ~79 ms (moving) for audio-leading detection. Commonly cited range about -50 ms to +150 ms. ([PMC4744562](https://pmc.ncbi.nlm.nih.gov/articles/PMC4744562), [Frontiers 2015](https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2015.00736/full), [PMC4451240](https://pmc.ncbi.nlm.nih.gov/articles/PMC4451240)). Musical transients are on the strict end. **Aim for the visual event to land 0 to ~40 ms after the audible sound; never let visuals precede audio.**

**Compensation.** Loopback is captured before the output buffer, so visuals naturally lead the ear by tens of ms wired, 100-250 ms Bluetooth. Add a user-adjustable delay ring buffer (0-300 ms) ahead of analysis, a calibration UI (flash + click), saved per output device. A predicted beat clock cancels tens of ms of analysis latency.

**Mapping principles** (practitioner consensus, no single authoritative paper):
- Map energy to velocity, acceleration or intensity of change, not absolute position. Continuous motion, no jitter.
- Fast attack, slow release; hysteresis or refractory periods on triggers.
- Separate **slow parameters** (palette, scene, camera, density; seconds to minutes; driven by key, loudness trend, sections, embeddings) from **fast parameters** (pulse, flash, scale; tens to hundreds of ms; driven by onsets, bands, beat phase).
- Limit flash rates: no large-area luminance flashing above ~3/s (WCAG 2.3.1, photosensitive epilepsy). Design in from day one.
- Determinism plus controlled randomness: seeded noise with drifting offset, weighted-random scene choice, anti-repetition memory, slowly modulated mapping coefficients, so an hour of listening does not get monotonous.
- Keep a few constant references (one parameter mapped directly to bass) so the viewer always sees the connection.

## Recommended default parameters (starting points, tune by eye and ear)

| Item | Default | Rationale |
|---|---|---|
| Sample rate | 48 kHz (resample 44.1 if needed) | Matches WASAPI shared-mode mix |
| Long STFT | N=8192 (or 4096), hop 512, Hann or Blackman-Harris | Bass and chroma resolution (~6 or 12 Hz bins) |
| Mid STFT | N=2048, hop 512, Hann | Vocals and mids |
| Short STFT | N=1024 (or 512), hop 256-512, Hann | Onset timing (~20 ms) |
| Bands | 1/3-octave, 24-32 bands, 30 Hz-16 kHz, power sum then x^0.4 or dB | Perceptual spacing, +3 dB/oct tilt |
| Attack / release | 10 ms / 200 ms (bars); kick band 5 ms / 100 ms | Punch without flicker |
| Slow average ("att") | tau 1-2 s | MilkDrop-like steady motion |
| Auto-gain | per-band decayed 95th percentile, tau 5-10 s, gate -60 dBFS | Quiet and loud tracks look alike |
| Loudness | K-weighted momentary (400 ms), short-term (3 s) | Scene-level intensity |
| Onset | SuperFlux-style flux, median + k·MAD over 0.5-1 s, min gap 40 ms, 3 band groups | Standard and cheap |
| Beat clock | 70-170 BPM, small PLL gain, confidence-gated | Predictable phase |
| Descriptors | centroid, rolloff 90%, flatness smoothed 0.3-1 s | Slow parameters |
| Visual delay | user calibration, default 0-50 ms wired | Output latency compensation |

## Recommended pipeline

**v1 (classical DSP only)**
1. Loopback into a ring buffer; downmix to mid/side, keep L/R energy.
2. Three STFTs via `realfft`; log bands; K-weighted loudness; per-band AGC.
3. Attack/release smoothing; MilkDrop-style bass/mid/treb fast + att; centroid, flatness, rolloff, stereo width.
4. Per-band-group onsets; autocorrelation tempo estimate; PLL beat clock with confidence.
5. Chroma 12-vector for a hue wheel; long-window section heuristics (build, drop, quiet).
6. Calibration delay, flash-rate limiter, seeded long-term variation.

**v2 (real beat tracker + small ML)**
1. Evaluate BTrack, real-time PLP, BeatNet+ against the PLL; keep PLL as low-confidence fallback.
2. Downbeat via BeatNet or heuristic.
3. CLAP or Essentia mood/genre via `ort` (CUDA) on a rolling 5-10 s window every few seconds for scene and palette selection, with hysteresis and anti-repetition.
4. Causal HPSS or drum-vs-rest if onset detection proves inadequate.
5. Defer real-time stem separation until open causal weights exist and latency is measured on the 2070 Super.

**Unproven**: all parameter defaults; tracker robustness by genre (no head-to-head numbers found); any separation model's real-time viability; CLAP/MERT latency; crate maintenance status in 2026.
