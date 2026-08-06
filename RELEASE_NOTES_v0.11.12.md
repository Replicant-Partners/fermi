# v0.11.12 — Completing the pattern: history everywhere, and an editor that knows what you can do

v0.11.11 put forecast history on the git substrate. This finishes it: every
mutating path now commits, and the composer stops letting you edit things
you can't save.

## History had holes exactly where the interesting edits are

v0.11.11 hooked create, update and probability. That's worse than it sounds
— a history with gaps *looks* complete while omitting the changes people
most need to see. Now hooked:

| path | why it matters |
|---|---|
| **resolve / void** | terminal events belong in the record most of all — they're the ones nobody can undo. The commit captures the exact state the Brier was computed against, which is what auditing a score needs. |
| **cascade apply / undo** | the highest-value hook. A cascade is the one act that silently rewrites **other people's** numbers — a teammate opens a forecast they own and finds its probability changed by a propagation they never saw. |
| **bayesops accept** | an operator decision, attributed to whoever accepted it rather than reading as "the system". |
| **recompose siblings** | the subtle one, and it's on the **hot path**. Every probability update rewrites the displayed value of every sibling in a mutex group. Those forecasts belong to other people and their numbers move without them touching anything — the same silent-change problem as a cascade, but constant. |

Cascades commit at the handler boundary rather than inside the propagation
recursion: those helpers take a bare `PgPool`, and threading git through them
would spread the hook across the code most likely to be refactored.

Three call-site conveniences (`commit_for` / `commit_system` /
`commit_cascade`) because a hook that isn't trivial to call is a hook writers
skip.

**Still uncommitted, stated rather than hidden:** the Polymarket
auto-resolution sweeper and `workspace/refit`. Both are background tasks
holding only a `PgPool`, so they need the git manager threaded into their
spawn — a separate change.

## The History tab

Next to Access, because "who can see this" and "who changed it" are the same
family of question.

```
● Alice Labra    revised 41% → 47% — new elo data     2h
│ 3f8a91c
● Bo             updated drivers, probability          1d
● —              cascade from 9f1adf4c · 6 adjusted    3d
```

Actor in its own column so you can scan who's been working. Unattributed
commits render dim rather than being blamed on a person — the server refuses
to guess an actor, and the UI refuses to hide that. Selecting a commit shows
its diff inline, coloured by line prefix.

**Revert** is deliberately easy to reach, because the entire model rests on
it: `edit` is only safe to hand out freely if undo is real. Two-step confirm,
and a line stating that reverting writes a new commit rather than rewriting
history — which is what makes people willing to use it.

One non-obvious bug found while building it: **revert has to clear the
composer's dirty flag.** The in-memory program is the revision the server
just undid, so leaving it dirty lets autosave `PUT` it straight back and
silently cancel the revert within ~15 seconds.

## The editor knows what you can do

The bug that started this thread:

```
✗ save — Saved locally, but backend save failed: Forbidden: Edit access denied
```

A forecast shared with a team at **`view`** opened in a fully editable
composer, let the operator do real work, and failed only at save — while the
permission had been known, fetched, and *displayed in the row* the entire
time.

Now: Save is gated on `access_summary.my_permission`, a read-only banner
names the owner to ask, and autosave short-circuits (it would otherwise retry
a guaranteed 403 every 5 seconds and bury the composer in warnings).

It fails **open** — a missing `access_summary` permits editing. A false
negative would block legitimate work, and the server is the real authority.

## Why this shape

Ward Cunningham's wiki bet: **reversibility beats prevention.** Shared write,
complete history, trivial revert. No locking, no merge, no review gates, no
optimistic concurrency — all of that was compensating for the absence of
undo.

Which is why `view`/`edit`/`admin` didn't need changing. The line that
matters is **revertible vs terminal**: everything revertible can be handed
out freely once history exists, and only terminal actions need real gating,
which v0.11.10 already did.

`view` stays load-bearing for published forecasts — public visibility grants
read to strangers, and that must never become edit.

## Validation

* 8 integration tests against **real git repos** in temp dirs, pinning the
  three properties the model rests on: commits carry the acting human, one
  action is one commit across several files, earlier content is recoverable.
* The sibling-recompose query planned read-only against production.
* `cargo check --workspace` clean; 58 collaboration tests green.

## Next

* **Driver-level annotation** — "your base rate is wrong" attached to a
  specific driver. The most likely thing teams actually want next, since it's
  assumptions they argue about rather than programs.
* **One-click grant edit** on a share row — now safe, which was the premise.
* Thread git into the two background writers.

Design: `docs/specs/SPEC_31_FORECAST_HISTORY.md`.
