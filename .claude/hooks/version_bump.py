"""PreToolUse hook (Bash): a `git commit` must move the workspace version past
HEAD's, with Cargo.lock agreeing (see CLAUDE.md, "Versions and commits").
Reads the hook's JSON on stdin; exit 2 refuses the call and tells Claude why.
`[no-bump]` in the command lets a commit through unbumped; `--amend` is left
alone (the commit it amends already carries its version)."""

import json
import re
import subprocess
import sys


def version(text):
    m = re.search(r'(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"', text or "")
    return tuple(int(x) for x in m.groups()) if m else None


def lock_version(text):
    m = re.search(r'\[\[package\]\]\s*name = "audioviz"\s*version = "([^"]+)"', text or "")
    return m.group(1) if m else None


def git(*args):
    r = subprocess.run(["git", *args], capture_output=True, text=True, encoding="utf-8")
    return r.stdout if r.returncode == 0 else None


def main():
    import os
    if os.environ.get("CLAUDE_PROJECT_DIR"):
        os.chdir(os.environ["CLAUDE_PROJECT_DIR"])
    try:
        cmd = json.load(sys.stdin).get("tool_input", {}).get("command", "")
    except (ValueError, AttributeError):
        return 0
    if not re.search(r"\bgit\b[^|;&]*\bcommit\b", cmd) or "--amend" in cmd or "[no-bump]" in cmd:
        return 0
    head = version(git("show", "HEAD:Cargo.toml"))
    # The working tree, not the index: `git add ... && git commit` stages it
    # in the same call, after this hook has run.
    try:
        with open("Cargo.toml", encoding="utf-8") as f:
            now = version(f.read())
        with open("Cargo.lock", encoding="utf-8") as f:
            lock = lock_version(f.read())
    except OSError:
        return 0
    if head is None or now is None:
        return 0
    show = lambda v: ".".join(map(str, v))
    if now <= head:
        nxt = (head[0], head[1], head[2] + 1)
        print(f"Commit refused: Cargo.toml is still {show(now)} (HEAD has {show(head)}). Bump the patch version to {show(nxt)} and build so Cargo.lock follows (in a command of its own: this check runs before the command), then stage both, and start the message with \"{show(nxt)}:\". A commit that must not bump puts [no-bump] in its message.", file=sys.stderr)
        return 2
    if lock is not None and lock != show(now):
        print(f"Commit refused: Cargo.lock has audioviz {lock} but Cargo.toml says {show(now)}. Run cargo build so the lock follows, and stage it.", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
