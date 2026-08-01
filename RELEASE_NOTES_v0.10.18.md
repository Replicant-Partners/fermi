# v0.10.18 — the updater picks the newest *version*, not the newest *publish*

## Why

v0.10.17 shipped the Activity panel. Nobody got it.

v0.10.15, v0.10.16 and v0.10.17 were tagged together, which started
three concurrent release builds. v0.10.17 had the smallest diff, so its
build finished first:

| tag | published |
|---|---|
| v0.10.17 | 13:44:29 |
| v0.10.15 | 13:45:00 |
| **v0.10.16** | **13:45:53** ← finished last |

The updater called `GET /releases/latest`, and **that endpoint returns
the most recently *published* release, not the highest version.** So it
returned v0.10.16, every client installed v0.10.16, and the console
cheerfully reported `v0.10.16 — up to date` while a newer release sat
in plain sight on the releases page.

The publish order is a race between CI build times. It is not something
the release process can reliably control, and it is not something
clients should depend on.

This failure mode had two other live variants:

- **Back-ports.** Publish a 0.9.x hotfix after 0.10.x has shipped and
  every 0.10.x client is told to *downgrade*.
- **Re-runs.** Re-running an older release workflow moves the pointer
  backwards.

## Changes

### 1. `check_latest` lists releases and chooses by version

`crates/fermi-console/src/updater.rs`

`GET /releases/latest` → `GET /releases?per_page=30`, then
`pick_best_release` selects the maximum by parsed semver rather than
trusting list position. The list arrives ordered by publish time, which
is exactly the ordering we can't rely on, so ordering is discarded.

`pick_best_release` is pure and total, and skips:

- drafts and pre-releases;
- **releases missing the platform binary asset.** A partially-failed
  workflow publishes the release before uploading assets. Previously
  that produced a hard error and blocked updating entirely; now the
  updater falls through to the newest release it can actually install.

### 2. The console gets a `lib` target so this code is testable

`crates/fermi-console/src/lib.rs` (new), `Cargo.toml`

`updater.rs` had a `#[cfg(test)] mod tests` with four tests in it. They
had **never run**, because `cargo test` on the bin target segfaults
rustc while expanding GPUI's element chains — the same pre-existing
condition that forced `#![recursion_limit = "4096"]` in `main.rs`:

```
$ cargo test -p fermi-console --bin fermi-console
note: rustc unexpectedly overflowed its stack!
```

`updater.rs` has no GPUI dependency, so it now lives in a `lib` target
alongside a documented rule about what may join it. `main.rs` consumes
it as `fermi_console::updater` — one definition, no duplicate build.

```
$ cargo test -p fermi-console --lib
running 13 tests ... ok    (3 seconds)
```

Nine new tests, including one that reproduces the v0.10.15–17 incident
exactly (releases supplied in publish order, asserting v0.10.17 wins),
plus downgrade refusal, back-port masking, asset-less releases, drafts,
pre-releases, empty lists, and a guard against returning one release's
notes attached to another's version.

Two modules are queued to migrate here as they decouple from GPUI:
`chat.rs`'s action-marker parser and `cockpit.rs`'s Anthropic error
extractor, both of which have tests in the same never-run state.

## Upgrade notes

If you are on **v0.10.16**, you never received the v0.10.17 Activity
panel. The `latest` flag on GitHub has been corrected by hand, so
**Help → Check for Updates…** will now offer v0.10.18, which contains
both the Activity panel and this fix.

After this release, tagging several versions at once is safe — clients
resolve by version regardless of which build finishes first.

## Validation

- `cargo test -p fermi-console --lib` — 13/13 passing.
- `cargo check --workspace` — 0 errors.
