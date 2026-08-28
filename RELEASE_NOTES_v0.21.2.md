# v0.21.2 — the card that never moved, and a latch released only on success

One session, one 141-event activity log, four defects. **Two were fixed in
`main` eleven hours after v0.21.1 was tagged and have been sitting unreleased
for six days.** Two were live in `main` and are fixed here.

The report was one sentence:

> *an agent gets assigned and hints get generated — but only a non-weather agent
> actually shows firing in the composer card*

The weather framing is the natural reading of what was on screen, and it is
wrong. That is what made it worth chasing.

---

## The session that found them

A Polymarket import: *Will the highest temperature in San Francisco be between
68-69°F on August 29?* Crowd price 34.5%.

Decomposition did its job. It detected `climate`, resolved `weather_oracle` as
the agent that declares that domain, and staged it on both meteorology drivers:

```
12:03:01  Staged 4 agents on 4 drivers: weather_oracle (2),
          sentiment_analyzer (1), entity_investigator (1).
12:03:01  ⚠ This base rate came from fermi, a generalist. weather_oracle
          declares the 'climate' domain and can measure it instead.
```

Then the operator ran them, and the composer showed this:

| driver | agent | invoked via | card said |
|---|---|---|---|
| `synoptic_pattern_aug29` | entity_investigator | **Assign Agent** | ✓ 1 findings (55s) |
| `synoptic_pattern_aug29` | weather_oracle | ▶ Run Now | ○ idle |
| `seasonal_climate_trend` | weather_oracle | ▶ Run Now | ○ idle |
| `microclimate_station_bias` | entity_investigator | ▶ Run Now | ○ idle |

Row four is the one that settles it. `entity_investigator` is not a weather
agent and it is also idle. The split is not the agent — it is **which button
started the run**.

---

## 1. ▶ Run Now left the card idle for the whole run

The driver card resolves its status chip with:

```rust
agent_runs.iter().find(|r| r.agent_name == bound_name)
```

Three code paths start a run. `assign_agent_to_driver` and
`run_pending_research` both push an `AgentExecution` row before firing.
`update_schedule_for_assigned_agent` — the ▶ Run Now button, which is the
*common* case once a forecast exists — called `fire_agent` and returned.

So the lookup found nothing and rendered `○ idle` for the entire run. The
activity log showed the agent starting, streaming findings and completing. The
place a human actually looks never changed.

Fixed in `e48d6601`. The row is reset rather than duplicated, because the card
takes the **first** match: a second row would be invisible while the stale
completed one kept showing `✓ N findings` throughout the re-run. `evidence_count`
resets too — carrying the previous count attributes an earlier answer's findings
to a run that has produced nothing yet.

**This fix already exists.** It was committed at 11:17 on Aug 22; v0.21.1 was
tagged at 00:31 the same day. The console correctly reported *"v0.21.1 — up to
date"* to an operator hitting a bug that had been fixed for six days.

---

## 2. A 429 was terminal, and the recovery drew from the bucket that had just refused

Two base-rate refreshes, 16 seconds apart, both dead:

```
12:05:15  ✗ weather_oracle_base_rate failed: ABW API: Rate limited —
          retry after 5s (after SSE HTTP 429: LLM rate limit exceeded
          (10/min). Retry after 9 seconds.)
12:05:31  ✗ weather_oracle_base_rate failed: … Retry after 11 seconds.
```

Three things are wrong in that one line, and all three are visible in it:

* **The console invented the delay.** `retry_after_secs: 5` was a literal. The
  server said 9. The console's own message contradicts the server's inside the
  same sentence.
* **Nothing waited.** The 5s it named was not a delay anything honoured; it was
  a terminal error with a number in it.
* **`(after SSE …)` is the fallback firing on a 429.** `/execute` and
  `/execute/stream` are both on ABW's `LLM_SPEND_ROUTES` and charge the same
  per-user bucket, so the non-streaming "recovery" drew a second token from a
  bucket that had just refused one — deepening the deficit for every sibling
  driver still in flight, and guaranteeing its own failure.

`b3867b90` added `abw_pacing`: a launch pacer that reserves against a local
model of the server's budget so a fan-out staggers instead of stampeding,
`retry_after_secs` which reads the server's stated delay from the body (ABW sets
no `Retry-After` header), and a bounded retry loop that classifies a 429 as a
statement about *when* rather than *whether*. The transport fallback is now
skipped for refusals — it is the one recovery that cannot work.

**Also already fixed, also unreleased.** Same six days.

---

## 3. The picker routed without the rung decomposition routes with

This one was live in `main`.

`routing::select_agent_for_driver_declared` is the ladder every selection site
shares. Its top rung is `declared` — the agent that claims the question's domain
on its own card, resolved from the roster rather than from a compile-time
`match` over four domains. That rung exists because two production weather
forecasts fell through to the generalist and came back as their own
climatological base rate.

`routing::select_agent_for_driver` is the same function with `declared`
hardcoded to `None`.

Only decomposition resolved the claimant. The other three sites — the research
panel, the picker's "Recommended" card, and the URL-ingest analyst fallback —
called the convenience arity. The comment above one of them read *"Same routing
the auto-assign path uses, so the picker's 'Recommended' card agrees with what
Fermi actually spawned"*. Same function; the disagreement had moved into the
argument list.

What that cost, on the same driver of the same forecast, ninety seconds apart:

```
12:03:01  Staged … weather_oracle on synoptic_pattern_aug29
12:04:18  🔬 Research panel for 'synoptic_pattern_aug29'
            — recommended: entity_investigator
12:04:23  Agent 'entity_investigator' assigned — researching now.
12:05:18  entity_investigator: "## DOMAIN MISMATCH NOTICE — I am the Entity
          Investigator. My expertise is corporate structures, ownership
          chains, regulatory risk … This is not an entity investigation.
          The 'entities' here are atmospheric phenomena."
```

73 credits for an agent to explain that it had been asked the wrong question —
by a console that had warned, two minutes earlier, that `weather_oracle`
declares `climate`.

The resolution is now one method, `CockpitState::declared_specialist_for`, and
all four sites call the wider arity with the result. `render_agent_picker` was
additionally gating on `agent_is_routable` ("can anything execute this id")
rather than `agent_is_assignable` ("may this be bound to a driver") — `e48d6601`
fixed six such predicates inside `impl CockpitState` and missed this one, which
is a free function, so the "Recommended" chip could name an agent that clicking
it would refuse.

`select_agent_for_driver` is no longer imported into `cockpit.rs`. Reaching for
the rung-less arity is now a visible edit.

---

## 4. The base-rate latch was released only on success

Also live in `main`, and the one with teeth.

`base_rate_producer` is a latch. `update_outside_rate` sets it before firing the
scoped "Update base rate" run; a completing agent that finds it set is diverted
into `apply_base_rate_only` instead of the normal path. Two defects, and the
second is only reachable because of the first:

1. **The guard asked the wrong question.** It read `base_rate_producer.is_some()`
   — *"is a base-rate refresh outstanding somewhere"* — where the question is
   *"is this completion that refresh"*.
2. **Only success released it.** `apply_base_rate_only` `take()`s the latch.
   Nothing on the failure path did.

So the 429 in §2 left the latch on. Three seconds later:

```
12:05:15  ✗ weather_oracle_base_rate failed: 429
12:05:18  ℹ Base-rate update: no parseable base rate in response.
             Existing base rate preserved.
12:05:18  ✓ entity_investigator_synoptic_pattern_aug29 complete
```

A driver run was fed to the base-rate extractor and reported that it had failed
to answer a question nobody asked it. The latch was then still set when the log
ended, with `weather_oracle_synoptic_pattern_aug29` in flight.

The confusing message is the benign symptom. `apply_base_rate_only` **writes**
when it can parse: any diverted run whose response happened to carry
`historical_frequency` would have overwritten the forecast's outside view — the
term every driver multiplies — with a number measured for one driver, and
stamped the wrong agent on its provenance. Nothing in the session hit that,
because `entity_investigator` returned prose.

Three changes:

* one definition of the tracking id, `base_rate_tracking_id()`, so the launch,
  the completion guard and the failure release cannot disagree about which run
  the latch refers to;
* the guard compares identities, so exactly one run can ever satisfy it and a
  leaked latch degrades to *"the base rate was not updated"* rather than to
  *"some other agent updated it"*;
* `mark_agent_failed` releases the latch and says what is now true of the
  **forecast**, not just of the run: `Base rate NOT updated — weather_oracle
  never ran. The outside view is unchanged, and every driver is a multiplier on
  it.`

A fourth, found while fixing the third: `apply_base_rate_only` located its run
row with `r.agent_name == tracking || r.base_agent_id == producer`. The fallback
was added because an earlier lookup missed, and it over-corrected. A declared
specialist is routinely on drivers *as well as* the base rate — `weather_oracle`
held three rows on this one forecast — and `find` returns the first match, so
the fallback marked a **driver** run completed and left the base-rate row
spinning. That is the bug it was written to fix, one row over. It is now an
exact match, which is total: `update_outside_rate` is the only site that sets
the latch and it always pushes exactly that name.

---

## The pattern worth naming

Three of the four are the same shape: **a check that could not tell two things
apart returned the answer that looked fine.**

* a card that cannot find a run row renders `idle`, not `unknown`;
* a router that cannot see the roster returns a confident recommendation, not an
  abstention;
* a latch that cannot tell which run completed diverts the wrong one, rather
  than declining to divert.

In each case the failing component produced its most reassuring output. That is
the direction the ratchets exist to catch, and two new ones catch these.

---

## New guards

Both are source scans over `cockpit.rs`, the established pattern here — see
`tests/execute_path_parity.rs` and `tests/gate_trust_coverage.rs`. A source scan
is a weaker instrument than a behavioural test and is chosen knowingly:
`CockpitState` needs a window, an async executor and a live ABW session to
instantiate, and `cockpit.rs` has no harness. What a scan can do is stop the
sites diverging again — which is the failure that actually happened, silently,
with every existing test passing.

`tests/agent_selection_parity.rs` — no selection site may use the rung-less
arity; every site passes a resolved claimant rather than `None` wearing the
longer name; no site gates on executability.

`tests/base_rate_latch_contract.rs` — the latch is never read as a bare boolean;
the tracking id has one definition; the failure path releases the latch and says
so; the run row is matched by identity, not by agent id.

Both were verified non-vacuous by reverting `cockpit.rs`: 1 of 3 and 4 of 4
fail against the code that shipped.

---

## Deploying this

Console-only. No migration, no card re-seed. The release workflow stamps the
version from the tag.

The two unreleased fixes (§1, §2) need nothing but the tag — they have been in
`main` since Aug 22 and Aug 25 respectively.

---

## Verification

```
fermi-console lib             385
agent_selection_parity          3  (new)
base_rate_latch_contract        4  (new)
cargo build --release -p fermi-console   green
```

The release profile is verified in a detached `git worktree` pinned to the exact
tagged commit, with `origin` re-fetched immediately beforehand — not in a
working tree, and not at `HEAD`. The next section is why.

## Two false starts, recorded

This tag failed twice before it built, both times on the same call site, and the
sequence is worth writing down because the second failure was caused by the fix
for the first.

A concurrent refactor of `src/assertions.rs` — authored in another session, on
this same branch — replaces the bare `(p5, p50, p95)` assertion value with a
`Claim` enum (`Numeric(Spread) | Literal(Value)`), so a non-numeric assertion is
retained verbatim rather than coerced. `cockpit.rs` holds the one console call
site.

**Attempt 1** migrated that call site to `Claim::as_spread()` while the refactor
was still *uncommitted*. Green in the working tree that had it, broken on every
clean checkout — which is what CI builds:
`no method named as_spread found for struct Spread`.

**Attempt 2** reverted the migration, and verified the revert properly: a
detached `git worktree` at the commit, `cargo build --release`, green. It failed
anyway. The refactor had been committed as `8b3ed2c6` during the eighteen
minutes that release build was running, so the worktree — created at the HEAD
from *before* the build — was checked against a base that no longer existed:
`no field p5 on type Claim`.

The migration is restored, and is now simply correct: `Claim` is in the tree.

The lesson is not "use a worktree", which attempt 2 did. It is that on a branch
with another session pushing to it, **the base must be re-fetched at the moment
of verification, and the worktree pinned to the exact commit being tagged** — not
to `HEAD`, which is a moving target. Both failures were the same error at
different scales: checking against a tree that was true when the check started
and false when it finished.

`src/assertions.rs` itself was never touched from this side.
