# Spec 31 — Forecast history: use the git you already have

**Status:** complete (v0.11.11 backend, v0.11.12 full coverage + console)
**Answers:** *"see which teammate made which change to a shared artifact"*

## 1. What we found before designing anything

Two assumptions were checked. Both were right, with one surprise that
changed the whole plan.

**ABW gives every workspace a real git repo.** True.
`agent-bestiary/ontology/src/workspace_git.rs` is a git2-backed
`WorkspaceGitManager` with `commit_file`, `read_file`, `list_files`,
`get_log`, `diff_commits` and push support, already wired into `AppState`.

**Every forecast is its own workspace.** Aspirationally — 48 of 77 (62%).

The surprise:

| | |
|---|---|
| Forecast workspaces with `git_repo_path` set | **0** |
| Commits across all forecast workspaces | **1** |

**The substrate was built and completely idle.** Forecast versioning was
about to be reimplemented in SQL, next to a finished implementation of it.

## 2. The hole this closes

`update_forecast_handler` wrote a revision row only when the probability
moved:

```rust
if (new_prob - current_prob as f64).abs() > 0.001 { /* INSERT ... */ }
```

and `forecast_spacetime` (including `fpl_snapshot`) is populated by a
**trigger on that insert**.

So **editing the FPL program without moving the number left no trace
whatsoever.** `fpl_source` was a mutable `TEXT` column, silently
overwritten, last-write-wins. Refactor a driver, change a distribution, fix
an assumption — if the mean landed in the same place, nobody could see that
you did it and there was no way back.

Two consequences, both bad: "which teammate changed what" was unanswerable
for program changes, and two editors with `edit` access silently clobbered
each other with no record that either had.

## 3. Version the inputs, not the artifact

The FPL is **generated** from structured state (`composer.rs::generate_fpl`,
`cockpit.rs::generate_fpl_text`). Versioning `fpl_source` as a column would
version a *build artifact* while its inputs stayed unversioned.

So the split is:

* **DB** — current state + the action log (`fermi_forecast_updates`, now
  with `actor_user_id` from Spec 26).
* **Git** — the materialised artifact per action: readable, diffable,
  revertible, authored.

Each commit writes the whole state:

| file | source |
|---|---|
| `forecast.fpl` | `fpl_source` — the diffable artifact |
| `drivers.json` | `drivers` — the real inputs |
| `evidence.json` | `evidence` |
| `agents.json` | `agents_used` |
| `state.json` | probability, CI, status, target date, visibility, tags |
| `README.md` | question, resolution criteria |

JSON is pretty-printed deliberately: a minified blob yields a one-line diff
for every change, which defeats the point of committing it.

## 4. The collaboration model

Ward Cunningham's wiki bet: **reversibility beats prevention.** Shared
write, complete history, trivial revert — no locking, no merge, no review
gates, no optimistic-concurrency machinery. A clobber stops being data loss
and becomes *"a commit; here's the diff; revert it."*

Which reframes the permission question entirely:

> `edit` was only ever frightening because there was no undo.

So no new permission mechanics. The existing ones are already the simplest
model that works:

| | may |
|---|---|
| `view` | read — including the history; provenance is not a privilege |
| `edit` | act — every act committed, attributed, revertible |
| `admin` / `resolve` | terminal, irreversible actions |

The line that matters is **revertible vs terminal**, not
viewer/editor/owner. Everything revertible can be handed out freely once
history exists; only terminal actions need real gating, which Spec 30
already did.

`view` stays for a concrete reason: **published forecasts.** Public and
`shared` visibility grant view to strangers, and that must never become
edit. So: moderate, not radical.

## 5. Substrate gaps closed

`commit_file` was unusable as a collaboration record for two reasons:

1. **No author.** It hardcodes the configured system signature, so every
   commit was by the platform. "Which teammate" is unanswerable no matter
   how much you commit.
2. **No atomicity.** One action changes the program, the drivers and the
   probability. Looping `commit_file` gives three commits for one act,
   making the log unreadable and diffs meaningless.

`commit_files_as` fixes both: a file *set*, one commit, an optional
`CommitAuthor`. It returns `Option<WorkspaceCommit>` — `None` for an
unchanged tree, where `commit_file` synthesises a fake commit reporting the
parent's SHA with a fresh `Utc::now()` timestamp (a commit that never
happened, at a time it didn't).

`read_file_at(slug, path, sha)` is the missing half of revert: `diff_commits`
could already say what changed, but nothing could recover the earlier
content, so undo was unimplementable. Returns `Ok(None)` when the path
didn't exist at that commit — an answer, not an error.

## 6. Endpoints

```
GET  /api/forecasts/:id/history        commit log — view-gated
GET  /api/forecasts/:id/history/:sha   diff vs parent, or ?against=<sha>
POST /api/forecasts/:id/revert         restore an earlier revision — edit-gated
```

`commit_forecast_state` is the single hook every mutating path calls. One
helper on purpose, the same discipline as the ACL predicate: nine writers
already touch `predicted_probability`, and if each had to remember to commit
the history would have holes exactly where the interesting edits are.

### Coverage

v0.11.11 hooked create/update/probability, which left precisely those holes.
v0.11.12 closes them:

| path | why it matters |
|---|---|
| `resolve` / `void` | terminal events belong in the record most of all — the commit captures the exact state the Brier was computed against |
| cascade `apply` / `undo` | the one act that silently rewrites **other people's** numbers: a teammate opens a forecast they own and finds it changed by a propagation they never saw |
| bayesops accept | an operator decision, so attributed to whoever accepted rather than reading as "the system" |
| recompose siblings | the subtle one, on the **hot path**: every probability update rewrites the displayed value of every sibling in a mutex group |

Cascades commit at the **handler boundary**, not inside the propagation
recursion — those helpers take a bare `PgPool`, and threading git through
them would spread the hook across the code most likely to be refactored.

Still uncommitted: the Polymarket auto-resolution sweeper and
`workspace/refit`. Both are background tasks holding only a `PgPool`, so
they need the git manager threaded into their spawn. Documented rather than
half-done.

Best-effort by contract — a git failure must never fail a save. The DB is
truth; the repo is a derived record. Losing a commit costs a line of
history; failing the save costs the operator their work.

Commit messages name what changed (`"Alice Labra: updated drivers,
probability"`, `"Bo: revised 41% → 47% — new elo data"`) rather than
"updated", because a log of forty identical messages is no better than none.

### Revert's two limits

1. **Gated on `edit`, not admin.** Reverting is itself revertible — it
   writes a forward commit, never rewrites history — so it belongs with the
   other reversible powers. Treating undo as more privileged than the edit
   it undoes would be backwards.
2. **Restores the analysis, never the lifecycle.** Probability, drivers,
   evidence and FPL come back; `status`, `resolved_at`, `actual_outcome` and
   `brier_score` do not. Reverting a resolved forecast is **refused
   outright** — mig-174 freezes the scoring tuple precisely so a score can't
   be quietly rewritten, and revert must not be a hole in that.

## 7. Lazy provisioning

38% of forecasts had no workspace, so `ensure_forecast_repo` is lazy and
idempotent: the first versioned action provisions the workspace and repo.
Better than a one-shot backfill, which would drift again the moment
something created a forecast by another path.

It also writes `teams.git_repo_path`, which nothing populated before — the
manager derives paths from the slug, so the column was dead and nothing
could tell whether a repo existed without touching the disk.

## 8. Not built

* **Merge, branches, three-way conflict resolution.** Complete history plus
  cheap revert covers parallel authorship at the scale of a few people on
  small programs. Revisit only after watching real collisions.
* **Optimistic concurrency.** Would have been the right answer *without*
  revert. With it, redundant.
* **Fork.** Only justified for genuine divergence — "I want my own number on
  your question" — and then as a *linked* forecast so it's a relationship,
  not sprawl. An unlinked copy is sprawl; a linked, compared, scored pair is
  signal.
* **Driver-level annotation** — "your base rate is wrong" attached to a
  specific driver. The most likely next thing people actually want.

## 9. Console

**History tab**, next to Access — "who can see this" and "who changed it"
are the same family of question. Commit list with the actor in its own
column so the eye can scan who has been working; unattributed commits render
dim rather than being blamed on a person; selecting one shows its diff
inline, colour-coded by line prefix.

**Revert** is deliberately easy to reach, because the whole model rests on
it: `edit` is only safe to hand out freely if undo is real. Two-step
confirm, and a line stating that reverting writes a new commit rather than
rewriting history — which is what makes people willing to use it.

One non-obvious bug found while building it: reverting has to clear the
composer's `dirty` flag. The in-memory program is the revision the server
just undid, so leaving it dirty lets the autosave loop `PUT` it straight
back and silently cancel the revert within ~15 seconds.

**Read-only composer.** The bug that started this whole thread: a forecast
shared at `view` opened in a fully editable composer, let the operator work,
and failed only at save with a 403 — while the permission had been known and
displayed the entire time. The cockpit now gates Save on
`access_summary.my_permission`, shows a read-only banner naming the owner to
ask, and short-circuits autosave (which would otherwise retry a guaranteed
403 every 5 seconds).

It fails **open**: a missing `access_summary` permits editing. A false
negative would block legitimate work, and the server is the real authority.
