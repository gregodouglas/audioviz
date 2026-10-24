## audioviz {{VERSION}}

<!-- Edit the highlights on the draft release before publishing. -->

### Highlights

- 

### Install

- **Installer:** `audioviz-{{VERSION}}-setup.exe` — per-user, no admin prompt, Start Menu shortcut, optional "start when I sign in".
- **Portable:** `audioviz-{{VERSION}}-portable-win64.zip` — unzip and run `audioviz.exe`.

Requires Windows 10 2004 (build 19041) or later and a GPU with DirectX 12 or Vulkan. Spotify is captured per-process when it is running; otherwise the default output device is visualized.

### Trust

This release is **not code-signed**. Windows SmartScreen will show "Windows protected your PC" on first run: click *More info* → *Run anyway*. Verify the download instead with the checksums below. The app sends no telemetry and connects to nothing except the update check you press yourself (see `PRIVACY.md`; enforced by `cargo deny` in CI).

audioviz is written by Claude Code under Greg Douglas's direction; see `AI_DISCLOSURE.md`.

### SHA-256

```
{{SHA256SUMS}}
```
