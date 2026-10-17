# Privacy

audioviz is a local music visualizer. It has no accounts, no telemetry, no crash reporting, no update checks and no network code of any kind.

## What the app touches

| Access | Why | Where |
|---|---|---|
| Windows audio engine (WASAPI loopback) | Reads the audio Spotify (or the default output device) is playing, in memory, to draw it. Nothing is recorded or written to disk. | `crates/audio/src/windows.rs` |
| Windows media session API (SMTC) | Reads the now-playing title, artist, album and album art that the player already publishes to Windows (the same data the volume flyout shows). | `crates/metadata/src/windows.rs` |
| Process list | Finds `Spotify.exe` (or the process you name in settings) to attach per-process capture. | `crates/audio/src/windows.rs` |
| `%APPDATA%\audioviz\settings.json` | Your slider values, chosen scene, monitor and window options. Plain JSON; delete it to reset. | `crates/app/src/config.rs` |
| `C:\Windows\Fonts` (read only) | Loads Segoe UI / Arial for the title overlay if present. | `crates/app/src/overlay.rs` |
| GPU (DirectX 12 or Vulkan) | Rendering. | `crates/app/src/gpu.rs` |

Nothing leaves the machine. The app never opens a socket.

## How to verify that yourself

The claim is checked mechanically rather than taken on trust:

1. **No network crates in the dependency tree.** `deny.toml` bans every common Rust HTTP, TLS, DNS, WebSocket and socket crate (`reqwest`, `hyper`, `ureq`, `curl`, `rustls`, `native-tls`, `openssl`, `tungstenite`, `mio`, `socket2`, `sentry`, …). CI runs `cargo deny check` on every push and the build fails if one appears.
   ```
   cargo deny check bans
   cargo tree -e normal -p audioviz | grep -iE "http|tls|socket|mio"   # prints nothing
   ```
2. **Open source, reproducible from the tag.** Every release is built by the public GitHub Actions workflow (`.github/workflows/release.yml`) from a tagged commit, with `Cargo.lock` locked. You can build the same tag yourself with `cargo build --release -p audioviz --locked` and compare.
3. **Watch it.** Run it under Resource Monitor or `netstat -b` and you will see no connections from `audioviz.exe`.

## Code signing

Releases are currently **unsigned**. Windows SmartScreen will warn on first run of the installer ("Windows protected your PC" → *More info* → *Run anyway*). Verify the download against `SHA256SUMS.txt` on the release page instead. Signing with Azure Artifact Signing is wired into the release workflow and will be turned on when a certificate is in place; this file and the README will say so when that changes.

## Contact

Questions or concerns: open an issue on the repository, or see `SECURITY.md` for anything sensitive.
