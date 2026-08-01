# v0.10.17 — Activity panel: the console gets a debuggable event log

## Why

The console had two notification surfaces, and both lost information.

**The top strip** (`cockpit::render_fermi_banner`) looked like a running
log but wasn't one. It rendered a status line plus *the two most recent
non-`Info` messages*, each truncated at 120–150 characters. So:

- every `Info` message was invisible;
- anything older than the last two entries was gone;
- long errors were cut mid-sentence.

Worse, the save path deliberately leaves `dirty` set so autosave
retries — which meant one persistently-failing save produced the same
chopped sentence three times and filled the entire strip, pushing
everything else out. That is exactly the screenshot that prompted this
release.

**Toasts** (`FermiConsole::show_toast`, 23 call sites) were worse: a
bottom-right pill that auto-dismissed after 3 seconds with **no
history at all**. Auth failures, RBAC provisioning errors, team
operations, updater results — everything that happens outside a
composer — went here. The failures hardest to debug were the ones that
vanished fastest.

And the console was already computing genuinely good diagnostics that
nobody could read:

- `friendly_backend_save_error` has a six-branch error taxonomy with
  specific remediation prose for each.
- On a foreign-key-shaped save failure, the save path makes a **live
  `GET /api/rbac/self-check` round-trip** (added v0.10.6) to classify
  the drift as stale-JWT vs stale-deploy vs missing-users-row.

Both were concatenated into one `String` and handed to the banner. The
remediation half — and the entire server diagnosis — always landed
past the truncation point. **That diagnosis has never been visible in
the UI since it shipped.**

## Changes

### 1. New crate: `fermi-activity`

`crates/fermi-activity/` — the event model, GPUI-free.

- `LogEvent` separates a short, stable `summary` (what the collapsed
  row shows, and what coalescing keys on) from `detail` prose,
  `context` key/values, a raw JSON `payload`, and a machine-readable
  `Remedy`. Nothing long is forced through the summary.
- `ActivityLog` is a bounded ring buffer (`MAX_EVENTS = 500`) that
  **coalesces**: identical `(source, severity, summary)` within 90
  seconds collapses to one row with an `xN` counter and a last-seen
  time. Scans 12 events back, so interleaved market ticks don't defeat
  it. The three-identical-warnings case is now one row reading `x3`.
- Eviction removes the dropped `seq` from the expanded set, so the
  expansion state can't grow without bound or leak onto a recycled row.

Split into its own crate for a concrete reason: `fermi-console` is a
bin crate whose GPUI element chains exhaust rustc's stack during macro
expansion under `--test` (hence its `#![recursion_limit = "4096"]`).
Unit tests placed there **cannot run**. In `fermi-activity` they
compile and run in 8 seconds. **18 tests**, covering coalescing
(including the non-coalescing cases: distinct sources, distinct
severities, distinct agent ids), ring-buffer eviction, filter
behaviour, badge counting, and plain-text export.

### 2. The Fermi panel is now tabbed: `Chat | Activity`

`crates/fermi-console/src/main.rs`, `chat.rs`

- `chat::FermiPanelTab` + a tab strip styled to match the cockpit's
  existing right-hand `render_tab_bar`.
- `Ctrl+'` opens the panel directly on Activity. `Ctrl+;` still
  toggles it.
- The panel widens 380px → 460px on Activity; rows carry a timestamp,
  a source chip and an untruncated summary, which 380px squeezes into
  ribbons.
- Rows are newest-first, collapse to one scannable line, and expand
  into detail + a context table + pretty-printed payload (capped at
  2000 chars inline; Copy always exports everything).
- Filter chips (`All` / `Problems`), `⧉ Copy all` for bug reports, and
  `Clear`.

### 3. Everything feeds one log

- **Cockpit messages** mirror in via a watermark over the existing
  `messages` vector, drained in the parent's `cx.observe` handler.
  **None of the 92 `messages.push(AssistantMessage { .. })` call sites
  in `cockpit.rs` changed.** The watermark self-heals if `messages` is
  cleared.
- **All 23 `show_toast` calls** now also append. Toasts still behave
  exactly as before; they just stop being the *entire* lifetime of an
  event.
- **Chat failures**, which previously existed only in the transcript.
- `CockpitState::push_rich` lets a call site emit a fully-structured
  event; `activity_suppressed` stops the generic mirror from
  double-reporting it.

### 4. The diagnostics become actionable

`classify_backend_save_error` replaces `friendly_backend_save_error`,
returning `{summary, detail, remedy}`. All six branches keep their
analysis but split it properly — and the advice each branch was
narrating in prose is now a **clickable `Remedy`**: `SignOut`,
`RunSelfCheck`, `ResetComposer`, `Retry`, `CopyDiagnostics`.

The `/api/rbac/self-check` response now lands as `detail` + structured
`context` + raw `payload` on an expandable row, and is reachable on
demand rather than only as a side effect of a save failure.

`ApiError` gained `status()`, `kind()` and `is_transient()`
(`api/client.rs`). The save path no longer flattens errors with
`map_err(|e| e.to_string())`, so HTTP status survives to the panel as a
discrete field instead of substring-matchable prose. Failures also
report *which* endpoint was attempted (`POST /api/forecasts` vs
`PUT /api/forecasts/:id`) — previously the error branch knew neither.

### 5. The top strip becomes a status strip

Three lines → one: current state plus a `⚠ N · Activity ↗` chip.
Clicking anywhere on it opens the Activity tab. It's a glance-target
now, not the only place the text exists.

The sidebar Fermi chip gains a red unseen-problem badge — the passive
signal that replaces "a toast flashed while you were reading something
else."

`render_assistant_panel` is deleted. It was a stub returning an empty
`div()`, commented "Legacy — kept for compatibility but Fermi banner is
now the primary display" — a vestige of an earlier attempt at exactly
this feature.

### 6. `🔮 Ask Fermi` on any event

One click drops the full structured event into the Chat tab as a
pre-filled question. This is why Chat and Activity share a panel rather
than being two surfaces: the natural next move after reading a failure
is to ask about it, and that shouldn't mean re-typing an error.

## Upgrade notes

No migrations, no API changes, no config. The log is RAM-only and
per-session — it does not persist across restarts.

Two judgement calls worth your eye:

- Cockpit `Suggestion` / `Tip` messages map to `Trace` severity so
  guidance doesn't drown real events. They're visible under `All`, not
  under `Problems`.
- The banner's problem count is scoped to the open composer; the
  panel's count is app-wide. They will legitimately differ.

## Validation

- `cargo test -p fermi-activity` — 18/18 passing.
- `cargo check -p fermi-console` — 0 errors.
- `cargo check -p fermi` — 0 errors.

The console's own test harness still cannot build (pre-existing rustc
stack exhaustion, reproduced on `main` before these changes); this
release is the first step toward not needing it.
