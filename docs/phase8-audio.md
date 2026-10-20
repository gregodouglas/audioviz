# Phase 8 — the audio engine

The plan and its reasons are ADR-0006 (`docs/decisions/ADR-0006-audio-engine.md`); this is the working log, newest last.

## Progress log

- **2026-10-19, 0.4.30: step 0, hits that are never dropped.** The onset detector counts its hits per group (`OnsetDetector::count`, `FeatureFrame::onset_count`). The app compares each frame's counts with the last frame's and sets `onset_hit` from that, so `V.hits` is 1 on exactly one frame per onset. Before, it read the hop's own flag from the latest snapshot, and a hop (10.7 ms) is shorter than a frame: at 60 fps about a third of hits were lost, and at 144 fps some showed on two frames. The engine's tap-along onset log compares counts too, and the per-layer band view carries the count of the group it picks.
