# Security

audioviz runs locally, reads system audio and now-playing metadata, and makes no network connections except the update check you start yourself (see `PRIVACY.md`). The attack surface is small, but reports are welcome.

- **Report privately** by opening a GitHub security advisory on the repository ("Security" tab → "Report a vulnerability"), or email the address on the maintainer's GitHub profile. Please do not open a public issue for anything exploitable.
- **In scope:** anything that lets the app read, write or transmit data it should not; crashes triggered by crafted media metadata or album art; a visual folder (`visual.json` / `visual.wgsl`) or scene file that can do more than draw; installer or updater behaviour that escalates privilege or touches other users' profiles.
- **Dependencies:** `cargo deny check advisories` runs in CI against the RustSec database on every push. If a vulnerable dependency is reported there, it will be bumped in the next release.
- **Releases:** built from tagged commits by the public workflow in `.github/workflows/release.yml`; checksums are published with every release. Binaries are unsigned for now (see `PRIVACY.md`).
