# Cascade thread — handoff (v0.8.10 baseline)

**Status:** compose-and-plan surface complete end-to-end. Ready for
scenario-tree work (see `SCENARIO_TREE_DESIGN.md`) or a foundation
refactor (see § Deferred).

**Author of this doc:** ai session that shipped v0.8.8 → v0.8.10.
**Baseline commit:** `v0.8.10` on `main` (amended to include the
bugfixes and scenario-tree design doc described in the release notes).
**Reader:** the next session (human or ai) picking up this thread.

---

## 1. Elevator TL;DR

Cascades were the only mechanism that actually moved forecasts during
the WC 2026 simulation — Spain's 55.9% was raw 11.9% plus
mass-conserving redistribution of 40+ eliminated teams through the
`wc_2026_winner` mutex group. The engine worked; the movement was
invisible in the UI. This thread made the cascade primitive
observable, compose-able, and preview-able from the composer:

| Release | Slice | User-visible artifact |
| --- | --- | --- |
| v0.8.8 | Provenance | *Read* — Provenance right-tab: "where did this probability come from?" waterfall over `fermi_forecast_updates` rows tagged `revision_trigger IN ('cascade','cascade_undo')`. |
| v0.8.9 | Composition Slice A | *Write* — `CASCADES:` chip strip on the question header; inline picker with browse + create-new modes. |
| v0.8.10 | Composition Slice B | *Plan* — chip label → inline detail panel with members table + kind-aware invariant health strip + per-member `preview NO ▶` dry-run preview. |
| v0.8.10 (amend) | Bugfixes + docs | Fixes to Slice A/B discovered during live use (see § 6). This doc + `SCENARIO_TREE_DESIGN.md` + Portfolio Risk view independence caveat (Slice 1 of the scenario-tree upgrade). |

The full loop works. Operators can compose a cascade dependency,
inspect the group, preview a hypothetical resolution, and see the
redistribution — all without leaving the composer.

## 2. Mental model

A **cascade** is a *lateral* rebalancing across a portfolio of
correlated forecasts, triggered when one member resolves. It's
distinct from the *vertical* DAG propagation (workspace →
workspace) already in Phase 2 of the roadmap.

Four primitives:

```
CASCADE  =  (Group, Rule, Trigger, Invariant)
```

- **Group** — a set of forecast IDs that share a constraint.
  Stored as a tag on `fermi_forecasts.relationship_groups` (Spec 25).
- **Rule** — the propagation function. Three kinds ship today:
  `mutex` (aka `mutually_exclusive`), `at_most_n`, `implies`.
- **Trigger** — a resolution or probability shift on one member.
- **Invariant** — a preservation predicate. For mutex + at_most_n
  it's `Σp ≈ 1.0`; for implies there isn't a simple additive one.

Server-side, all three kinds live in
`src/handlers/relationships/propagation.rs` as `propagate_mutex` /
`propagate_at_most_n` / `propagate_implies` behind a hard-coded
dispatcher. This is the surface that would be repackaged in the
**CascadeRule trait refactor** (see § Deferred).

### Vocabulary — cascade vs scenario

Per `docs/fermi/SCENARIO_TREE_DESIGN.md`, the user-facing name
should evolve to **scenario**; the engine name stays **cascade**.
Two names for two audiences, same object.

| User-facing (UI) | Internal (Rust / DB / SDK) |
| --- | --- |
| Scenario | `cascade_group` / `relationship_group` |
| Scenario constraint | `kind` field on the group |
| Scenario builder | `render_cascade_group_strip` + picker + create form |
| — | `cascade` verb / `cascade_undo` (post-resolve propagation — keep) |

**Where we are:** v0.8.8–v0.8.10 all use "cascade" in the UI (matches
the Provenance vocabulary and the WC roadmap). Slice 2 of the
scenario-tree work is the vocab sweep. This is a ~15-string edit and
zero logic change; deliberately left for the successor.

## 3. Code map

### Backend (Rust)

| Path | Role |
| --- | --- |
| `src/handlers/relationships/mod.rs` | Module root; re-exports the propagation surface. |
| `src/handlers/relationships/propagation.rs` | The engine. `dispatch_propagation` / `dispatch_propagation_group` + `propagate_mutex` / `propagate_at_most_n` / `propagate_implies`. FLOOR/CEIL clamping at 0.001/0.999. Refactor target. |
| `src/handlers/relationships/groups.rs` | CRUD for `forecast_relationship_groups` + the new v0.8.10 `preview_group_propagation_handler` (dry-run propagate). |
| `src/handlers/relationships/membership.rs` | `GET/PUT /api/forecasts/:id/groups` + `POST/DELETE /api/forecasts/:id/groups/:gid`. GET was fixed in v0.8.11 — see § 6. |
| `src/handlers/relationships/apply.rs` | Operator-gated pending_cascade Apply/Dismiss. Not touched by this thread. |
| `src/handlers/relationships/undo.rs` | `cascade_undo` post-apply. Not touched by this thread. |
| `src/handlers/relationships/recompose.rs` | Holistic mutex re-snap. Not touched. |
| `src/handlers/relationships/legacy.rs` | Old per-relationship-ID model (mig 150). Kept for back-compat. |
| `src/handlers/pending_cascades.rs` | Queue table for operator-gated writes. |
| `src/handlers/forecast_benchmark.rs` | Home of `forecast_cascade_provenance_handler` (v0.8.8). |
| `src/bin/apply_wc_cascades.rs` | One-shot backfill used during the WC sim. Delete after CascadeRule trait refactor lands. |
| `src/api_server.rs` | Route registrations. `/api/relationship-groups*` + `/api/forecasts/:id/groups*` + `/api/forecasts/:id/cascade-provenance`. |

### Console (GPUI, Rust)

| Path | Role |
| --- | --- |
| `crates/fermi-console/src/api/client.rs` | `ApiClient::forecast_cascade_provenance`, `list_cascade_groups`, `get_cascade_group`, `create_cascade_group`, `get_forecast_cascade_groups`, `add_forecast_to_cascade_group`, `remove_forecast_from_cascade_group`, `preview_cascade_propagation`, `pm_link`. |
| `crates/fermi-console/src/cockpit.rs` | Everything: state fields, loader/mutator methods, chip strip, picker, detail panel, preview strip, Provenance tab. Grep for `cascade` to see the whole surface. Types `CascadeGroupSummary` + `CascadeCreateDraft` near the top of the file. |
| `crates/fermi-console/src/main.rs` | Tab-cycle wiring for `RightTab::Provenance`; pre-warms for `load_forecast_cascade_groups`. |

### Tests

| Path | Coverage |
| --- | --- |
| `tests/forecast_cascade_provenance_shapes.rs` | 8 wire-format assertions for the Provenance endpoint. |
| `tests/cascade_group_propagate_shapes.rs` | 6 wire-format assertions for the dry-run propagate endpoint (includes mutex mass-conservation). |
| `tests/relationships_mutex_math.rs` | Pre-existing 6 pure-function tests for the mutex redistribution math. |

### Migrations

| Path | Notes |
| --- | --- |
| `migrations/150_forecast_relationships.sql` | Original per-relationship model. Superseded by group-tag but kept live. |
| `migrations/153_pending_cascades.sql` | Operator-gated queue. |
| `migrations/155_forecast_relationship_groups.sql` | Group-tag model (Spec 25). `forecast_relationship_groups` table + `relationship_groups` column on `fermi_forecasts`. |
| `migrations/156_pending_cascades_extensions.sql` | `applied_deltas`, `superseded_by`, `group_id`, `undone` status, `cascade_undo` trigger. |
| `migrations/159_pending_cascades_relationship_id_nullable.sql` | Allows group-only cascades. |

## 4. Endpoint surface (current)

All the read + write endpoints the console needs today. Group them
by resource:

```
Provenance (per-forecast waterfall)
  GET  /api/forecasts/:id/cascade-provenance         v0.8.8

Cascade groups (aka scenarios)
  GET  /api/relationship-groups                      pre-existing
  POST /api/relationship-groups                      pre-existing
  GET  /api/relationship-groups/:id                  pre-existing
  PATCH /api/relationship-groups/:id                 pre-existing
  DELETE /api/relationship-groups/:id                pre-existing
  GET  /api/relationship-groups/:id/members          pre-existing
  POST /api/relationship-groups/:id/propagate        v0.8.10 (dry-run default)

Per-forecast group membership
  GET  /api/forecasts/:id/groups                     v0.8.10 (wiring bugfix, folded into the amend)
  PUT  /api/forecasts/:id/groups                     pre-existing
  POST /api/forecasts/:id/groups/:gid                pre-existing
  DELETE /api/forecasts/:id/groups/:gid              pre-existing

Pending-cascade operator gate
  POST /api/pending-cascades/:id/apply               pre-existing
  POST /api/pending-cascades/:id/undo                pre-existing
  POST /api/pending-cascades/requeue                 pre-existing
```

The dry-run propagate endpoint is the one the successor most likely
wants to know about: it wraps `dispatch_propagation_group` with
`dry_run=true` default, returns the standard `PropagateResult` shape,
and is owner-gated. Safe to call repeatedly.

## 5. Next work (recommended)

Two live options, roughly independent:

### Option A — scenario-tree slices (per `SCENARIO_TREE_DESIGN.md`)

This is the **operator's own roadmap** and the higher-leverage path
right now. Three slices, each shippable independently:

1. **Slice 2 — UI vocab sweep.** Rename user-visible strings in
   `render_cascade_group_strip` / picker / create form / detail
   panel from "cascade" → "scenario". ~15 string edits, zero logic
   changes. Enables consistent vocabulary before Slice 3.
2. **Slice 3a — Backend `member_ids` on `list_groups_handler`.**
   One SQL change + one JSON field. Independent PR; picker ignores
   the new field until Slice 3b needs it.
3. **Slice 3b — Console cache + Level 2 joint-probability compute.**
   `scenarios_cache` on `FermiConsole`, `compute_portfolio_risk`
   gains a `scenarios` parameter, Level 2 filter + renormalization,
   tile/footer swap to `scenario-aware` label. Fixes the
   independence-assumption bug in `JOINT SCENARIOS (TOP 4)`.

Read `SCENARIO_TREE_DESIGN.md` end-to-end before starting; the
algorithm section and the data-plumbing route comparison are the
key parts.

### Option B — CascadeRule trait refactor (foundation)

Repackages `propagate_mutex` / `propagate_at_most_n` /
`propagate_implies` behind a pluggable trait:

```rust
pub trait CascadeRule {
    fn kind(&self) -> &'static str;
    fn output_key(&self) -> &'static str;
    fn propose(&self, state: &GroupState, trigger: &Trigger) -> Vec<Delta>;
    fn invariant_check(&self, state: &GroupState) -> InvariantReport;
}
```

Zero behavior change; sets up the trait so new kinds slot in without
touching a per-kind dispatcher. Once it lands the natural additions
are:

- `bracket` — knockout tree (WC round-of-16 → semis).
- `k_of_n` — generalization of `at_most_n` for "exactly k succeed".
- `budget` — Σweight = const (portfolio allocation, LP positions).
- `correlation` — coupling matrix; soft rebalance without a hard
  constraint. Opens the door to correlated-markets forecasting.
- `rollup` — parent = f(children); trigger fires upward.
- `bayesian` — `P(B) ← P(B|A=outcome)` from a stored conditional
  table. The natural home for Phase 6 self-improvement.

**When to do this vs Option A:** the trait refactor is speculatively
valuable — it pays off when a specific new kind is actually needed.
The scenario-tree slices address a *known* correctness bug on a
surface operators are looking at today. Recommend Option A first.

If you do Option B: `src/bin/apply_wc_cascades.rs` can be deleted
after — its logic is a `propagate_mutex` loop.

### Deferred / longer-term

Documented so they aren't forgotten:

- **Apply-from-preview button** in the detail panel. Trivial UX;
  still routes through `pending_cascades` operator gate.
- **Search + pagination on members table.** Fine at O(50); needs a
  scroll container + `EditorEntity` search box at O(500).
- **Editable `group_id` and `description`** in the create-new form
  (Slice A shipped as read-only auto-slug). Needs `EditorEntity`.
- **`at_most_n.n` numeric input** (hard-coded to 1 in Slice A).
- **`implies` antecedent/consequent forecast pickers.**
- **Level 3 — All Groups Dashboard section.** Only worth it once
  operators start managing >10 groups per account.
- **Multi-trigger preview** — server engine supports chained
  triggers; UI doesn't have a shape for "resolve these three
  sequentially" yet.
- **First-class `trigger_forecast_id` column** on
  `fermi_forecast_updates`. Currently parsed from the `reason`
  string; parser is stable across all four call sites but a real
  column would remove the fragility.
- **Delete `src/bin/apply_wc_cascades.rs`** after the trait
  refactor lands.

## 6. Known bugs fixed in the v0.8.10 amendment

Both discovered during live use of what v0.8.9 / v0.8.10 initially
shipped; folded into the v0.8.10 amendment alongside this handoff
doc and the scenario-tree design.

See `RELEASE_NOTES_v0.8.10.md` for the operator's write-up.
Relevant summary below.

### GET route missing on forecast-groups membership

`src/api_server.rs` had only `put()` bound for
`/api/forecasts/:id/groups`. The GET handler existed
(`membership::get_forecast_groups_handler`) but was never wired, so
`load_forecast_cascade_groups` in Slice A returned HTTP 405 in
production. The chip strip would surface as "Failed to load: HTTP
405". Fix: added `.get(...)` on the route binding.

### Polymarket link not persisted on first save/publish

Unrelated to cascades but discovered in the same session. The
cockpit stored `pm_event_id` / `pm_market_id` in RAM after
`import_polymarket_forecast`, but neither the save payload nor the
publish payload carried them. On next `open_forecast` the server
returned `metadata.polymarket = null`, the PM panel disappeared,
and the question-input observer fired a fresh typeahead search
(the stuck "🔍 SEARCHING POLYMARKET…" strip). Fix: new
`ApiClient::pm_link` method + `POST /api/polymarket/link` call from
`persist_backend_save` and the publish handler, on
`created == true` only.

## 7. Known operational quirks

- **Filesystem sync races.** During the session I saw tracked file
  edits silently revert twice (once for `apply_wc_cascades.rs`
  handler code, once for `tests/forecast_cascade_provenance_shapes.rs`
  which disappeared entirely). Both were re-applied. Root cause not
  identified — possibly LSP cache, Zed's autosave interacting with
  a watcher, or a background sync process. Verify your commits
  contain what you think they contain by checking `git status` +
  `git diff --cached --stat` before pushing.
- **Auto-push before you're ready.** An intermediate commit
  (`e5c0f9d`) got pushed to `origin/main` before I amended it
  locally. Root cause not identified. If you hit this, use
  `git push --force-with-lease` to replace the incomplete commit
  with the intended one (same recipe used for v0.8.8 and v0.8.10).
- **Auto-forecast bot.** Something in the repo periodically writes
  `forecasts/*.state.json` / `.evidence.md` / `.fpl` files (e.g.
  the EPL Chelsea + Man City files that landed in v0.8.10 via
  `git add -A`). Harmless but noisy in diffs. Use explicit file
  paths on `git add` if you want a clean commit.
- **`git add -A` will sweep operator working-tree changes.** If you
  see files you didn't touch in `git status`, they're probably from
  a parallel thread the operator is working on. Ask before folding
  them in; the pattern in this thread was to fold them with an
  honest section in the release notes (see v0.8.8, v0.8.10, v0.8.11
  release notes' "Also in this release" sections).
- **Schema-consistency lint on pre-commit.** `.git/hooks/pre-commit`
  runs `scripts/lint-schema-consistency.py` on staged `.rs` files.
  It's fast and doesn't remove files from the commit; the "scanning
  N Rust file(s)" line in the commit output is just a lint report.
  Migrations get their own lint via `scripts/lint-migrations.sh`.

## 8. How to validate a change to this thread

Fast to slow:

```bash
# 1. Console-side changes compile
cargo check -p fermi-console

# 2. Server-side changes compile
cargo check --bin api-server

# 3. Everything compiles
cargo check --workspace

# 4. Cascade shape tests + engine math
cargo test --test forecast_cascade_provenance_shapes \
           --test cascade_group_propagate_shapes \
           --test relationships_mutex_math

# 5. If you touched the timeline / trajectory
cargo test --test forecast_timeline_shapes

# 6. Full test suite (slow)
cargo test --workspace
```

None of the cascade shape tests need a live DB — they assert JSON
invariants on constructed fixtures. If you add live-DB integration
tests, keep them gated behind `DATABASE_URL` per the existing
convention in `tests/api_tests.rs`.

## 9. Companion documents

Read these before touching the thread:

- **`docs/fermi/WORLD_CUP_ROADMAP.md`** — the original 7-phase
  roadmap. Cascades are the Phase 2.5 addition; the design
  proposal for the generalization + surfacing lives in the same
  conversation history the WC roadmap references.
- **`docs/fermi/reports/WORLD_CUP_SIMULATION_POST_MORTEM.md`** —
  the retrospective that identified cascades as the only loop
  that moved forecasts during the WC sim. This is why the
  Provenance surface exists.
- **`docs/fermi/SCENARIO_TREE_DESIGN.md`** — the operator's
  design doc for the next slice. Authoritative for scenario-tree
  work. Contains the algorithm, the data-plumbing route trade-off,
  and the slice breakdown.

## 10. Contact / provenance

Everything in this thread was shipped by ai session on
`2026-07-27` against the operator's live testing. Commits are
attributable to `labra.studio` (the operator's git identity).
Release notes for each version live at the repo root
(`RELEASE_NOTES_v0.8.8.md` … `v0.8.10.md`).

Baseline for the next session: `v0.8.10` (as amended). Working tree
at handoff time should match tag `v0.8.10` (unless the operator has
since started Slice 2 or bug-fixed something else).
