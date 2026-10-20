# ADR-0006 — The audio engine: dependable signals, measured on real music

- **Status:** Accepted 2026-10-19 (Greg: the next set of work is the audio engine, "enhancing and refining the audio engine for better performance on analysis of the audio so we can get the best performance from visualizations, enhancing the band performance"). He chose to start with the hit fix and replay, an offline timeline sheet before a live inspector, and EDM plus rock/pop as the tuning music. Then (2026-10-19) the focus, in his words: "capture and live history and best predictions and signals we can get from a live stream like we are doing with Spotify". And: "part of this is in say a director mode where we're getting live spectrographs or other audio visual aids to help create sense (not what the consumer sees but what helps the visual creator)", so a first live inspector moved up into step 1. Step 0 done in 0.4.30, step 1a (capture) in 0.4.31, the history and the first director view (1b, 1d) with the WAV test bench in 0.4.32.
- **Context:** Phase 2 (`docs/phase2-analysis.md`) built a full v1 feature set: 32 log bands, bass/mid/treble, loudness, centroid, stereo width, onsets in four groups, a predictive beat clock, chroma, an energy trend. What holds the visuals back now is quality and plumbing, not missing FFTs:
  - Onset hits last one hop (10.7 ms) and reach the render thread through a "latest frame" snapshot, so at 60 fps about a third never reach a visual and at 144 fps some show twice.
  - Bands under 150 Hz come from the 8192-point FFT (a 171 ms window): the kick, the band that most wants to punch, arrives late and smeared.
  - Every band is relative only (auto-gained against its own 8 s peak): a quiet passage is lifted toward full scale, and there are no per-band attacks.
  - The director's "drop" cue is an energy fall after a rise, which is a breakdown.
  - The tempo locks in 10 to 13 s and is not reset when the track changes, although the metadata says when it does.
  - Capture health is not tracked: WASAPI's discontinuity and silent flags are ignored, no sample clock reaches the features, and endpoint loopback does not follow a change of default device.
  - Capture adds delay: it reads one packet per wake-up (a second waiting packet sits until the next event, up to 60 ms), and holds back the part of a packet that does not fill a 256-frame chunk. While nothing plays it sends 256 frames of silence every 60 ms, so the analysis clock runs at a twelfth of real time.
  - Nothing is remembered beyond a few seconds: the tempo, the loudest moments, the parts that repeat are worked out again and again, and forgotten at a track change.
  - Nothing can be tuned repeatably against real music: neither the app nor the lab can play a recording, and there is no view that shows why something fired.
- **Reads:** `docs/research/03-audio-analysis.md`, `docs/phase2-analysis.md`, ADR-0004 (living motion: entrain, do not trigger).

## Decisions

### 1. Analysis is a shared source of musical signals

Capture, then timestamped audio, then analysis, then signals and events, then the visuals. Analysis stays on the engine thread, sample-clocked, independent of wgpu and the frame rate: a slow frame never changes what is detected. The lab, the director and the visuals all read the same signals.

### 2. Live first: causal, low latency

The product listens live to Spotify, so every signal is worked out from what has already been heard: no look-ahead, no analysis of the whole song. Each signal's delay is measured and kept small, and anything that needs the future (where the next downbeat falls, whether a build will drop) is a prediction with a confidence. Mapping a whole song from a file (a song map, scenes timed to it, rendering videos) is later work with its own ADR (Greg, 2026-10-19: "future work will include wav and files for true mapping of songs, but for now we are live"); it can reuse these signals, but nothing here waits for it.

### 3. Continuous values and events are delivered differently

Continuous values (levels, bands, phase, trends) are snapshots and may be read at any rate. Events (onsets, beats, later transitions and drops) carry a running count per kind, so a reader turns "the count moved" into a hit: a frame never misses one or sees one twice, whatever the frame rate.

### 4. Measured on real music, repeatably

Tuning clips of about 30 s are recorded from Spotify with `record.rs` into `tmp/` (never committed): electronic/EDM and rock/pop to start. The app and the lab can play a WAV in place of capture, and `analyze_wav` renders a deterministic timeline sheet (waveform, log-frequency dB spectrogram, band curves, onset markers with their thresholds, beat grid, confidences). The WAV source is a test bench, not a feature: it feeds the same live, causal pipeline at real speed (or faster, in `analyze_wav`), so what it shows is what the live app does. Every analysis change is judged on the same clips before and after.

### 5. The engine remembers what it has heard

A live stream has no future, but it has a past. The engine keeps a history of the stream: a compact record of every hop's signals for the current track and the session, and the last couple of minutes of audio. It is split by track (the metadata says when one starts). The history is what better predictions are built from: a track's settled tempo and bar grid carry on through a breakdown, a track's own loudness range sets the absolute floor, and a section that comes back (a chorus, a drop) can be recognised as one heard before and its length in bars expected. A developer can save the history to disk as a session (a WAV of the audio and the signals beside it): that is the test bench of decision 4, made from live listening.

### 6. A director view for the people who make visuals

The audience never sees it. Whoever builds and tunes visuals can open a director view over the running app: a live scrolling spectrogram (log frequency, dB colour), the band curves, onset markers with their thresholds, the beat grid and phase, the confidences, the capture's health. It shows the same signals the visuals read, from the engine's history, so it can be frozen and scrolled back over the last minutes. It makes sense of why a visual did what it did.

### 7. Signals carry their meaning

Each band has an absolute level, a relative level (against its own recent past) and an attack, so "the bass is loud" and "the bass just got louder" are separate inputs, and an absolute floor keeps a near-silent passage from being scaled up to full. Inferred signals (tempo, bars, builds, drops) carry a confidence and may say "not sure". Names describe what is measured ("low attack", "build intensity"), not instruments or feelings. New fields reach visuals additively in `V`; existing names keep working.

## Steps

0. **Hits that are never dropped.** Running onset counts per group in the analysis; the app turns new counts into the frame's hits.
1. **Capture and live history.** (a) Capture: every waiting packet read at once and sent whole, WASAPI's discontinuity and silent flags honoured, silence while nothing plays filled in real time, a running frame clock, health counters (glitches, gaps, clipping, reopens, device changes), endpoint loopback following a change of default device. (b) History: the per-hop signal record and the audio ring, split by track, saved as a session on request; `analyze_wav` replays a session or a WAV and renders the timeline sheet; the app and the lab can play one (`AUDIOVIZ_WAV=`, `--wav`). (c) Each signal's delay measured from sound to screen. (d) The director view, first version: a live spectrogram, band curves, onset and beat markers and capture health, read from the history, with freeze.
2. **Bands.** Measure delay and separation on synthetic kicks and tones, then rebuild the low end (a filterbank or a shorter transform, whichever measures better); absolute, relative and attack per band; six broad bands with adjustable edges beside the 32; noisiness (spectral flatness), stereo balance, activity; the per-layer band view made accurate with the per-band attacks.
3. **Groove.** Faster lock-in, a fresh start on a track change, the track's settled tempo held from its history through breakdowns, pulse stability, bar and downbeat estimates with their own confidence.
4. **Context.** Trends over several timescales, contrast, transitions, build intensity and a real drop detector that replaces the director's cue; from the track's history, sections that come back recognised and the next change expected on the bar grid (a prediction with a confidence).
5. **Director tools.** Mappings (response curves, separate attack and release, threshold with hysteresis, confidence gating), and the director view grown to show, for a chosen parameter, the raw signal, the processed control and the value it reached. Every step before adds its new signals to the director view as it lands.

## Consequences

- Visuals keep reading `V` as now; new signals arrive as new fields, documented in `docs/visuals.md`.
- The lab's made-up groove stays for motion checks (ADR-0004); recorded clips are for analysis.
- Working log: `docs/phase8-audio.md`.
