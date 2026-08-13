# v0.16.1 — the column existed, was the right type, and was always zero

A user reported that the Dashboard's Research panel was empty for
forecasts they knew had been researched. It was: `list_forecasts_handler`
never selected `evidence` or `agents_used`, so the card that summed them
read `None` for every row and rendered "no research yet" unconditionally.

Then they pointed at ABW's EXECUTION HISTORY panel — per-run costs, right
there on screen, `$0.616272`, `$0.311628` — and asked why the console said
`cost n/a`. That question opened the larger one.

## Two ledgers, and we trusted the empty one

`agents` carries five denormalised counters:

```
total_executions  successful_executions  failed_executions
total_cost_usd    avg_execution_time_ms
```

Nothing writes them. There is no `UPDATE agents SET total_executions`
anywhere in the codebase; they were added with the table and never wired
to the execution path. Meanwhile `episodes` records every run faithfully —
`cost_usd`, `tokens_used`, `execution_time_ms`, `execution_status` — which
is why the ABW panel was right the whole time.

At the moment of the fix:

| | |
| --- | --- |
| agent rows with a non-zero rollup | **3 of 743** |
| agents with real episodes | **196** |
| measured spend those episodes represent | **$296.49** |

Eight surfaces read the empty ledger and served zeros to users:

| Surface | Was showing | Actual |
| --- | --- | --- |
| console marketplace | every agent unpriced, ranked never-run | 196 agents with runs |
| Dashboard Research card | `cost n/a` on research that cost money | — |
| `profile.rs` / `users.rs` | 0 runs for everyone | 2703 for one user |
| orchestra membership inbox | 0 runs for every applicant | `efra_valuation` 26 |
| ecology census | 30 lifetime runs, platform-wide | 2735 |
| ecology specimen "vital signs" | `football_analyst` 0 runs / $0.0000 | 190 runs / $63.68 |
| workspace agent roster (×2) | 0 runs per agent | — |
| observatory fleet table | 0 executions per agent | — |

And one that was not a display bug. `admin_cleanup_test_cruft_handler`
gated deletion on four safety criteria, one of which was
`total_executions = 0` — "never ran real workload". Against a column that
is zero for every row, that predicate was always true: it eliminated
nothing and protected nothing.

Nothing was ever wrongly deleted; the prefix, tier and age gates are
individually sufficient. But a safety criterion that *cannot fail* is
worse than an absent one, because it makes the remaining gates look more
redundant than they are. The next person to relax one would think there
were four backstops when there were three.

## One definition

`migrations/192_agent_execution_rollup.sql` adds `agent_execution_rollup`,
a view over `episodes`. Every consumer — Rust and SQL — goes through it.

Deliberately **a plain view, not a materialized one**: a matview needs a
refresh path, and an unrefreshed matview is the same silent-staleness bug
wearing a different hat. `fermi_leaderboard` already needs
`refresh_fermi_leaderboard()` and is declared in `SCHEMA_FUNCTIONS`
precisely so the refresh cannot be forgotten.

Deliberately **not a backfill** of the existing columns: backfilled
counters are correct on the day of the backfill and wrong forever after,
which is strictly worse than visibly zero.

`src/agent_economics.rs` is the single Rust entry point. Two of its
choices are load-bearing:

- `avg_cost_per_run()` returns `Option<f64>`, `None` when there is nothing
  to divide. A `0.0` renders as "$0.00/run", which reads as *free* rather
  than *unknown* — and local/self-hosted models legitimately record zero
  cost.
- Failed runs are billed. A run that burned tokens and returned an error
  still cost money; pricing off successes alone under-reports exactly the
  agents wasting budget. `efra_critical_factor` has three runs, all
  errors, $1.032012.

The dead columns now carry `COMMENT ON COLUMN … DEPRECATED /
WRITE-ORPHANED`, so `\d agents` tells the story without anyone having to
find the migration.

## Why nothing caught this

This is schema drift, but not the shape kind. The column *existed*, was
*correctly typed*, and was *declared in `SCHEMA_COLUMNS`*. It was simply
always zero.

| Guard | Why it missed this |
| --- | --- |
| `schema_trust` boot probe | Declares `("agents","total_executions")` and checks presence. Present. |
| `SCHEMA_STRICT=1` | Same probe — nothing to abort on. |
| `lint-schema-consistency.py` | Flags refs to columns that don't exist. This one existed. |
| `schema_contract_check.sh` | Asserts the contract is *satisfiable*. It was. |
| Type checking | `i32` is `i32` whether it means 253 or 0. |
| Unit tests | Fixtures set the counters by hand, so they were never zero under test. |

Every one of them reasons about **shape**. This was **content**. Absence
would have been caught at boot; emptiness was invisible to all six.

## The harness

`src/rollup_trust.rs` declares each denormalised column with its source of
truth, its replacement, and a `Disposition` — `Maintained` (something
writes it, and it must agree) or `WriteOrphaned` (nothing does, and
nothing may read it).

**Tier 1 — offline, blocking, no database.** The tripwire: no
request-serving handler may read a write-orphaned column. Runs in a bare
`cargo test`, in the pre-commit hook, and in CI. This is the check whose
absence let six surfaces independently reach for the same dead ledger,
each written by someone who reasonably assumed a column called
`total_executions` contained the number of executions.

**Tier 2 — live, `scripts/rollup_contract_live.sh`.** Content
verification. The view must exist and *be a view*. `Maintained` columns
must agree with their source. `WriteOrphaned` columns must **disagree** —
if one agrees, either someone quietly added a writer (promote it, and let
the harness assert it) or this database has too little data to tell them
apart. Both deserve a look, so both fail loudly rather than passing on a
coincidence.

Current live output:

```
agent_execution_rollup: 196 agents, 3189 runs, $296.49 measured
agents.total_executions:      196 row(s) disagree
agents.successful_executions: 192 row(s) disagree
agents.failed_executions:      43 row(s) disagree
agents.total_cost_usd:        196 row(s) disagree
agents.avg_execution_time_ms: 196 row(s) disagree
```

### The detector was wrong twice first

Worth recording, because both failures are the ones this kind of check
usually dies of.

**v1 flagged the column name anywhere** and reported 23 hits, nearly all
legitimate — because the fix keeps the *wire key* and changes only the
*source*:

```rust
"total_executions": m.executions,                 // JSON output key
COALESCE(r.executions, 0) AS total_executions     // SQL result alias
r.try_get::<i64, _>("total_executions")           // read of that alias
```

A check that fails on correct code gets deleted by the first person it
inconveniences, and the deletion looks like cleanup. The precise signal is
a **qualified read** — `a.total_executions` — unambiguous because the
replacement view has no column of that name.

**v2 scanned only the first occurrence**, so it passed
`"total_executions": a.total_executions` — the innocent key first, the sin
second. It certified the exact line it was written to catch.

Both are now pinned by
`detector_flags_real_reads_and_ignores_innocent_ones`, which asserts in
both directions using lines transcribed from the real pre-fix and post-fix
source.

The tripwire was verified by regression: reverting one `profile.rs` query
to `a.total_executions` failed the test, naming `profile.rs:97`.

### It found two surfaces the audit missed

The workspace agent roster — `get_workspace_handler` and
`list_workspace_agents_handler` — reported 0 runs for every agent on every
workspace. Manual grep had missed both.

The ecology specimen sheet was the most misleading: a code comment claimed
the lens "doesn't render run counts or cost", so it was passing `None` for
measured stats. `templates/ecology.html` renders exactly those fields as
"Vital signs". The comment was false, and the zeros were user-facing.

## Schema contract

`SCHEMA_VIEWS` / `VIEW_KINDS` are new, so the replacement view sits under
the boot probe — a source of truth with no existence guarantee just
relocates the problem. All eight of its columns are contracted;
`pg_attribute` covers views as well as tables, so renaming a column in the
view definition is now a boot-time failure rather than a page-load
failure.

`every_verdict_axis_counts_toward_unhealthy` earned its keep: it refused
to compile until `missing_views` was wired into `is_healthy()` and
`total_issues()`.

## The original bug

`list_forecasts_handler` now ships two research rollups: `evidence_count`
(a scalar, not the `evidence` array — items carry full source text and
would multiply every list page's payload for a number the UI reduces to a
count anyway) and `agents_used` whole, since the Research card needs the
agent ids to price runs.

Both are wrapped in the `jsonb_typeof(...) = 'array'` guard used by
`ops::detect_ungrounded`, because `jsonb_array_length` *errors* on a
non-array instead of returning NULL, which would 500 the entire forecast
list.

The card prefers `evidence_count` and falls back to measuring the array,
so forecasts hydrated from the detail endpoint — and older API builds that
predate the count — still register.

## Mine / Shared on the Research card

The card's three forecast lists come from `list_forecasts` with no
`scope`, so they mix owned forecasts with ones shared with the operator.
Three chips now split them: `All`, `👤 Mine`, `📥 Shared`, dimmed and
non-interactive when a scope is empty.

Two details that matter more than they look:

- **An unknown identity admits everything.** `current_user_id` is `None`
  until the first successful auth; treating that as "nobody" would render
  an empty card, which is the failure mode this whole release is about.
- **The `All` count is counted, not summed.** With an unknown identity
  both sub-scopes admit every row, so `mine + shared` would double-count.

The empty state is scope-aware: "No research on forecasts you own — switch
to Shared" rather than telling an operator to go hire an agent when they
already have.

## Upgrade

Requires the API deploy. Migration 192 is registered in `run_migrations()`
and applies to an empty cluster. Nothing to do on the console side beyond
taking the build.

If the view is missing, the eight surfaces above 500 rather than serving
zeros — which is the intended failure mode, and strictly better than what
they did before.

## Known gaps

- **Cost is still estimated, not attributed.** `episodes` has no
  `forecast_id`, so a forecast's research cost is approximated as the sum
  over `agents_used` of one average run per agent — an honest lower bound,
  labelled `est.` in the UI. Attributing episodes to forecasts would make
  it exact without changing the visual shell.
- **The dead columns are not dropped.** `Agent`'s `SELECT` list in
  `agent-bestiary/memory/` still names all five. Dropping them is now
  mechanical rather than archaeology.
- **`ROLLUP_CONTRACTS` covers only these five.** Other cached counters —
  `fork_count`, `dreaming_credits_used` — have not been audited for the
  same defect.
- **Ecology now exposes measured per-agent cost on an anonymous route.**
  The key was always in that payload, just always zero. Consistent with
  public profiles and marketplace pricing, but it is a real change in
  what is public.
