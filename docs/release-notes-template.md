## audioviz {{VERSION}}

<!-- Edit the highlights on the draft release before publishing. -->

### Highlights

- 

### Install

- **winget:** `winget install {{PACKAGE_ID}}` (after the manifest is merged into winget-pkgs; until then use the installer below).
- **Installer:** `audioviz-{{VERSION}}-setup.exe` — per-user, no admin prompt, Start Menu shortcut, optional "start when I sign in".
- **Portable:** `audioviz-{{VERSION}}-portable-win64.zip` — unzip and run `audioviz.exe`.

Requires Windows 10 2004 (build 19041) or later and a GPU with DirectX 12 or Vulkan. Spotify is captured per-process when it is running; otherwise the default output device is visualized.

### Trust

This release is **not code-signed**. Windows SmartScreen will show "Windows protected your PC" on first run: click *More info* → *Run anyway*. Verify the download instead with the checksums below. The app makes no network connections and sends no telemetry (see `PRIVACY.md`; enforced by `cargo deny` in CI).

### SHA-256

```
{{SHA256SUMS}}
```
