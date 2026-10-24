# Privacy

audioviz is a local music visualizer. It has no accounts, no telemetry and no crash reporting. It makes one kind of network request, and only when you press a button (below).

## What the app touches

| Access | Why | Where |
|---|---|---|
| Windows audio engine (WASAPI loopback) | Reads the audio Spotify (or the default output device) is playing, in memory, to draw it. The last two minutes are kept in memory so the director view can save them (next row); nothing is written to disk unless you ask. | `crates/audio/src/windows.rs`, `crates/app/src/engine.rs` |
| Microphone (only if you pick it) | Sync › Source › *The room*, for music playing somewhere else. Same handling as loopback. Windows shows its microphone-in-use indicator while it runs. | `crates/audio/src/windows.rs` |
| Windows media session API (SMTC) | Reads the now-playing title, artist, album and album art that the player already publishes to Windows (the same data the volume flyout shows). | `crates/metadata/src/windows.rs` |
| Process list | Finds `Spotify.exe` (or the process you name in settings) to attach per-process capture. | `crates/audio/src/windows.rs` |
| `%APPDATA%\audioviz\` | `settings.json` (your settings, plain JSON; delete it to reset) and the folders of scenes, presets and visuals, the shipped ones included, which you can edit. | `crates/app/src/config.rs`, `crates/app/src/factory.rs` |
| `%APPDATA%\audioviz\sessions\` (only when you press **S** in the director view) | Saves the last two minutes of audio as `session-<time>.wav` plus a `.json` of the director's marks, for looking at how the music was read. Nothing is saved otherwise; delete the files whenever you like. | `crates/app/src/engine.rs` |
| A photos folder (only if you set one) | The Kaleidoscope's *my photos* source reads pictures from the folder you choose. Read only. | `crates/app/src/settings.rs` |
| GPU (DirectX 12 or Vulkan) | Rendering. | `crates/app/src/gpu.rs` |

Nothing leaves the machine on its own. The app opens exactly one kind of connection, and only when you ask for it:

| Access | Why | Where |
|---|---|---|
| **Check for updates** button (About section) | When pressed, fetches `https://api.github.com/repos/gregodouglas/audioviz/releases/latest` once over the Windows HTTP stack (WinHTTP) and compares the tag with the running version. Sends the request GitHub needs and nothing else (no identifiers, no settings). Never runs on a timer or at startup. | `crates/app/src/platform/windows.rs` |

Links in the About section (releases, privacy, issues) open in your browser; the app itself does not fetch them.

The app contains no AI model and calls no AI service. How AI was used to *build* it is in [`AI_DISCLOSURE.md`](AI_DISCLOSURE.md).

## How to verify that yourself

The claim is checked mechanically rather than taken on trust:

1. **No network crates in the dependency tree.** `deny.toml` bans every common Rust HTTP, TLS, DNS, WebSocket and socket crate (`reqwest`, `hyper`, `ureq`, `curl`, `rustls`, `native-tls`, `openssl`, `tungstenite`, `mio`, `socket2`, `sentry`, …). CI runs `cargo deny check` on every push and the build fails if one appears. The manual update check goes through the operating system's WinHTTP API in one small, readable function, so the ban list still holds and there is exactly one place to audit.
   ```sh
   cargo deny check bans
   cargo tree -e normal -p audioviz | grep -iE "http|tls|socket|mio"   # prints nothing
   ```
2. **Open source, reproducible from the tag.** Every release is built by the public GitHub Actions workflow (`.github/workflows/release.yml`) from a tagged commit, with `Cargo.lock` locked. You can build the same tag yourself with `cargo build --release -p audioviz --locked` and compare.
3. **Watch it.** Run it under Resource Monitor or `netstat -b` and you will see no connections from `audioviz.exe` unless you press *Check for updates*.

## Code signing

Releases are currently **unsigned**. Windows SmartScreen will warn on first run of the installer ("Windows protected your PC" → *More info* → *Run anyway*). Verify the download against `SHA256SUMS.txt` on the release page instead. Signing with Azure Artifact Signing is wired into the release workflow and will be turned on when a certificate is in place; this file and the README will say so when that changes.

## Contact

Questions or concerns: open an issue on the repository, or see [`SECURITY.md`](SECURITY.md) for anything sensitive.
