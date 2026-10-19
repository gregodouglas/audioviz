# audioviz: project rules

Spotify-fed, Windows-first music visualizer (Rust, wgpu, WGSL). Start with `docs/model.md` (the words), `docs/visuals.md` (the visual format) and `docs/decisions/` (ADRs); the working log is `docs/phase6-ui.md`, open issues `docs/ROADMAP.md`.

## Versions and commits

- **Every commit bumps the patch version by 0.0.1** in the workspace `Cargo.toml` (`version = "x.y.z"`; the crates inherit it). Bump in a step of its own and build once so `Cargo.lock` follows. Then stage both and commit in a later command: the hook below checks before a command runs, so a bump inside the commit command is not seen.
- The commit message starts with the version: `0.4.3: what changed, in one line of plain sentences`.
- A minor or major bump (0.5.0) only when Greg asks for one.
- `.claude/hooks/version_bump.py` refuses a `git commit` whose version has not moved past `HEAD`'s, or whose `Cargo.lock` disagrees. A commit that should not bump (a pure revert, say) puts `[no-bump]` in its message.
- **No `v*` tags.** Pushing one runs `release.yml` and publishes a GitHub release. Releases are Greg's call.
- **Never push without asking Greg.** Commits stay local until he says so.

## Working rules

- **Patches** are `.py` files written with the Write tool and run with `python -I`. Always open files with `newline=''`: text mode on Windows writes CRLF, which breaks diffs and the factory fingerprints.
- After changing any factory file (`assets/visuals`, `assets/presets`, `assets/scenes`), run `cargo test -p audioviz rewrite_factory_history -- --ignored`. After editing a scene file by hand, normalise it first with `rewrite_factory_scene_files`.
- **Measure** windowed 2560x1440, release build. Logs go to stdout with `RUST_LOG=audioviz=debug`, one line per second with `gpu n ms` and `worst n ms`. Report the median of 8 lines over two runs; layouts vary per run, because `V.instance` is random.
- Dev runs use `AUDIOVIZ_DEFAULTS=1`. Never touch `%APPDATA%\audioviz\settings.json`.
- Useful dev variables:
  - `AUDIOVIZ_SETTINGS='{…}'` merges settings fields.
  - `AUDIOVIZ_SCENE=n` starts on factory scene n; it is applied after `AUDIOVIZ_SETTINGS`, so it overrides it.
  - `AUDIOVIZ_SIZE=WxH` sets the window size.
  - `AUDIOVIZ_PANEL=<section>` opens the panel there and skips the welcome sheet.
- **UI:**
  - Labels are in proper case.
  - One object is in focus at a time.
  - Nothing may widen the panel column.
  - Palettes reach everything.
  - Effects are modular and visual-agnostic.
- **Docs stay current** with the code: the format in `docs/visuals.md`, a log entry in the phase doc, and an ADR's status when an increment of it lands.
