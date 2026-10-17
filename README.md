# audioviz

A spacey, line-based music visualizer for whatever Windows is playing, Spotify first. Native Rust + wgpu. Not a MilkDrop clone.

| Phosphor scope | Flow-field ribbons | Lensed starfield |
|---|---|---|
| ![scope](docs/img/scene-1.png) | ![ribbons](docs/img/scene-2.png) | ![starfield](docs/img/scene-3.png) |

Three scenes with cross-fades: a phosphor XY oscilloscope of the real stereo signal, chroma-steered flow-field ribbons (GPU compute), and a gravitationally lensed starfield with a Swiss-grid title overlay fed by the Windows now-playing metadata. Per-process Spotify loopback with auto-reconnect, full real-time analysis (bands, loudness, onsets, predictive beat clock, chroma), bloom + AgX + auto-exposure post stack, frame cap, monitor selection, optional auto director that changes scenes on track changes, drops and quiet passages.

## Install (Windows 10 2004+ / 11, DirectX 12 or Vulkan GPU)

- **Installer:** download `audioviz-<version>-setup.exe` from the [latest release](https://github.com/gregodouglas/audioviz/releases/latest). Installs per-user (no admin prompt) into `%LOCALAPPDATA%\Programs\audioviz`, adds a Start Menu shortcut, and optionally starts when you sign in.
- **Portable:** download `audioviz-<version>-portable-win64.zip`, unzip anywhere, run `audioviz.exe`.

**SmartScreen:** releases are not yet code-signed, so Windows shows "Windows protected your PC" the first time. Click *More info* → *Run anyway*, or verify the file against `SHA256SUMS.txt` on the release page first. Signing is planned; `PRIVACY.md` tracks the status.

Start Spotify, play something, start audioviz. If Spotify is not running the app visualizes the default output device instead and says so on screen.

## Use

Press **Tab** for the settings panel. Everything is a live slider and every section has a reset button.

| Key | Action |
|---|---|
| Tab | settings panel |
| 1 / 2 / 3, Space | choose scene / next scene |
| A | auto scene director |
| T | title overlay (starfield) |
| C | audio-delay calibration: tap Space on the beat, then apply |
| F11 | fullscreen on the current monitor |
| Esc | quit |

Loopback captures audio before it reaches your ears, so the visuals lead the sound by your output latency (wired headphones ~10-40 ms, Bluetooth 100-250 ms). Set *Sync → audio delay* by ear or press **C** and tap along for ten beats; the app measures the offset and offers to apply it.

Settings live in `%APPDATA%\audioviz\settings.json` (plain JSON; delete it to reset). `audioviz --version` prints the version.

## Privacy and trust

No telemetry, no network code, no accounts. `PRIVACY.md` lists exactly what the app touches and how to verify the no-network claim yourself (`cargo deny` bans every HTTP/TLS/socket crate and CI enforces it). `SECURITY.md` says how to report a problem.

Direct dependencies (all MIT/Apache-2.0 or similar; `cargo deny check licenses` enforces the allow-list): `wgpu`, `winit`, `egui` + `egui-wgpu` + `egui-winit`, `wasapi`, `windows`, `sysinfo`, `realfft`, `image`, `glam`, `bytemuck`, `crossbeam-channel`, `serde` + `serde_json`, `dirs`, `tracing` + `tracing-subscriber`, `anyhow`, `pollster`. The icon generator (`xtask`) additionally uses `resvg` and `ico` and is not part of the shipped binary.

## Build from source

```
cargo build --release -p audioviz        # target/release/audioviz.exe, no console window
cargo run -p audioviz                    # debug build: console logs + WGSL hot reload (F5)
cargo test --workspace
cargo xtask icon                         # regenerate assets/icon.{svg,png,ico}
scripts/release.ps1                      # installer + portable zip + checksums + winget manifests → dist/
```

Dev overrides: `AUDIOVIZ_SCENE=n`, `AUDIOVIZ_SIZE=WxH`, `AUDIOVIZ_DEFAULTS=1` (ignore the settings file), `RUST_LOG=audioviz=debug` for a per-second stats line. `scripts/snapshot.ps1` runs the app for N seconds and captures the window to `target/snapshot.png`.

Releases: bump `version` in `Cargo.toml`, commit, `git tag vX.Y.Z && git push --tags`. The release workflow builds and tests on Windows and macOS, packages everything with `scripts/release.ps1`, and opens a draft GitHub release with the notes from `docs/release-notes-template.md`.

## Status and docs

**v0.1.0 released 2026-10-17** (Phase 5, ship-ability). Phase briefs: `docs/phase1-spike.md` (build/run, latency protocol), `docs/phase2-analysis.md`, `docs/phase3-polish.md`, `docs/phase4-scenes.md`, `docs/phase5-ship.md`, `docs/phase6-ui.md` (next: UI refresh). Backlog in `docs/ROADMAP.md` and the issue tracker. Research in `docs/research/00-synthesis.md`; decisions in `docs/decisions/`. macOS follows the UI refresh; the core already builds there in CI with null audio/metadata backends.

## Layout

- `crates/audio` — `AudioSource` trait + WASAPI per-process / endpoint loopback backend
- `crates/analysis` — `Analyzer` → `FeatureFrame` (bands, loudness, onsets, beat clock, chroma)
- `crates/metadata` — `MetadataSource` trait + Windows SMTC backend (now playing + art)
- `crates/app` — winit + wgpu app: `scene.rs` / `director.rs` / `post.rs`, `scenes/` (scope, ribbons, starfield), `overlay.rs`, egui panel in `ui.rs`, WGSL in `shaders/`, `build.rs` embeds the icon and version resource
- `xtask` — `cargo xtask icon`
- `installer/audioviz.iss` — Inno Setup script; `scripts/release.ps1` — packaging; `winget/` — manifest templates for a future winget submission (not submitted)
- `.github/workflows/` — `ci.yml` (build + test on Windows and macOS, `cargo deny`), `release.yml` (tag → draft release)
- `docs/research/` — research briefs (`00-synthesis.md` first)

## License

MIT, © 2026 Gregory Douglas.
