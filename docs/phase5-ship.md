# Phase 5 — ship-ability

Status: **not started** (brief written 2026-10-17 at the end of the Phase 4 session). Goal from `docs/research/00-synthesis.md`: installable by someone who isn't Greg; trustworthy by construction (open source, signed, no telemetry).

## Where the project stands

Phases 1-4 are done and confirmed by Greg on his machine (Windows 11, RTX 2070 Super, portrait second monitor). Three scenes (phosphor scope, flow-field ribbons, lensed starfield with Swiss title overlay), per-process Spotify loopback with auto-reconnect, full v1 analysis, post stack, frame cap, monitor selection, persisted settings, Windows now-playing metadata, auto director. `cargo build --release` produces `target/release/audioviz.exe` with no console window. No remote is configured; the repo lives only at `W:\projects\audioviz`.

## Scope

1. **Repository and release pipeline**
   - Decide hosting: GitHub (public, MIT) for releases/winget, optionally mirrored to Greg's Gitea on gdbastion (`/gitea` skill). Add remote(s), push.
   - GitHub Actions: on tag, build Windows release, run `cargo test`, upload `audioviz.exe` + installer + SHA256SUMS. A second job builds macOS (even though the audio/metadata backends are null there) so the core stays green for Phase 6.
   - Version from `Cargo.toml` (bump to 0.1.0 for the first tag), `--version` flag, version shown in the panel header.
2. **Installer and app identity**
   - App icon (.ico from an SVG of the mark; the scope's phosphor-green spine on black is the obvious motif), window icon via winit, exe resources via `winres`/`embed-resource`.
   - Installer: Inno Setup or `cargo-wix`; per-user install (no admin), Start Menu shortcut, optional "start with Windows", uninstaller removes settings only if asked. Also a portable zip.
   - Settings file location documented; first-run defaults are the current `Settings::default()`.
3. **Trust**
   - Code signing: Azure Artifact Signing (formerly Trusted Signing), Basic tier, under Byte Clarity's validated identity; wire into CI via the Azure action. Until then releases are unsigned and SmartScreen will warn, so the README must say so plainly.
   - `PRIVACY.md`: no telemetry, no network calls (the app talks only to the Windows audio engine and media-session API). Verifiable: no HTTP crate in the dependency tree.
   - `cargo deny` or `cargo audit` in CI; dependency list in the README.
4. **Distribution**
   - winget manifest (`Greg.audioviz` or `ByteClarity.audioviz`) pointing at the GitHub release installer; validate with `winget validate` and `wingetcreate`.
   - GitHub release notes template; screenshots of the three scenes (use `scripts/snapshot.ps1`).
5. **Loose ends worth folding in while shipping**
   - Per-scene post settings (the starfield wants less bloom than the scope).
   - Latency calibration mode (click + flash) and a measured default; protocol in `docs/phase1-spike.md`.
   - Hide the debug panel by default on first run; show a one-line hint ("Tab for settings").
   - Graceful message when no audio device / Spotify not running (currently falls back silently to endpoint capture).

## Exit criteria

- A tagged release on the chosen host with installer, portable zip, checksums, and release notes.
- `winget install <id>` works on a clean Windows 11 VM or a second machine.
- README with screenshots, install instructions, privacy statement, and the honest signing status.
- CI green on Windows and macOS.

## Decisions Greg needs to make early

- Hosting: GitHub public, Gitea only, or both.
- Publisher identity for signing and winget (personal vs Byte Clarity).
- Whether to pay for signing now or ship unsigned first.
