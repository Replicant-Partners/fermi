# Reconciliation: Schema-Drift Harness + Business-Rule Audit vs. Actual ΞSYSTEM

**Date:** 2026-08-06 · **HEAD:** `0f0616e0` (post-v0.11.8)
**Reconciles:** `docs/schema-drift-harness-prompt.md`, `docs/business-rule-execution-audit-prompt.md`

---

## TL;DR

The two prompts diagnose the right disease and prescribe for the wrong body. Their
*failure-mode taxonomy* is accurate and several named modes are live in production right
now. Their *architectural premises* — event sourcing, projectors, ΞPROV, TimescaleDB,
scope-leases — are largely fiction. Roughly **40% of the commissioned deliverables have
no substrate to attach to**, and the 60% that do are higher-value than the prompts imply
because they address the actual root cause of the v0.10.15 → v0.10.29 hotfix run.

Three findings dominate everything else:

1. **The v0.11.0 schema trust contract is a no-op that reports permanent failure.**
   `fermi_leaderboard` is a `MATERIALIZED VIEW` (`migrations/094:178`) but is listed in
   `SCHEMA_TABLES` (`src/schema_trust.rs:84`), and `verify()` probes
   `information_schema.tables` (`:384-386`), which **excludes materialized views in
   PostgreSQL**. Therefore `verify()` can never return healthy, `SCHEMA_STRICT=1` would
   hard-fail every boot (which is presumably why it is set nowhere), and the verdict is
   `eprintln!`-only (`:538-594`) so nobody has ever noticed.
   *The drift detector is itself a textbook instance of business-rule failure mode #2 —
   a guard that runs, cannot pass, and is therefore ignored.*

2. **Migration application is unverifiable.** `run_migrations`
   (`src/api_server.rs:429-899`) is a hardcoded 180-path list that swallows every error
   (`:890-893`, `eprintln!` + "Don't panic"). CI mirrors the swallow with `|| true`
   (`.github/workflows/ci.yml:81`). There is **no migration ledger** — no
   `schema_migrations`, no checksums, no applied_at. "Did migration N run in prod?" is
   currently unanswerable, which *is* the root cause the harness prompt is chasing.
   Bonus: `migrations/126_agent_version_full_config.sql` is on disk and in CI's glob but
   **absent from the runner list** — CI and production schemas differ by one migration.

3. **`tracing` has no subscriber in the production binary.** `src/api_server.rs::main`
   never initialises `tracing_subscriber`; all 108 `tracing::{info,warn,error}!` call
   sites emit into a no-op dispatcher and are discarded. Any rule-execution tracing built
   on `tracing` today writes to `/dev/null`. This is step zero for the entire second
   prompt.

---

## Part 1 — Premise audit

| Prompt premise | Reality | Verdict |
|---|---|---|
| CQRS / append-only event-sourced backend | Conventional Axum + sqlx CRUD over ~123 mutable tables. 511 `UPDATE`s. No `EventStore`, `Aggregate`, `apply_event`, `replay`, `fold`. | **FALSE** |
| Event records are the source of truth; tables are projections | Inverted. Tables are canonical; logs are written *after* the mutation and never read back. Canonical example: `fermi-auth/src/credits.rs:245` mutates `wallets.balance`, then `:283` inserts `credit_ledger.balance_after` as a denormalised copy. Zero code recomputes balance from the ledger. | **FALSE** |
| Projector code reads events → writes rows | Word collision. `crates/projections` = Monte Carlo distribution projection. `agent-bestiary/projector` = PCA dimensionality reduction. Neither touches an event log; the latter has no DB writes. | **FALSE** |
| ΞPROV provides provenance logging | The string `ΞPROV` appears in exactly two files: the two prompts. Nearest real artifact is `embedding_provenance` (`migrations/135`) — narrow (embeddings on 5 tables) but genuinely well-built, transactional, and the **only** DB-enforced-immutable table in the repo (`135:106`, the sole `REVOKE` in 182 migrations). | **FALSE as described** |
| pgvector | Confirmed throughout — `vector(1024)`, HNSW indexes (`migrations/142:44-59`), `pgvector` crate. | **TRUE** |
| TimescaleDB | Zero `create_hypertable`, zero `time_bucket`. Only `CREATE EXTENSION`s are `uuid-ossp` and `vector`. The only file containing "TimescaleDB" is the prompt. | **FALSE** |
| ABW scope-leases exist at Episode/Workspace granularity | No lease system exists at all. Zero hits for `lease`/`ScopeLease`/`pg_advisory_lock`/`FOR UPDATE SKIP LOCKED`. Only `ConsolidationLock` (`agent-bestiary/memory/src/locking.rs`), per-agent, memory-consolidation only — correctly implemented, with a stubbed `cleanup_expired()` returning `Ok(0)`. | **FALSE** |
| Table drift is a downstream artifact of event drift | No projectors, so no such causal chain. **Table schema drift is real and severe** — it just has a different cause: swallowed migrations + a 634-line duplicate DDL path. | **FALSE cause, real symptom** |
| Business rules may be silently failing | **Confirmed, extensively.** See Part 2. | **TRUE** |

### The one place resembling event sourcing is itself broken

`crates/simops/src/event_kinds.rs` declares `SimOpsEventKind` (17 variants) and documents
payload schemas *in doc-comments only* — no Rust payload structs, so nothing validates
them. It has three live drift bugs:

1. `event_kinds.rs:191` says kinds live in `workspace_messages.kind`. **That column does
   not exist** (`migrations/014:6-17`; only later `ALTER` adds `audio_url`).
2. `event_kinds.rs:155` says events store `message_type: "event_append"`. The live CHECK
   constraint (`migrations/077:7-13`) does not permit that value — such an insert is
   rejected.
3. `src/handlers/workspace/messages.rs:285-289` **discards the caller's `message_type`
   entirely**, writing `"chat"` or `"agent_invocation"`. Every SimOps "event" persists as
   `message_type = 'chat'` with the kind surviving only inside free-form `metadata` JSONB.

The fold logic that would make these events authoritative is client-side JavaScript
(`simops-folds.js`) **in a different repository** and rebuilds browser state on page
mount, not Postgres tables (`event_kinds.rs:137-150`).

**Implication:** event sourcing is an aspiration documented as a fact. Deliverables built
on it must be deferred, not descoped-in-place.

---

## Part 2 — The failure-mode taxonomy is correct

Every mode named in the business-rule prompt is present, with concrete instances:

| Mode | Live instance |
|---|---|
| **#1 Swallowed errors** | ~240 `let _ = sqlx::query` (swallowed *writes*). Worst: `src/handlers/billing.rs:255-266` — Stripe idempotency check is `if let Ok(Some(_))`, so a transient DB error falls through and **double-credits real money**; the idempotency marker write at `:282-289` is itself `let _ =`. |
| **#2 Trivially-always-* guards** | `src/schema_trust.rs:84` + `:384` — the drift detector can never report healthy (see TL;DR). `src/gas.rs:342-352` `check_low_balance` has three swallows in one expression and **fails open** to "balance is fine" on DB error. |
| **#3 Orphaned rules** | `llm_rate_limit_middleware` (`src/api_server.rs:171`) is `#[allow(dead_code)]` and never referenced — **protected routes, which spend credits and invoke LLMs, are unrated**. `SemanticAnalyzer` (`src/semantic.rs`, 1101 lines, ~40 validation rules) has **zero callers in `src/handlers/`**; HTTP execution paths skip semantic analysis entirely. |
| **#4 Short-circuited guards** | `src/handlers/relationships/recompose.rs:160-171` issues an `UPDATE` with **no `status` filter**; for resolved forecasts the freeze trigger (`src/api_server.rs:1430-1462`) silently reverts it with only `RAISE WARNING` and `NEW.x := OLD.x`. The function returns `Ok(displayed)` containing values never persisted, and the UI renders them. The trigger's own message literally instructs callers to filter on `status='active'`. **Two rules in direct silent conflict.** |
| **#5 Precondition drift** | The SimOps `event_append` chain above — the rule path is fine, the trigger is unreachable. |
| *(bonus)* **Fail-open on unscorable input** | `agent-bestiary/coherence-gate/src/gate.rs:104-114` — the Γ(C) ≥ 0.5 block is inside `if let Some(gamma)`. If settling yields `None`, the block is skipped and the verdict is set to `Approved`. No "cannot evaluate → deny" branch. |

Also worth recording: the coherence gate is **not** orphaned (it is routed, live, and
fails loudly at `src/handlers/observatory.rs:367-381`), but it is **re-run rather than
replayed** at consensus time (`:580-589`), so the verdict on the receipt is not
necessarily the verdict the first reviewer saw. Nothing persists which gate decision
authorised a write — precisely the gap rule-execution tracing closes.

### Rule-layer structural facts

- **No rule registry, no rule engine, no naming convention.** Invariants live in four
  substrates with no shared vocabulary: Rust guards (`fermi-auth/`), inline handler
  checks (37 ad-hoc sites), Postgres CHECK/trigger/plpgsql (some in migrations, some
  inline in `api_server.rs`), and pure-computation validators (some not on the live path).
- **ACL enforcement is bimodal.** The canonical ladder
  (`fermi-auth/src/visibility.rs:57-139`) is genuinely well-built — the
  portfolio-inheritance leak guard (`:187-210`) even has a regression test asserting the
  guard text survives edits (`:460-470`). But only **15 of 88** handler files use it;
  **37 sites in 16 files** reimplement a weaker `owner_id != user_id && !can_admin()`
  version that bypasses share/team/portfolio-inheritance, and will 403 users who
  legitimately hold a team share.
- **12 database transactions in 223k LOC.** Every multi-write business operation
  inspected — charge+ledger, charge+royalty, resolve+counterfactual, git-commit+audit —
  is non-atomic, and several of the non-atomic follow-ups are `let _ =`-swallowed.

---

## Part 3 — Deliverable disposition

### Build (substrate exists, high value)

| Prompt | Deliverable | Note |
|---|---|---|
| Schema #4 | Drift alarm & provenance log | **This is the keystone.** Build it for real; do not "integrate with ΞPROV". |
| Schema #3 | Unified introspection tool | Real and needed. Requires #4 to report "last known diff status". |
| Schema #5 | Migration diff gate | Highest root-cause value. `scripts/migration_apply_check.sh` is a working embryo that nothing invokes. |
| Schema #6 | Compile-time / type-check enforcement | **0 of 1,474 queries are compile-checked.** Biggest single win. `scripts/spec26_sql_check.sh` PART A is the working prototype. |
| Rule #1 | Business rule registry | Model it on `schema_trust`'s hand-declared const contract — the pattern that already works in this codebase. |
| Rule #2 | Execution tracing | Blocked on initialising `tracing_subscriber` first. |
| Rule #3 | Swallowed-error lint | Narrow scope only (see below). |
| Rule #5 | Correlation report | Viable once #4 exists, with a substituted correlation key (see below). |
| Rule #6 | Task-completion gate | Needs a `CLAUDE.md`/`AGENTS.md` — **no such file exists in the repo today**. |

### Defer (no substrate)

| Prompt | Deliverable | Why |
|---|---|---|
| Schema #1 | Event schema registry | No event log, no payload structs, no versioning. Would be registering schemas for events that are stored as `message_type='chat'` with the kind thrown away. Revisit only if/when SimOps event-append is made real. |
| Schema #2 | Projector validation hook + `--rebuild-and-diff` | **No projectors exist.** Nothing to hook. Nothing to rebuild from. |
| Schema #7 | Fine-grained scope leases | There is no lease system to make finer-grained. With 12 transactions in 223k LOC, **transactions are the prerequisite**; leases are premature optimisation of a concurrency model that doesn't exist yet. |
| Rule #4 | `cargo-mutants` | Prohibitively slow on 223k LOC, and it answers a weaker question than a targeted known-bad-input harness. Replace, don't adopt. |

### Substitutions

Two prompt requirements need a different mechanism to survive contact with reality:

**1. "Tag everything with the event schema version" → tag with a `contract_fingerprint`.**
There are no event schema versions. The available, sufficient correlation key is:

```
contract_fingerprint = hash(SCHEMA_TABLES ++ SCHEMA_COLUMNS ++ SCHEMA_FUNCTIONS)
migration_high_water = max(applied migration filename)   [once the ledger exists]
build_sha            = git describe
```

This satisfies the actual acceptance criterion — bisect drift to a specific
schema-version/migration event without replaying anything — without inventing an event
log to hang it on. When a rule's pass rate jumps to 100%, you correlate against
`contract_fingerprint` and `migration_high_water` changes in the same window. Same
diagnostic power, real substrate.

**2. "Mutation testing" → per-rule known-bad-input reachability tests.**
For each registered rule, a table-driven test asserting a known-invalid input is
*rejected*. This is what actually distinguishes "orphaned" from "legitimately always
passing", it runs in seconds rather than hours, and it doubles as executable
documentation of the invariant. Note this is genuinely different from the existing
"shapes" tests (`tests/*_shapes.rs`), which are **fixture-vs-fixture and invoke no
handler or DB code** — they cannot detect handler drift, let alone schema drift.

---

## Part 4 — Sequenced plan

### Phase 0 — Un-break the instruments already paid for
*Hours of work. Every item is a fix to something that exists and doesn't function.*

- **0.1** Fix the matview false positive. Probe `pg_class` with
  `relkind IN ('r','p','v','m','f')`, or split the contract into `SCHEMA_TABLES` +
  `SCHEMA_MATVIEWS`. **Without this, nothing downstream can ever be green.**
- **0.2** Make `schema_trust` testable. It is included via
  `#[path] pub(crate) mod` into the binary only (`src/api_server.rs:53-54`) and is
  invisible to `cargo test`. Export it from `src/lib.rs`; add a test asserting the
  contract is satisfiable against a freshly-migrated DB.
- **0.3** Fix strict-mode fail-open: `verify_and_report` (`:601-616`) returns
  `DriftContinueBoot` when the *probe itself* errors, so a permissions error silently
  disables the check even under `SCHEMA_STRICT=1`.
- **0.4** Add `migrations/126_agent_version_full_config.sql` to the runner list, or
  supersede the list entirely in Phase 2.
- **0.5** Re-link the stale pre-commit hook (`.git/hooks/pre-commit` is a **regular file
  dated May 15**, not a symlink, and omits the `lint-owner-columns.sh` call — that lint
  currently runs *nowhere*), and move all three linters into `ci.yml` as blocking jobs.
  Note `migration-lint` in CI does **not** call `scripts/lint-migrations.sh` — it
  reimplements one of its three rules inline.
- **0.6** Initialise `tracing_subscriber` in `src/api_server.rs::main`. One line;
  unlocks 108 already-written structured events.
- **0.7** Remove `|| true` from `ci.yml:81` so a broken migration fails CI.

**Exit criterion:** `SCHEMA_STRICT=1` can be enabled in staging without a false positive.

### Phase 1 — The integrity log (the shared timeline both prompts require)

One append-only table, modelled on `embedding_provenance` (the only DB-enforced-immutable
precedent in the repo — copy its `REVOKE UPDATE, DELETE` and its transactional write
path). This *is* ΞPROV; we're building it, not integrating with it.

```
integrity_log(
  event_id, occurred_at,
  kind,            -- schema_verdict | migration_applied | rule_execution
                   -- | lint_finding | diff_gate | lock_event
  severity,        -- info | warn | breaking
  domain,          -- table / bounded context
  subject,         -- rule_id | migration filename | table name
  expected JSONB, observed JSONB, diff JSONB,
  contract_fingerprint, migration_high_water, build_sha,
  correlation_id
)
```

Resolution is a **new row**, never an UPDATE — keeps it honestly append-only and gives
you alarm *age* for free.

Then: persist the boot verdict here (closes the "no history" gap — you currently cannot
answer "did the last 5 deploys boot clean"), and add a CLI to list unresolved alarms by
age.

### Phase 2 — Migration ledger + apply gate *(root cause)*

- **2.1** Replace the hardcoded 180-path list with a directory scan against a real
  `schema_migrations(filename, checksum, applied_at, duration_ms, error)` ledger. On
  error: record it and, under strict, abort. ⚠️ **This is the one genuinely risky change**
  — prod has 180 migrations already applied with no ledger, so it needs a careful
  bootstrap (mark existing files applied-by-assumption, flagged as such, only if the
  Phase-0 contract passes).
- **2.2** Wire `scripts/migration_apply_check.sh` into CI on any `migrations/**` diff. It
  already works — spins a throwaway cluster, applies twice with `ON_ERROR_STOP=1` — and
  **nothing calls it**. This is free value.
- **2.3** Make the `ensure_critical_schema` duplication visible. Those 634 lines
  (`src/api_server.rs:905-1538`, 54 single-statement DDL pairs) re-declare load-bearing
  objects from migrations 094/113/140/166/172/174 because PgBouncer transaction mode ate
  the multi-statement versions. The PgBouncer reason is real — **do not delete it** — but
  several objects now have two competing definitions with no sync mechanism. Emit the
  diff between them into `integrity_log`.

### Phase 3 — Diff gate

> ⚠ **BLOCKED on Phase 2.** Phase 0 execution proved that **26 of 181
> migrations fail against an empty database** — the migration set cannot
> rebuild the schema from scratch. Every dump-and-diff strategy needs a
> buildable reference schema, so this phase cannot start until the migration
> baseline reaches 0. See "Phase 0 execution findings" below.

**Tool recommendation: `migra`-style dump-and-diff, not Atlas.** Justification: Atlas
wants a declarative desired-state, and authoring one over 182 accreted migrations plus a
634-line imperative patch path is a large lift with poor payoff. What you actually need
is "apply migrations to a fresh DB → dump; introspect prod → dump; normalise; diff" —
which is `migration_apply_check.sh` generalised, answers the acceptance criterion
directly, subsumes the 2.3 duplication question, and handles pgvector/matviews/plpgsql
without Atlas's extension edge cases. Gate output goes to `integrity_log` pass or fail.

### Phase 4 — Unified introspection tool

`schema-introspect --domain <name>` as a real bin: live `information_schema` (fresh,
never cached) + relevant contract subset + last verdict and diff status from
`integrity_log`. Then **create `AGENTS.md`/`CLAUDE.md`** (none exists) mandating its
invocation before any migration/schema-adjacent edit. The prompt is right that
availability alone doesn't guarantee use — but there is currently no file in which to
place the instruction.

### Phase 5 — Query verification *(biggest single win)*

Generalise `scripts/spec26_sql_check.sh` PART A from Spec-26 to the whole repo: extract
SQL string literals (the extractor in `lint-schema-consistency.py` already does this) and
`PREPARE` every one against a freshly-migrated DB. This catches typos, bad casts, wrong
arity, and missing columns across ~1,474 queries **with zero code churn** — most of the
value of compile-time checking without rewriting anything. Adopt `sqlx::query!` +
committed `.sqlx/` cache opportunistically afterward for new/hot queries, with a CI job
refreshing the cache against a fresh migrated DB (the staleness risk the prompt correctly
flags).

### Phase 6 — Business rule layer

- **6.1** Rule registry as a hand-declared const manifest in the `schema_trust` house
  style: `rule_id`, invariant, impl site, trigger. Seed it with the ~16 rules already
  enumerated in the audit. Do not attempt completeness.
- **6.2** `rule_trace!(FR_007, outcome)` at registered call sites → `integrity_log`
  `kind='rule_execution'`, stamped with `contract_fingerprint`. Orphaned-rule alarm =
  registered rule with zero traces over a rolling window.
- **6.3** Narrow swallowed-error lint, in the existing Python-linter style (**not** custom
  clippy lints — that needs nightly/dylint). Scope: `let _ =` / `.ok()` applied to
  `sqlx::query*` or `credit_*` (~240 hits, catches 8 of the 10 worst offenders), plus
  `if let Ok(Some(_)) = <query>` guard reads (catches the Stripe double-credit). A blanket
  lint would fire ~3,300 times and be ignored — that outcome is worse than no lint.
- **6.4** `tests/rule_reachability.rs` — per-rule known-bad-input rejection tests.
- **6.5** Correlation CLI: rule outcome rate over time vs. `contract_fingerprint` /
  `migration_high_water` changes.

---

## Part 5 — Bugs to fix regardless of any of the above

These are live defects surfaced by the audit. None require the harness; several involve
money or silent data corruption.

| Severity | Bug |
|---|---|
| 🔴 | **Stripe double-credit** — `src/handlers/billing.rs:255-266` + `:282-289`. |
| 🔴 | **Credit destruction** — `src/gas.rs:414-433` (royalty deposit + its audit row both `let _ =` after the caller is debited at `:374`) and `:442-471` (auto-collect). |
| 🔴 | **Protected routes unrated** — `llm_rate_limit_middleware` is dead code; `rate_limit_middleware` is layered only on `public_routes` (`src/api_server.rs:2356-2359`). Credit-spending LLM endpoints have no rate limit. |
| 🟠 | **Mutex recompose vs. freeze trigger** — `relationships/recompose.rs:160-171` missing `status` filter; returns and renders values never persisted. |
| 🟠 | **Coherence gate fails open** on `gamma == None` — `coherence-gate/src/gate.rs:104-114`. |
| 🟠 | **`let _ =` DELETE inside a transaction** — `src/handlers/admin.rs:1539-1542`. On Postgres a failed statement aborts the whole tx; every subsequent delete fails with "current transaction is aborted" and the `let _ =` hides the trigger. |
| 🟠 | **`check_low_balance` fails open** — `src/gas.rs:342-352`. |
| 🟡 | **SimOps event-append is non-functional** — non-existent `kind` column, CHECK-rejected `message_type`, and the handler overwrites it anyway (`workspace/messages.rs:285-289`). Also `messages.rs:451` inserts `message_type='system'`, not in the CHECK list (`system_event` is). |
| 🟡 | **`ConsolidationLock::cleanup_expired()` is a stub** returning `Ok(0)` (`locking.rs:91-95`); the real `cleanup_expired_locks` has no production caller. |
| 🟡 | **11 of 12 integration test targets never run in CI**; `forecast_acl.rs` carries 39 `#[ignore]`s over 38 tests. |

---

## Part 6 — Recommended immediate scope

Phase 0 in full, plus Phase 1's table, plus the 🔴 bugs. Rationale: Phase 0 is all
small fixes to machinery that already exists and currently doesn't work, and it converts
the v0.11.0 investment from decorative to load-bearing. Phase 1's table is the
prerequisite for literally every remaining deliverable in both prompts. The 🔴 items
involve real money and are independent of all of it.

Phases 2 and 5 are where the real leverage is, and both should be scheduled deliberately
rather than opportunistically — 2.1 is the riskiest change in the plan, and 5 is the
largest.

**Do not start** with the event schema registry or the projector hook, regardless of the
prompts' stated sequencing. Their sequencing assumes item #1 is the foundation; here it
is the one item with no ground to stand on.

---

## Part 7 — Phase 0 execution findings (2026-08-06)

Phase 0 is implemented. Building the harness immediately surfaced findings that change
the plan, so they are recorded here rather than in a release note.

### 7.1 A *second* always-fails guard in the same contract

The matview bug was not alone. `verify()` compared the declared function signature
against `pg_get_function_identity_arguments(oid)`, which **includes parameter names**:

```
contract:  "text, boolean, text, text"
observed:  "p_forecast_id text, p_actual_outcome boolean, p_resolved_by text, p_resolution_notes text"
```

So `resolve_forecast` — the v0.10.19 witness, the very function the contract was extended
to protect — reported permanent signature drift and could never match. Two of the four
function entries and one of the 44 table entries were structurally unsatisfiable.

Fixed by building the type-only list from `proargtypes`. **Lesson: a contract that is
never asserted to be satisfiable will accrete unsatisfiable entries.** That is the whole
argument for `tests/schema_trust_contract.rs` existing.

### 7.2 The migration set cannot rebuild the schema — 26 failures

`scripts/schema_contract_check.sh` applies the real `run_migrations()` list to an empty
cluster. **26 of 181 files fail.** A representative sample of root causes (as opposed to
cascades):

| Migration | First error |
|---|---|
| `004b_migrate_users_for_auth.sql` | `column "id" does not exist` |
| `089_dashboard_spatial_queries.sql` | `PostGIS extension not found` |
| `090_social_layer.sql` | `relation "migrations_log" does not exist` |
| `094_fermi_forecasting.sql` | `column "status" does not exist` |
| `097_governance.sql` | `functions in index predicate must be marked IMMUTABLE` |
| `113_composition_as_first_class.sql` | `syntax error at or near "||"` |
| `142_performance_indices.sql` | `CREATE INDEX CONCURRENTLY cannot be executed from a function` |
| `166_agents_updated_at.sql` | `column reference "is_nullable" is ambiguous` |

Several are outright bugs (a syntax error; `CONCURRENTLY` inside a `DO` block; an
ambiguous column reference), not just ordering. All are invisible in production because
the runner `eprintln!`s and continues, and were invisible in CI because the loop ended in
`|| true`.

Most of the 26 cascade from a handful of early failures — `094` aborting at its first
error leaves `fermi_forecast_updates` uncreated, which then fails `140`, `149`, `150`,
`156` and `176`. So the true repair count is far smaller than 26.

**This is the finding that reorders the plan.** Phase 3's diff gate — and any
"rebuild-and-diff" strategy at all — requires building a reference schema from
migrations. That is currently impossible. Rebuildability moves from a nice-to-have to a
hard prerequisite.

### 7.3 `users.id` exists in production but in no migration

The sharpest single illustration. Migration 004 is the **only** file that creates
`users`, and it declares `user_id TEXT PRIMARY KEY` with no `id` column. Nothing else in
the repository adds one — not another migration, not `ensure_critical_schema`. Yet:

- `migrations/004b:24` does `UPDATE public.users SET user_id = id::text` (and fails)
- `fermi-auth/src/api_keys.rs:96` and `:166` do `JOIN users u ON ak.user_id = u.id`
- `schema_trust` declares `("users", "id")` — and production presumably satisfies it

The only consistent explanation is that **migration 004 was edited in place after it had
already been applied**. Production's `users.id` is a historical artifact that the
codebase can no longer reproduce.

Two consequences worth acting on:

1. **API-key authentication would break on any rebuilt database.** The `api_keys` lookup
   joins a column that a fresh migration run does not create.
2. It is proof that "the migrations are the schema definition" is false here. Any
   declarative-schema tooling (Atlas et al.) would be building on sand.

### 7.4 Two more migrations production never applies

Beyond `126` (now wired), comparing `run_migrations()` against the directory:

- **`136_embedding_provenance_not_null.sql`** — converts the Spec-22 embedding-provenance
  discipline into a hard DB invariant. Never applied. **The invariant the
  `embedding_provenance` design depends on is not actually enforced in production.** Do
  not wire it blindly: it will fail if any row has NULL provenance, which is precisely
  what needs checking first.
- **`180_orchestra_members.sql`** — SPEC_29, appears to be in-flight; left alone.

### 7.5 CI never built production's schema

`ci.yml` applied `ls migrations/*.sql | sort`. That is neither the runner's order nor the
runner's set. Combined with `|| true`, CI's database was a third distinct schema —
neither production's nor the migrations'. Now fixed to parse the real list, report each
failure as a GitHub annotation, and ratchet on the failure count.

### 7.6 What Phase 0 shipped

| Item | Status |
|---|---|
| 0.1 Matview probe (`pg_catalog`, relation-kind drift detection) | ✅ |
| 0.1b Function signature probe (`proargtypes`) — found during execution | ✅ |
| 0.2 `schema_trust` exported from `lib.rs`; 13 tests in `tests/schema_trust_contract.rs` | ✅ |
| 0.3 Strict mode fails closed when the probe itself errors | ✅ |
| 0.4 `migrations/126` added to the runner list | ✅ |
| 0.5 Stale pre-commit hook re-linked; 2 linters promoted to blocking CI | ✅ |
| 0.6 `tracing_subscriber` installed in `api_server::main` | ✅ |
| 0.7 `|| true` replaced with a per-file annotation + failure ratchet | ✅ |
| — `scripts/schema_contract_check.sh` (new harness) | ✅ |

**Phase 0's exit criterion is partially met.** The two unsatisfiable contract entries are
fixed, and satisfiability is now machine-checked. But it cannot be *proven* end-to-end
locally, because the migration set cannot build a complete schema to check against. The
remaining live-check failures are all downstream of §7.2, except `users.id` (§7.3).

Concretely: **do not enable `SCHEMA_STRICT=1` yet.** Run the live tier of
`tests/schema_trust_contract.rs` against production's `DATABASE_URL` first. If it comes
back healthy, strict mode is safe; the two false positives that made it un-enablable are
gone.

### 7.7 Revised immediate priority

Phase 2 is now unambiguously next, and its first task has changed:

1. **Make migrations rebuildable** — repair the ~8 root-cause failures, driving the CI
   ratchet to 0. Cheap (most are one-line SQL bugs), and it unblocks Phases 3 and 5.
2. **Reconcile `users.id`** — decide whether it is real and add a migration that creates
   it, or excise it from `api_keys.rs` and the contract. Currently the code and the
   schema definition disagree, and only production knows the answer.
3. Then the migration ledger (2.1), which prevents the whole class from recurring.
4. Then Phase 1's `integrity_log`.

The 🔴 bugs in Part 5 remain independent and can proceed in parallel.

---

## Part 8 — State integrity audit (`scripts/integrity_audit.sql`)

Everything above checks the **shape** of the database. This checks its **state**. They
fail differently: a swallowed `let _ = sqlx::query(...)` on a credit deposit leaves the
schema perfectly valid and the money gone.

36 read-only checks across seven categories — credit conservation, secrets, user/agent/
sharing orphans, forecast and Brier integrity, embedding provenance, and schema-object
presence. Every check is guarded on object existence and degrades to `SKIPPED` rather
than erroring, so it runs against any database regardless of which migrations landed.

```
psql "$DIRECT_DATABASE_URL" -f scripts/integrity_audit.sql
```

**Use a direct connection, not the PgBouncer pooler** — in transaction-mode pooling the
temp table can land on a different backend than the final SELECT. On Neon, the host
without `-pooler`.

Exits non-zero if there is any CRITICAL violation, any missing schema object, or any
check that errored, so it works as a release gate rather than only a report.

### The checks that matter most

| Check | What a non-zero result proves |
|---|---|
| `CREDIT-001` | `wallets.balance` ≠ `SUM(credit_ledger.amount)`. Nothing in the codebase ever recomputes a balance from the ledger, so this has never been looked at. Divergence = a swallowed ledger write or a balance mutated without a ledger row. |
| `CREDIT-003` | A Stripe session credited more than once — `billing.rs:255` guarded with `if let Ok(Some(_))`, so a transient DB error falls through and credits again. **Retroactive detection of real money already lost.** |
| `CREDIT-005` | `balance_after` ≠ the running total. Detects interleaved non-atomic writes. |
| `SEC-001` | Encrypted provider keys owned by a principal that no longer exists — live billable credentials with no accountable owner. |
| `USER-003` | `users.id` present or absent. If SKIPPED, `api_keys.rs:96` is joining a column that does not exist. |
| `FC-003` | `brier_score` ≠ `(scored_probability - outcome)²`. Direct evidence of post-resolution mutation — the `recompose`/freeze-trigger conflict. |
| `PROV-001` | Rows with an embedding but no provenance. This is precisely the backlog count that must reach zero before migration 136 can be applied. |
| `PRESENCE-*` | Whether the migrations that cannot be verified actually applied. |

### Why the presence checks are in a state audit

Because `run_migrations` swallows errors and there is no ledger, "did migration 171
apply?" is not answerable from any record — only by looking for what it should have
created. Until the ledger exists (Phase 2.1), object presence *is* the migration history.

### Reading the output

`SKIPPED` means the check could not run. **Treat it as UNKNOWN, never as passing.** A
check that cannot fail is not a check — which is the exact defect that made
`SCHEMA_STRICT=1` un-enablable for eight releases.
