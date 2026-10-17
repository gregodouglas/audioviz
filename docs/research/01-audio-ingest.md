# 01 — Getting audio and metadata out of Spotify

> Research brief, 2026-10-16. Produced by a Sonnet research subagent, spot-checked by the lead session. Items marked **[unsourced]** are background knowledge and must be measured or verified. Several 2026 Spotify details come from third-party blogs, not Spotify.

## 1. Windows loopback

**Standard WASAPI loopback** captures the mixed render stream of an endpoint. Works only on shared-mode streams; exclusive-mode streams cannot be looped back. Event-driven loopback has been supported since Windows 10 1703. ([MS Loopback Recording](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording))

**Per-process loopback**: `ActivateAudioInterfaceAsync` with the process-loopback activation type and `AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS`.
- Target a PID with a mode that includes or excludes that process tree. Spotify's helper processes are covered by "include tree". ([PROCESS_LOOPBACK_MODE](https://learn.microsoft.com/en-us/windows/win32/api/audioclientactivationparams/ne-audioclientactivationparams-process_loopback_mode), [params struct](https://learn.microsoft.com/en-us/windows/win32/api/audioclientactivationparams/ns-audioclientactivationparams-audioclient_process_loopback_params))
- The activation completion handler runs on an MTA worker thread. ([ActivateAudioInterfaceAsync](https://learn.microsoft.com/en-us/windows/desktop/api/mmdeviceapi/nf-mmdeviceapi-activateaudiointerfaceasync))
- Microsoft's reference lists a minimum of Windows 10 build 20348, which conflicts with the usual "2004+" claim. Moot on Windows 11. The official sample is "ApplicationLoopback" in Microsoft's Windows-classic-samples repo.
- GStreamer's `wasapi2src` exposes `loopback-mode` and `loopback-target-pid`, a second implementation to read.

**Gotchas** [unsourced unless noted]:
- Loopback delivers no packets while nothing plays. Drive your own timer and treat silence as zeros.
- Process loopback captures at a fixed engine format, commonly 32-bit float, 44.1 or 48 kHz stereo. Request the format with `AUTOCONVERTPCM`/`SRC_DEFAULT_QUALITY` or resample yourself.
- Process loopback captures pre-endpoint audio, so system volume and mute probably don't affect it (verify).
- Spotify must be running before you resolve its PID. Re-enumerate on restart.

**Latency**: shared-mode event-driven buffers are about 10 ms, so expect 10 to 30 ms of capture-side latency. No measured source found. [unsourced]

**Rust crates**
- [`wasapi`](https://github.com/HEnquist/wasapi-rs) (HEnquist) supports loopback, and its `record_application` example records a single application via process loopback. Needs Rust 1.88+ (we have 1.97). **Best Rust option.**
- `cpal`: the README (checked 2026-10-16 by the lead session) documents loopback only for macOS ("loopback recording requires 14.6+") and says nothing about Windows loopback. No per-process loopback. Treat Windows loopback via cpal as unconfirmed.
- The `windows` crate gives raw COM access if binding the API directly.

**Recommendation**: per-process loopback through the `wasapi` crate (or `windows` crate), include-tree on `Spotify.exe`, with fallback to endpoint loopback for "whatever is playing".

## 2. macOS capture

- **Core Audio process taps** (`CATapDescription`, `AudioHardwareCreateProcessTap`, macOS 14.2+) capture one app's outgoing audio: create the tap, attach to a HAL aggregate device, read buffers in an I/O callback. Multi-process apps need helper PIDs included. Docs are sparse. ([Recall.ai writeup](https://www.recall.ai/blog/core-audio-taps), [AudioTee](https://github.com/makeusabrew/audiotee), [catap](https://pypi.org/project/catap/))
- **ScreenCaptureKit** (macOS 13+): set `capturesAudio` and `excludesCurrentProcessAudio` on `SCStreamConfiguration`. Consent lives under Screen Recording privacy and the app must restart after the grant. No true audio-only mode; people set a tiny video size and frame rate. ([Apple sample](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos.md), [forum](https://developer.apple.com/forums/thread/718279), [screencapturekit crate](https://docs.rs/screencapturekit))
- **Permissions** [unsourced]: taps use an audio-capture TCC prompt (`NSAudioCaptureUsageDescription`), less alarming than Screen Recording. Unconfirmed.
- Rust: `coreaudio` crate is low-level; taps will need Swift/ObjC FFI or `objc2` bindings. cpal's macOS loopback is 14.6+.
- BlackHole / Loopback virtual devices remain the fallback for older macOS.

**Recommendation**: Core Audio process tap on macOS 14.2+ (Swift shim or objc2), ScreenCaptureKit as the 13.x fallback, BlackHole as last resort.

## 3. librespot (open-source Spotify Connect client, Rust)

**Activity**: latest release v0.8.0 (Nov 2025, presumably), after v0.7.x. v0.7.0 added `OAuthClient`, deprecated `get_access_token()`, replaced Mercury with Dealer. v0.8.0 introduced `SpotifyUri` (breaking), local file playback, FLAC. MSRV 1.85. No 2026 release found. ([releases](https://github.com/librespot-org/librespot/releases))

**Auth**: OAuth with PKCE ([librespot-oauth](https://lib.rs/crates/librespot-oauth)). Premium required.

**Active breakage (confirmed by lead session 2026-10-16)**: starting 10 Aug 2026, librespot 0.8.0 users report OAuth "authenticated" but `login5.spotify.com/v3/login` returns `INVALID_CREDENTIALS` for every track. Reported via [Mopidy-Spotify](https://discourse.mopidy.com/t/mopidy-spotify-not-working-gstreamer-no-resource-found/6324) (marked solved 20 Aug with a workaround post) and [Music Assistant](https://community.home-assistant.io/t/when-will-music-assistants-spotify-integration-be-fixed-at-the-librespot-streaming-layer-the-oauth-login-shows-authenticated-fine-but-under-the-hood-librespot-rejects-every-track-with-invalid-credentials/1020856) (issue #6051). A 2023 precedent ([go-librespot #364](https://github.com/devgianlu/go-librespot/issues/364)) was a "structural and definitive" Spotify-side change. Pattern: Spotify periodically breaks it and the community catches up weeks later.

**PCM and metadata**: pluggable `Sink` trait gives decoded PCM at 44.1 kHz. Spirc/Connect and player event channels give track and position events. The visualizer appears in the user's Spotify device list.

**ToS risk**: librespot's README says the project is "probably forbidden" by Spotify. Raspotify calls commercial or public use a flagrant violation. Private personal use is the lowest-risk case. The Spotify Developer Policy clause against synchronizing sound recordings with visual media applies to Web API apps. ([raspotify](https://github.com/dtcooper/raspotify), [terms](https://developer.spotify.com/terms))

**Recommendation**: optional "premium-quality" personal-use mode behind an `AudioSource` trait. Not the foundation, not distributed by default.

## 4. Web API and local hooks

**Web API 2026**
- Since Feb 2026, dev-mode apps require the owner to have Premium, 5 test users, 1 Client ID. ([TechCrunch](https://techcrunch.com/2026/02/06/spotify-changes-developer-mode-api-to-require-premium-accounts-limits-test-users/))
- July 2026 changelog: up to 25 apps per developer, quotas per account, 429 carries `QUOTA_EXCEEDED`. No endpoint removals listed. ([changelog](https://developer.spotify.com/documentation/web-api/references/changes/july-2026))
- Extended quota is organizations only (250k+ MAU, May 2025).
- Audio Features and Audio Analysis gone for new apps since Nov 2024. No reversal found.
- Canvas video has no official API.
- Album art comes from the track object's `album.images`.
- For a personal single-user app, dev mode suffices. Poll currently-playing every 1 to 3 s and interpolate locally.

**Windows SMTC** (`GlobalSystemMediaTransportControlsSessionManager`): media properties, playback info, timeline, change events, thumbnail. No API key, no network. Timeline updates are irregular, so extrapolate from the last-update timestamp. ([Windows.Media.Control](https://learn.microsoft.com/en-us/uwp/api/windows.media.control))

**macOS MediaRemote**: Apple gated private-framework access in 15.4. [ungive/mediaremote-adapter](https://github.com/ungive/mediaremote-adapter) routes through `/usr/bin/perl`, fragile. Alternatives: Spotify's AppleScript dictionary (`tell application "Spotify"`) for track and position, or the Web API.

**Recommendation**: SMTC on Windows. AppleScript or Web API on macOS, mediaremote-adapter optional.

## 5. Latency budget [unsourced; measure early]

- Loopback reads from the audio engine mix *before* the endpoint device, so capture sees the signal slightly before you hear it: engine + driver buffer + DAC, roughly 10 to 40 ms wired. Bluetooth adds 100 to 300 ms and Windows does not expose that delay to loopback clients.
- Render lag adds one to two frames plus compositor delay. Rendering as soon as audio arrives will light up slightly *early* relative to what you hear, not late.
- Provide a user "audio delay" slider: a ring buffer of samples (or feature frames) ahead of the analysis, default about 20 to 50 ms wired, saved per output device. A calibration screen (flash + click) is the practical answer.
- Never use SMTC or Web API position for beat sync; its error is seconds.
- librespot path: since you own playback, you can delay PCM to the output by your visual latency, giving truly sample-aligned visuals. Loopback cannot do that.

## Recommended architecture for audio ingest

| Option | Pros | Cons |
|---|---|---|
| (a) Loopback only | Simplest, no account, works with any client, no ToS exposure, per-process isolates Spotify | No metadata, sync depends on user tuning |
| (b) librespot Connect device | Sample-accurate PCM, rich metadata, align by delaying playback | ToS risk, no 2026 release, active Aug 2026 breakage, user must pick device, you own audio output |
| (c) Hybrid: loopback + SMTC (Win) / AppleScript or Web API (Mac) | Best value, lowest risk, metadata and art without keys | Sync still user-calibrated |

**Build (c) first**, behind a clean `AudioSource` trait so (b) can be added later as an optional personal-use backend. Spend an early day measuring loopback-to-ear latency with a test tone and a microphone or phone camera.

**Gaps to close in week 1**: run the ApplicationLoopback sample or `wasapi-rs` `record_application` against Spotify; confirm whether system volume affects process loopback; confirm SMTC thumbnail update behaviour with Spotify; check the live currently-playing endpoint reference.
