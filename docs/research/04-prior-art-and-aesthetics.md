# 04 — Prior art and aesthetic lineage

> Research brief, 2026-10-16. Produced by a Sonnet research subagent. Items marked **[unverified]** came from model knowledge rather than a retrieved source and must be checked before anything is built on them (prices especially). Links were not all opened.

## 1. Classic and current visualizers

| Name | Status (Oct 2026) | Platform / audio source | Price | Aesthetic and complaints |
|---|---|---|---|---|
| MilkDrop 2 (Ryan Geiss) | Dormant. Last stable 2.25d (2022), source-available ([Wikipedia](https://en.wikipedia.org/wiki/MilkDrop)) | Winamp plugin, Windows. Host feeds audio. | Free | Feedback warps plus per-pixel equations and a huge preset ecosystem. Great because presets were a user-authored language. Dated, psychedelic blobs. |
| MilkDrop 3 and forks (BeatDrop, MDropDX12) | Community, active. MilkDrop3 last pushed Mar 2026; MDropDX12 at 2.7.0; foobar2000 [foo_vis_milk2](https://www.foobar2000.org/components/view/foo_vis_milk2/release/0.6.2-beta4) beta Jan 2026. | Windows, WASAPI loopback. | Free | Same look. Unofficial forks with murky provenance. This *is* the "unknown/untrustworthy" problem. |
| projectM | Alive. libprojectM 4.1.8 (Oct 2026), maintenance fixes ([releases](https://github.com/projectM-visualizer/projectm/releases)). | Cross-platform C++/OpenGL library; hosts supply PCM. | Free, LGPL | Reimplements MilkDrop. A library, not a polished end-user app. |
| Butterchurn | Alive, low activity [unverified]. | Web (WebGL), Web Audio. | Free | MilkDrop in the browser. Browser-tab audio is awkward with Spotify. |
| G-Force / WhiteCap / Aeon / SoftSkies (SoundSpectrum) | Still sold; site copyright 2024 ([soundspectrum.com](https://www.soundspectrum.com/about.html)). | Win/Mac standalone, system audio. | ~$20-30 [unverified] | Glossy particle-and-shape "polished 2000s". |
| Plane9 | Alive, v2.5; GLSL conversion under way ([plane9.com](https://www.plane9.com/)). | Windows; screensaver, standalone, VR. | Free + paid [unverified] | 3D tunnels and particles. Dated. |
| Synesthesia | Alive; free download + Pro ([synesthesia.live](https://synesthesia.live/)). | Win/Mac; line-in or loopback, MIDI/OSC. | ~$149-399 [unverified] | Shader-based, best-looking pro option. VJ-oriented, not ambient. |
| Kauldron / Magic Music Visuals | Alive [unverified]. | Win/Mac node-based VJ tools. | Magic ~$100 [unverified] | Build-your-own. Too much work for casual listening. |
| VSXu | Effectively dead. | Linux/Win. | Free | Old node-based. |
| Resolume / TouchDesigner / Notch | Alive, pro. | Win/Mac. | Resolume €299-699; TD free non-commercial tier. | Not music-first. Powerful but demanding. |
| Apple Music / iTunes | iTunes visualizer dead; Apple Music has none [unverified]. | n/a | n/a | The iTunes visualizer was Hodgin's Magnetosphere (see §3). |
| Windows 11 Media Player | New app has no visualizer found ([Windows Central](https://windowscentral.com/windows-11-new-media-player)); legacy WMP still ships. | Windows | Free | WMP visualization SDK is [archived](https://learn.microsoft.com/en-us/previous-versions/aa393409(v=vs.85)). |
| Winamp | Legacy source released 2024 under a license barring forks ([PCWorld](https://www.pcworld.com/article/2470340/winamp-goes-open-source-sort-of-on-github-in-search-of-collaborators.html)); Llama Group's new Winamp is nostalgia branding. | Windows | Free | The host the old plugins lived in. |
| Spotify | Never shipped a true visualizer [unverified]. Canvas loops, lyrics, Now Playing visuals. | n/a | n/a | Web API cut audio-features/analysis for new apps 27 Nov 2024 ([forum](https://community.spotify.com/t5/Spotify-for-Developers/Changes-to-Web-API/m-p/6541868/highlight/true)). Raw audio is DRM-protected. |
| Wallpaper Engine | Alive. | Windows, desktop loopback. | ~$4 Steam [unverified] | Big audio-reactive wallpaper market, mostly low-effort. |
| cava / cavalier | Alive. | Linux, terminal/GTK. | Free | Bars only. |
| Steam indies | Small long tail: RippleFX (2025, $6.99), Chromesthesia VR (2024), VZX Player (2019, free), V/ARCH (Unity). A galaxy-themed visualizer listed for Sep 2026 ([Steam](https://store.steampowered.com/app/5064900)). | Windows, WASAPI loopback. | $0-10 | Mostly particles and bars. |

**Common complaints** (general knowledge, not retrieved this session): dated looks, unofficial installers and re-hosted download sites (the "virus-infested" perception), bundled adware on abandoned freeware, uncurated presets, crashes on modern GPUs, no Spotify integration beyond whatever loopback captures.

## 2. Why nothing modern has stuck

1. **Host-plugin ecosystem collapsed.** Winamp, WMP and iTunes hosted visualizers and handed them PCM. Streaming apps ship no plugin API.
2. **No raw audio, no analysis API.** Spotify DRM blocks direct decode; its analysis endpoints were restricted in Nov 2024. Developers must do their own DSP from OS loopback.
3. **Listening moved to phones.** The desktop is no longer the listening surface for most people.
4. **The market professionalized.** Enthusiasts went to VJ tools (Resolume, TouchDesigner, Synesthesia). Powerful, but not "put on a record and watch."
5. **Free-software distrust.** MilkDrop descendants are community forks with unclear provenance.

**The gap.** Nobody ships a small, signed, open-source, no-telemetry desktop app that listens to whatever is playing (Spotify included, via loopback) and renders something *designed* rather than preset-driven: a fixed, curated aesthetic (lines, space, typography) that works as an ambient second-monitor display. Current options are dated MilkDrop clones, VJ tools that need operating, or wallpaper-store noise. Spotify metadata (artist, album, track, art) is still reachable via OS media-session APIs and the Web API, which enables designed typographic and cover-art layers classic visualizers never had.

## 3. Aesthetic reference board

- **Ryoji Ikeda.** Data-driven lines, test patterns, black-and-white strobing. Map spectrum or bit-level data to barcode and pixel-grid blocks. [ryojiikeda.com](https://www.ryojiikeda.com/)
- **Refik Anadol.** Fluid data sculptures in volumetric color. Closest to the "blobby" look being avoided, but a reference for scale and depth. [refikanadol.com](https://refikanadol.com/)
- **Inconvergent / Anders Hoff.** Differential line growth, sand-splines. Onsets add nodes so a single line folds like coral. [inconvergent.net](https://inconvergent.net/)
- **Tyler Hobbs.** Fidenza, flow fields. Long ribbons in a controlled palette, curl driven by chroma. [Flow fields essay](https://tylerxhobbs.com/essays/2020/flow-fields)
- **Zach Lieberman.** Playful line and gesture sketches. [openFrameworks](https://openframeworks.cc/)
- **Robert Hodgin / Flight404.** Magnetosphere (became the iTunes visualizer), Solar, Cell. Particle trails with soft glow; the best precedent for "spacey that still feels designed." [flight404.com](http://flight404.com/)
- **Memo Akten.** Learning-based and physics-based abstraction. [memo.tv](https://www.memo.tv/)
- **Dave Whyte (beesandbombs).** Geometric looping GIFs. Clean geometry, exact timing, ideal for beat-locked loops. [beesandbombs](https://beesandbombs.tumblr.com/)
- **Kim Asendorf.** Pixel sorting, glitch. [kimasendorf.com](https://kimasendorf.com/)
- **Jerobeam Fenderson / Hansi Raber / OsciStudio / osci-render.** Oscilloscope music, where the stereo audio literally *is* the picture. The most distinctive anti-MilkDrop identity. [oscilloscopemusic.com](https://oscilloscopemusic.com/), [osci-render](https://osci-render.com/)
- **Universal Everything (Matt Pyke).** Calm, slow, procedural figures. [universaleverything.com](https://www.universaleverything.com/)
- **Joshua Davis.** Algorithmic composition and color. [joshuadavis.com](https://joshuadavis.com/)
- **Paul Bourke attractors.** Lorenz, Clifford, de Jong, Pickover. Parameters drive the shape. [paulbourke.net/fractals](https://paulbourke.net/fractals/)
- **Hubble / JWST / Gaia.** Real data as a design source: diffraction spikes, dust lanes, star fields. [Gaia sky map](https://www.esa.int/ESA_Multimedia/Images/2018/04/Gaia_s_sky_in_colour), [JWST gallery](https://webbtelescope.org/news/news-releases)
- **Interstellar / Kip Thorne.** Gargantua lensing. [arXiv:1502.03808](https://arxiv.org/abs/1502.03808). Bass deflects a star field.
- **Tron Legacy, Blade Runner 2049.** Territory Studio, GMUNK. Thin cyan and amber vector UI, wireframes, data readouts. [territorystudio.com](https://territorystudio.com/), [gmunk.com](https://gmunk.com/)
- **Swiss / International Typographic Style, brutalist web.** Grid, one typeface, asymmetry, rules as structure. The "post-modern" type layer. [Wikipedia](https://en.wikipedia.org/wiki/International_Typographic_Style), [brutalistwebsites.com](https://brutalistwebsites.com/)
- **Demoscene minimalism.** Mercury, Conspiracy. Tiny code, strong rhythm, restraint. [mercury.sexy](https://www.mercury.sexy/), [demozoo](https://demozoo.org/)
- **Max Cooper's collaborators.** Generative and scientific imagery paired to music. [maxcooper.net](https://maxcooper.net/)

## 4. Concepts

1. **Phosphor XY scope.** L/R as X/Y with beam physics: persistence decay, bloom, intensity from beam speed. Near it: Fenderson, osci-render, scope plugins. Unexplored: a polished ambient app treating *real music* (not specially authored tracks) with curated gain, rotation and phosphor. GPU: trivial; additive segments into a decaying float buffer.
2. **Chroma-driven flow-field ribbons.** Long-lived lines whose curl field is steered by the chroma vector and key, fixed palette. Near it: Hobbs. Unexplored: pitch content steering the field. GPU: compute advection + instanced ribbons; easy at 100k+ lines.
3. **Gravitational-lens star field.** Bass drives a lensing mass, bands perturb the field. Near it: Interstellar renders. Unexplored: music-controlled lensing as a primary ambient scene. GPU: fragment shader with a deflection function. Cheap.
4. **Strange-attractor drift.** Clifford/de Jong parameters wander with centroid and flux. Near it: Bourke, chaos screensavers. Unexplored: continuous parameter-space traversal. GPU: millions of points via compute, density accumulation.
5. **Differential-growth line.** Onsets inject nodes so a single closed curve grows. Near it: Hoff, Nervous System. Unexplored: audio-timed growth. GPU: neighbor search is the cost; fine at 10-50k nodes with a spatial hash.
6. **Spectrogram ribbon / slit-scan.** Time becomes depth, the spectrogram a 3D trail receding into space. Near it: Unknown Pleasures. Unexplored: cinematic camera. GPU: scrolling texture + displaced mesh.
7. **Mid/side stereo field.** Stereo image as a 3D point cloud, correlation as depth. Near it: goniometer plugins. Unexplored: spatial, artistic treatment. GPU: easy.
8. **Swiss-grid typographic layer.** Spotify metadata set in a rigorous grid, rules and type weights reacting to bands and beats. Near it: Ikeda-style readouts. Unexplored: type as the primary reactive element. GPU: SDF text.
9. **Orbital mechanics.** Beats as periapsis passes; tempo sets orbital period. Unexplored. GPU: trivial.
10. **Constellation per track.** Album/track metadata seeds a star pattern; the spectrum perturbs gravitation. Near it: few or none. GPU: easy.

**Ranked shortlist to prototype first.**
1. **Phosphor XY scope.** Most distinctive, needs only PCM, cheap on GPU, nobody has made it a polished ambient app.
2. **Chroma flow-field ribbons.** Closest to "lines and spacey," visually rich, exercises the DSP (chroma, onsets) the Spotify API no longer supplies.
3. **Gravitational-lens star field plus Swiss type overlay.** Cheap, instantly beautiful, delivers the "post-modern" identity through the metadata layer, and prototypes the design system other scenes share.

## 5. Trust and distribution

- **Windows signing.** Microsoft Artifact Signing (formerly Trusted Signing), Basic and Premium tiers ([Azure](https://azure.microsoft.com/en-us/pricing/details/artifact-signing/)); Basic historically ~$10/month with identity validation [unverified]. EV certs cost several hundred dollars a year and since 2024 no longer grant instant SmartScreen reputation. Byte Clarity as a validated org would qualify.
- **macOS.** Apple Developer Program (~$99/yr [unverified]), Developer ID signing and notarization.
- **Trust signals.** Permissive open-source license, reproducible CI builds with checksums, no telemetry (stated plainly), a visible permissions model (loopback audio only, no network except optional metadata), minimal-dependency SBOM.
- **Auto-update.** Velopack, Squirrel-style, Sparkle on macOS, or defer to a store. Sign manifests.
- **Distribution.** Steam ($100/app fee [unverified]) gives reputation and updates. winget for a free signed installer. Microsoft Store handles signing but its loopback/sandbox implications need checking. Homebrew Cask for macOS. GitHub Releases as baseline.

**Design implication.** Loopback capture plus self-computed features (FFT, chroma, onset, tempo). OS media-session APIs or the Web API only for metadata and cover art.
