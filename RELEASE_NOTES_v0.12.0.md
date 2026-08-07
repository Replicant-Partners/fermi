# v0.12.0 — The console runs on macOS

Every release before this one built exactly one binary: Linux x86_64. That
was never a decision, it was the shape the first release workflow happened
to have, and it hardened into a platform constraint. Testers on Macs were
told to install Rust and build from source — for a *desktop app whose whole
pitch is that it isn't a web page*.

This release ships the console for Apple Silicon and Intel Macs from the
same tag as Linux, through the same installer, with the same in-app updater.

## What you get

```
fermi-console-linux-x86_64                   plain ELF binary
fermi-console-macos-aarch64                  plain Mach-O, Apple Silicon
fermi-console-macos-x86_64                   plain Mach-O, Intel
fermi-console-v0.12.0-linux-x86_64.tar.gz    archive + README
fermi-console-v0.12.0-macos-aarch64.zip      "Fermi Console.app"
fermi-console-v0.12.0-macos-x86_64.zip       "Fermi Console.app"
checksums-<platform>.txt                     SHA256 of the pair
```

The one-liner is unchanged and now detects your platform:

```bash
curl -fsSL https://raw.githubusercontent.com/Replicant-Partners/fermi/main/scripts/install-fermi-console.sh | bash
```

Or, if you want a Dock icon, download the `.zip` for your chip and drag
**Fermi Console.app** to `/Applications`.

## One writer, many uploaders

The release workflow is now a `create-release` job followed by a build
matrix, rather than three symmetrical jobs. That ordering is the whole
design, not tidiness.

Three concurrent jobs each asking a create-or-update action for the same tag
race on the **release body**: last writer wins, and a macOS job that knows
nothing about `RELEASE_NOTES_<tag>.md` would clobber the notes the Linux job
wrote. The in-app updater renders that body verbatim as its "what's new"
modal, so losing it is user-visible — the update lands and the tester is
told nothing about what changed.

So one job composes the release, and the matrix uses `gh release upload
--clobber`, which touches assets and nothing else. `--clobber` also makes a
re-run idempotent, which is what you want when one matrix leg failed and
you're retrying just that leg.

`fail-fast: false`, for the same reason: a macOS toolchain hiccup must not
cost us the Linux binary that most testers install.

## Ad-hoc signing, and why the updater re-signs

macOS builds are `codesign --sign -` (ad-hoc), not Apple-notarized. A
**browser**-downloaded `.app` therefore needs one right-click → **Open** on
first launch. Binaries fetched by the installer or the in-app updater never
get the quarantine attribute in the first place, so that dance is a
first-run-only, download-from-GitHub-only cost.

The subtler problem is self-update. On Apple Silicon an invalidated
signature isn't a warning — the kernel refuses to `exec` the binary. The
asset we download is already ad-hoc signed, so replacing a *bare binary* is
fine. Replacing the executable **inside a `.app`** is not: the bundle's
`_CodeSignature` seal covers the file we just swapped, so the bundle now
fails validation as a unit and the app dies with SIGKILL and no useful
message. The updater re-seals the whole bundle after installing. Best-effort
by design — if `codesign` is missing we log and continue, because the
binary's own signature is intact either way and a warning beats aborting an
otherwise-successful install.

Relaunch changed too. Spawning `Contents/MacOS/fermi-console` directly
produces a process LaunchServices has no bundle identity for: no Dock icon,
no app menu, and `activate()` can't bring it to the front. When we're
running from a bundle we hand the relaunch to `open -n`, so you get back the
application you had a moment ago.

## ⌘, not Ctrl

Every binding in the console uses GPUI's `secondary-` modifier, which
already resolved to Command on macOS. **Every label describing those
bindings was the hardcoded string `Ctrl+`.** So the app was correctly bound
and comprehensively mislabelled — the sidebar, the menu bar, the shortcuts
panel, and around forty toasts and empty states all told a Mac user to press
a key that does nothing.

Labels now render through one helper and read `⌘R` on macOS, `Ctrl+R`
elsewhere. Two details that are deliberate: `⌘` carries no `+` (`⌘+R` reads
as a typo to anyone who uses a Mac), and **Toggle Fullscreen still says
Ctrl+Shift+F on macOS** — it's bound as a literal `ctrl-shift-f`, not a
`secondary-` chord, so Ctrl is the honest label there.

The shortcuts panel's old footnote — *"Ctrl maps to ⌘ on macOS"* — is gone.
It was the workaround; the rows now do the translation themselves.

## Finding the agents from a double-clicked app

A `.app` launched from Finder starts with its working directory set to `/`.
Every CWD-relative path the console used to look for `agents/curated` missed,
and the only anchor that survives is the executable's own location. The
exe-relative search now covers the bundle layout (`../../../`, since
`package-console.sh` stages agents *beside* the `.app`, not inside it), and
the "couldn't find agents" warning prints both the CWD-relative and
exe-relative candidates instead of one of them.

## A closed allowlist on `/fermi-console/download`

The download redirect takes a `?platform=` slug and interpolates it into an
outbound URL it then 302s to. Unvalidated, that's an open-redirect
primitive. It's an allowlist of the three published slugs, returning the
`&'static str` rather than the caller's string, so nothing
attacker-controlled reaches the URL.

An unknown slug is a **400, not a silent fallback**. Someone asking for
`macos-arm64` — the plausible near-miss for `macos-aarch64` — wants to be
told they guessed wrong, not handed a Linux ELF that dies on exec with no
diagnostic they could act on.

An **omitted** slug still means `linux-x86_64`. Every console built before
this release calls the endpoint with no platform at all, and there's no way
to reach back and upgrade them; that default is what keeps the installed
base updating.

## Making the drift impossible to repeat

CI compiled for Linux only, which is precisely why the console *stayed*
Linux-only. A new `console-macos` job runs `cargo check -p fermi-console` on
a macOS runner every push, so a `cfg`ed branch that doesn't compile fails a
PR instead of surfacing at tag time — where the Linux binary would publish
and the macOS one silently wouldn't.

`cargo test --bin api-server` was also added: `handlers` is a module of the
*binary*, not the lib, so `cargo test --lib --workspace` could never see it.
The download allowlist above would have been tested only in principle.

The macOS bundle-path logic is written as pure path arithmetic with no
filesystem access, specifically so it's unit-testable on any host — the
behaviour only *runs* on macOS, and near-miss layouts (`.appx`, a missing
`Contents/`) are asserted to be rejected rather than `codesign`ed by
accident.

## Also in this release

**Read-only forecasts open read-only.** `can_edit_forecast` fails *open* on
a missing access summary, which is the right default for a false negative
but meant the gate had no data at the one moment it mattered. The summary
was only ever fetched by the Access / Assumptions / History tabs, which most
operators never open — so a view-shared forecast opened fully editable, the
operator did real work, and the first autosave 403'd. Both the workspace and
Portfolio drill-in paths now pre-warm it.

**`wc_arg_scenario` compiles again.** It `include_str!`'d a hardcoded
`/tmp/arg_scenario.fpl` that exists on no machine, which broke the whole
`--tests` graph and stopped clippy dead before it linted anything. It now
reads the tracked Argentina template and asserts the *current* Option 2
contract: the model expression is the forecast quantity. The old assertion
multiplied by `base_rate` a second time — the template already carries it —
and read 2.09% where the model says 7.7%. A new neutral-team assertion (all
drivers at 1.0 must return the uniform prior) makes that specific
double-count fail loudly rather than drift.

## Not in this release

**Notarization.** Needs a paid Apple Developer ID; ad-hoc signing costs one
right-click on first browser download and nothing thereafter.

**Windows.** Would additionally need `self_replace` — you can't `rename`
over a running `.exe`. The updater is inert on unsupported targets rather
than offering an update it has nothing to download for.

**Linux aarch64.** No demand yet; the installer names it explicitly in its
"build from source" message rather than failing cryptically.
