# Agent Version History — Design Note

**Status:** not yet built — this document captures the design so it can
be built correctly when prioritised.
**Date:** 2026-05-18
**Author:** Ivan Labra (drafted with Kilo)

---

## Why this matters more than it looks

An agent's system prompt is its behaviour. Every observation the platform
records — eval scores, anomaly events, coherence evaluations, calibration
signals — is only meaningful relative to the prompt version that produced
it.

Without version history linked to observations:

- You see a Brier score improve. Was it the prompt change you made
  yesterday, or the new training data, or just variance?
- An anomaly fires. The reviewer resolves it. But the agent was on v1.3
  when the anomaly happened and is now on v1.5. Did the fix actually
  address the root cause, or did the prompt change mask it?
- Loop 5 (calibration) accumulates scores across prompt versions and
  treats them as a single signal. A version boundary is a distribution
  shift. Without labelling it, the calibration is measuring a mixture.

**The `persona_version` integer on `episodes` was added precisely to
handle this** (migration 103, observability foundations). But it's only
half the solution: the integer increments on every save but doesn't link
back to the actual prompt content that produced those episodes. You can
see that version 7 was different from version 6, but not *how*.

This note specifies what "complete" looks like.

---

## What already exists

### `agent_versions` table (migration 024)

```sql
CREATE TABLE agent_versions (
    version_id      UUID PRIMARY KEY,
    agent_id        UUID NOT NULL REFERENCES agents(agent_id),
    version_number  INT NOT NULL,
    system_prompt   TEXT,
    tags            TEXT[],
    model           TEXT,
    temperature     FLOAT,
    changed_by      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

`create_agent_version()` in `store.rs` snapshots the agent before every
`update_agent()` call. So the history exists — every time someone saves
via the Manage tab, a snapshot is written.

**What's missing:** the snapshot does not capture capability_gates,
model_ladder, min_tier, or output_contract. These are the "Intelligence
tab" fields — exactly the configuration that determines *how* the agent
routes, which model it runs, what it's allowed to do. A prompt change
without a model ladder change looks the same in agent_versions today.

### `persona_version` on `episodes`

The integer foreign-key stand-in. Populated at execution time from
`agents.persona_version`. Lets the observatory say "these 50 episodes
were all produced by version 7." But version 7's content is only
recoverable by looking at `agent_versions WHERE version_number = 7` —
and that join is not currently surfaced anywhere in the UI.

### `eval_signals.model_used` (migration 124)

Records which model actually ran. This is the provider/model string, not
the persona version. Useful for Phase 2 observability (provider filtering)
but doesn't capture the prompt.

---

## What needs to be built

### 1. Extend `agent_versions` to capture full capability config

The snapshot currently captures: `system_prompt`, `tags`, `model`,
`temperature`, `changed_by`.

It needs to also capture:
- `model_ladder` JSONB
- `capability_gates` JSONB
- `min_tier` TEXT
- `output_contract` JSONB
- `version` TEXT (the human-readable semver string, e.g. "2.0.0")

Migration:
```sql
ALTER TABLE agent_versions ADD COLUMN IF NOT EXISTS model_ladder    JSONB;
ALTER TABLE agent_versions ADD COLUMN IF NOT EXISTS capability_gates JSONB;
ALTER TABLE agent_versions ADD COLUMN IF NOT EXISTS min_tier        TEXT;
ALTER TABLE agent_versions ADD COLUMN IF NOT EXISTS output_contract JSONB;
ALTER TABLE agent_versions ADD COLUMN IF NOT EXISTS version_string  TEXT;
```

`create_agent_version()` in store.rs then needs to bind these fields
from the current agent row before the update is applied.

### 2. History tab on the agent detail page

A new tab on `/agent/:id` — after Manage — that shows:

```
VERSION HISTORY

v7  2026-05-18 14:30  Ivan Labra   claude-sonnet-4-6  "You are the SimOps Companion — the strategist..."
v6  2026-05-17 09:12  Ivan Labra   claude-sonnet-4-6  "You are the SimOps Companion — the navigator..."
v5  2026-05-16 22:44  Ivan Labra   claude-haiku-4-5   "You are the SimOps Companion — the navigator..."
```

Each row:
- Version number + timestamp + author
- Model used (from snapshot)
- First 100 chars of system prompt
- [View diff vs previous] button
- [Restore to this version] button
- Eval signal summary for episodes produced under this version
  (mean score, anomaly count, episode count) — pulled from
  `eval_signals JOIN episodes ON persona_version = version_number`

### 3. Diff view

When the user clicks "View diff vs previous", show a side-by-side or
unified diff of:
- `system_prompt` (most important — this is where the behaviour lives)
- `model_ladder` (if changed)
- `capability_gates` (if changed)
- `output_contract` (if changed)

This doesn't need a fancy diff library — a simple character diff rendered
as coloured spans is enough. The important thing is that the user can see
*what changed* between the version that worked and the version that
introduced a regression.

### 4. Restore

"Restore to this version" applies the snapshot as a new `update_agent()`
call, which itself creates a new version snapshot. So a restore is
version 8 → restore to v6 → creates version 9 (which has v6's content).
The history is never destructive — you can't delete a version, only move
forward.

### 5. Observatory integration

The observatory (`/observatory?agent=<id>`) currently shows anomalies and
eval signals without version context. Add:

- A version selector dropdown: "Filter to version: [all] [v7] [v6] ..."
- A version boundary marker in the timeline: a horizontal line with
  "v6 → v7 (prompt change)" at the timestamp of the version bump
- The eval signal chart split by version: instead of one trend line,
  show one line per version with the boundary visible

This makes the calibration signal interpretable: "eval scores dropped at
the v6 → v7 boundary, then recovered at v7 → v8" is a diagnostic, not
just noise.

---

## The key invariant

**An observation without a version label is incomplete.**

Every row in `eval_signals`, `anomaly_events`, `coherence_evaluations`
that has a `persona_version` integer already satisfies the data model
requirement. The `persona_version` integer is the label. What's missing
is:

1. The UI to make that label meaningful (History tab + observatory
   version filter)
2. The snapshot completeness (full capability config, not just prompt)

Both are UI and store changes — no new tables needed except the column
additions to `agent_versions`.

---

## Why capability_gates matters as much as the prompt

A developer working on their agent via the Manage tab will change:
- The system prompt (obviously)
- The model (claude-haiku → claude-sonnet when they notice parse errors)
- The min_tier (free → standard when the action grammar needs Sonnet)
- The capability_gates (adding min_provider_class: cloud_standard)

Each of these changes the agent's behaviour in production. If the eval
scores change after bumping min_tier from free to standard, that's a
configuration change, not a prompt change. The version history needs to
capture both so the developer can distinguish:

```
v5 → v6: system_prompt changed (200 chars added to action grammar)
v6 → v7: model changed (haiku → sonnet), min_tier changed (free → standard)
v7 → v8: system_prompt changed (emergent design section added)
```

Without capturing the full config, v6 → v7 looks like "nothing changed"
in the current snapshot, but the agent's behaviour changed completely.

---

## Priority and sequencing

**Don't build before:**
- The smoke test is green (companion v3 working end-to-end)
- The Manage tab correctly saves all fields (currently version string and
  output_contract may not persist on every save — verify first)

**Build in this order:**
1. Migration: extend `agent_versions` with the capability columns
2. Update `create_agent_version()` to snapshot the full config
3. History tab UI (list view + diff + restore)
4. Observatory version filter and boundary markers

Steps 1+2 are a day. Steps 3+4 are 2-3 days. Total: ~4 days.

**The reason to do 1+2 first even before the UI:** every save from now
on will capture the full snapshot. Waiting means losing history that
would have been captured. The UI can be built later against existing data.
The data collection should start now.
