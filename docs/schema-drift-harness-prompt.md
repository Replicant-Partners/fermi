# Coding Agent Task: Schema Drift Audit Instrumentation & Harness

## Context (give this to the agent verbatim)

We run a CQRS / append-only event-sourced backend (ΞSYSTEM) in Rust over Postgres
(pgvector, TimescaleDB). Event records are the source of truth; Postgres tables are
projections/materializations built by projector code that reads events and writes rows.
ΞPROV provides provenance logging for state-changing operations.

We are experiencing schema drift bugs at two coupled layers:

1. **Event schema drift** — event payload shapes evolve (fields added/renamed/retyped)
   without version tracking, so projectors silently build stale or incorrect
   assumptions into table schema.
2. **Table schema drift** — Postgres table schema diverges from what application code
   (structs, queries) assumes, often *because* it's a downstream artifact of (1).

Your job is not to redesign the domain model. Your job is to build the **audit
instrumentation and harness layer** that makes drift impossible to miss and cheap to
diagnose — turning an implicit, discoverable-only-at-runtime failure mode into an
explicit, gated, logged one.

## Non-negotiable design principles

- **Single source of truth per layer.** Event schema is canonical for event shape.
  Live Postgres schema (via `information_schema`) is canonical for table shape. Neither
  may be inferred from memory, prior conversation context, or hand-maintained docs.
- **Forced re-read, not cached trust.** Any tool/agent operation that touches schema
  must query current live state fresh, every time, before acting on it.
- **Diff-gated writes.** No schema-adjacent change (migration, projector edit, event
  type change) lands without an automated diff/validation step in the loop — not just
  advisory CI, but a blocking step in the same task.
- **Provenance-tagged everything.** Every schema-relevant artifact (event version,
  migration, projector rebuild) is stamped with enough metadata to bisect drift later
  without archaeology through the raw event log.
- **Symptom vs. root cause separation.** Table schema issues should be traceable back
  to the event schema version that caused them, not treated as independent bugs.

## Deliverables

### 1. Event Schema Registry
- A versioned schema definition per event type (JSON Schema or a Rust-native
  equivalent derived via `schemars`/`serde` — pick whichever integrates more cleanly
  with existing ΞSYSTEM event type definitions; state which you chose and why).
- Schema versions are immutable once published. A changed payload shape = a new
  version, never an in-place mutation.
- A registry lookup API: given `(event_type, version)` → schema. Given `event_type` →
  latest version.
- CI check: fail the build if any event type's Rust struct has changed in a way that's
  incompatible with its currently-registered schema version without a corresponding
  new version being registered.

### 2. Projector Validation Hook
- Before a projector applies an event to build/update a table row, it validates the
  event against its declared schema version.
- On validation failure: do not silently apply, do not crash the whole projection run.
  Log a structured drift-alarm record (see #4) with event id, event type, expected
  schema version, actual shape diff, and halt processing for that event with a
  retryable/quarantine state.
- Add a `--rebuild-and-diff` mode: wipe a target table (in a scratch schema, not
  production) and rebuild it from the event log via current projector logic, then diff
  the rebuilt schema against the live production table schema. Non-empty diff = drift,
  reported with column-level detail.

### 3. Unified Schema Introspection Tool
- One CLI command / tool-callable function, e.g. `schema-introspect --domain <name>`,
  that returns in one call:
  - (a) live Postgres schema for the relevant tables via `information_schema` (columns,
    types, constraints, indexes) — queried fresh, never cached.
  - (b) latest registered event schema version(s) relevant to that domain.
  - (c) a flag indicating whether (a) is consistent with what (b) implies the
    projection should look like (i.e., calls the rebuild-and-diff logic from #2 in a
    lightweight/cached-scratch-schema mode if feasible, or reports "last known diff
    status + timestamp" if a full rebuild is too expensive to run on every call).
- This is the tool any coding agent (human-directed or autonomous) MUST invoke before
  writing or editing any migration, projector, or schema-adjacent query. Add this as an
  explicit instruction in the agent's system prompt / CLAUDE.md, not just as an
  available tool — availability alone doesn't guarantee use.

### 4. Drift Alarm & Provenance Log
- A dedicated append-only log (can live in ΞPROV or alongside it) for drift events:
  validation failures, rebuild-diff mismatches, migration-diff gate rejections.
- Each record includes: timestamp, domain/table/event-type, schema versions involved
  (expected vs. observed), diff detail, and severity (informational drift e.g. a new
  nullable column vs. breaking drift e.g. a type change or dropped column).
- Every ΞPROV record for a schema-relevant write is tagged with the event schema
  version it was written under, so future drift can be bisected to the exact version
  that introduced it without replaying the whole log.
- Expose a simple query/report surface (even a CLI command) to list unresolved drift
  alarms and their age.

### 5. Migration Diff Gate
- Integrate a schema-diff tool (Atlas, `migra`, or equivalent — pick one, justify
  the choice against our Postgres/pgvector/Timescale stack) into the migration
  workflow such that:
  - Migrations are generated as diffs against introspected live schema, not hand-authored
    against assumed schema.
  - No migration is applied without passing this diff-gate step first.
  - The gate output is logged to the drift alarm log from #4 regardless of pass/fail,
    so there's a full history of proposed vs. applied schema changes.

### 6. Compile-Time / Type-Check-in-Loop Enforcement
- If using `sqlx`, confirm compile-time query checking is enabled against a real
  connected schema (not just `sqlx::query!` with an offline cache that can itself go
  stale — flag this risk explicitly and recommend a CI step that refreshes the offline
  cache against live schema on a schedule or pre-merge).
- Add a task-completion check for any agent (human or automated) that any code change
  touching DB-mapped structs must run `cargo check` (or the sqlx-aware equivalent)
  against a live/current schema as a gating step before the task is considered done.

### 7. Scope-Lease Enforcement at Schema-Domain Granularity
- Audit current ABW scope-leasing implementation: confirm leases can be acquired at
  schema-domain (table/bounded-context) granularity, not just Episode/Workspace
  granularity.
- If granularity is coarser than schema-domain today, add the finer-grained lease type
  and require any agent proposing a schema-adjacent change to acquire it before writing.
- Log lease acquisition/release events into the same provenance trail as #4 so
  concurrent-write drift incidents can be correlated with lease history after the fact.

## Acceptance criteria (the agent should self-check against these before declaring done)

- [ ] Can I ask "what schema version was event X written under?" and get an answer
      without manually reading raw event payloads?
- [ ] Can I run one command and know whether live Postgres schema currently matches
      what the event log + projector logic implies it should be?
- [ ] Is it structurally impossible (not just discouraged) for a migration to be
      applied without passing the diff gate?
- [ ] If two agents/processes touch overlapping schema domains concurrently, is that
      contention visible in a log rather than manifesting only as a downstream bug?
- [ ] If drift is discovered next month, can it be bisected to a specific schema
      version/migration/lease event without re-reading the entire event log by hand?

## Sequencing suggestion for the agent

Build in this order, since each depends on the prior: (1) Event Schema Registry →
(2) Projector Validation Hook → (4) Drift Alarm Log (needed by 2, so may need a stub
first) → (3) Unified Introspection Tool → (5) Migration Diff Gate → (6) Compile-check
enforcement → (7) Scope-lease audit. Ship each as a working, tested increment rather
than one large PR — schema-layer changes are exactly the kind of thing you don't want
to debug in aggregate.

## What NOT to do

- Do not invent a new domain modeling layer or ORM replacement — this is instrumentation
  around the existing stack, not a rewrite.
- Do not make the introspection tool optional/advisory only — it must be a required
  step, enforced via the agent's own operating instructions and, where possible, via
  CI/pre-commit gating.
- Do not collapse event-schema and table-schema validation into one layer — keep them
  distinct so root cause vs. symptom stays diagnosable.
