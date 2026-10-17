# Phase 5 — ship-ability

Status: **built 2026-10-17, first release pending the push** (brief written 2026-10-17 at the end of the Phase 4 session). Goal from `docs/research/00-synthesis.md`: installable by someone who isn't Greg; trustworthy by construction (open source, signed, no telemetry).

## Progress log

**2026-10-17.** Decisions: hosting = GitHub, Greg's personal account (`gregodouglas/audioviz`, public, MIT); publisher identity = personal, "Gregory Douglas" (winget id `GregoryDouglas.audioviz`); ship unsigned first, Azure Artifact Signing steps are in `release.yml` behind the `CODE_SIGNING` repo variable.

Done and verified locally:
- `ci.yml` (Windows + macOS build/test, `cargo deny`), `release.yml` (tag → draft release with installer, portable zip, SHA256SUMS, winget manifests, notes). `deny.toml` passes: advisories, licenses, no-network bans, sources.
- `scripts/release.ps1` builds everything into `dist/`; the Inno installer was installed silently to a scratch dir, checked (uninstall key, Start Menu shortcuts, settings untouched) and uninstalled. `winget validate` passes on the rendered manifest.
- Icon (`cargo xtask icon`), exe version resource, window icon, `--version`/`--help`.
- `PRIVACY.md`, `SECURITY.md`, README with screenshots (`docs/img/`), `docs/release-notes-template.md`, `docs/ROADMAP.md`.
- Loose ends: per-scene post settings (blended across cross-fades, old global values migrated), tap-along latency calibration (C, Space, Enter), panel hidden on first run with a hint, on-screen notice when capture falls back to the endpoint or fails.

Later the same day: pushed (CI green on Windows, macOS, cargo-deny), tagged `v0.1.0`; the Release workflow produced a **draft** release with installer, portable zip and SHA256SUMS (Windows job 14 min, macOS 6 min). Greg installed the local build, confirmed the tap calibration works. Per Greg, nothing user-facing mentions winget until the package is actually published; the manifests remain a workflow artifact.

Open:
- Publish the draft release when Greg is ready (edit the highlights in the notes first).
- Exit-criteria check on a second machine or clean VM with the installer from the release.
- Measured default delay: Greg to run the tap calibration on the reference rig and record it in `docs/phase1-spike.md`.
- UI refresh (custom chrome, panel, installer look) is tracked in `docs/ROADMAP.md` and GitHub issues, not in this phase.

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
