# Release Notes Template

The Fermi Console release workflow (`.github/workflows/release-console.yml`)
looks for a file named `RELEASE_NOTES_<tag>.md` at the repo root when
you push a tag (e.g. `RELEASE_NOTES_v0.8.0.md` for tag `v0.8.0`).

**The in-app updater renders these notes verbatim.** They are the
first thing every remote tester sees when a new build ships, so write
them for that audience — not as a git log dump.

## Workflow

1. Cut your work as usual on `main`.
2. Copy this template to `RELEASE_NOTES_v<X.Y.Z>.md`, fill it in.
3. Bump the version in `crates/fermi-console/Cargo.toml`.
4. Commit both together.
5. Tag: `git tag v<X.Y.Z> && git push origin v<X.Y.Z>`.
6. The release workflow builds Linux binary, uploads to a GitHub Release
   with these notes as the body, and clients pick it up on next launch.

## Template

Copy everything below the `---` into `RELEASE_NOTES_v<X.Y.Z>.md`.

---

## What's new

- **Feature name** — one-liner describing user-visible value. Keep it
  concrete: "Portfolio detail now shows recent-activity sort" not
  "improved sorting infrastructure".
- **Another feature** — …

## Fixes

- Fixed X causing Y (mention triggering symptoms so testers can
  confirm their issue was the one addressed).

## Known issues

- Things not yet fixed but that testers should know about, so they
  don't file duplicate reports.

## Breaking changes

Only if applicable. Prefer "no breaking changes" as the default.

## Upgrade notes

Anything users must do beyond clicking Update & Restart, e.g.
"clear `~/.fermi/cache/` if you see stale agent cards".
