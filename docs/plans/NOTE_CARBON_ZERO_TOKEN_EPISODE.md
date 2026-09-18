# Note — a `carbon_accountant` episode on 2026-09-17 with zero tokens and zero duration

**From:** the studio/UX session (flush timeout `9dd78615`, reference-flow guard
`ac39c763`, run history `28b00836`).
**To:** whoever owns `carbon_accountant` and `carbon.rs`.
**As of:** `1d9f40f0`.
**Status: an observation with candidate causes. I could not determine the cause
from the public endpoint alone and have not guessed at one.**

This bears directly on `1d9f40f0`, which concluded that this agent "has never
produced any" verified output. That conclusion looks right. This note says the
evidence base for it is now contaminated, and in a direction that makes the
agent look *better* than it is.

---

## 1. What is observable

`GET /api/agents/carbon_accountant/metrics`, unauthenticated, read repeatedly:

```json
"daily": [
  {"date":"2026-09-16","executions":1,"failures":1,"tokens":151790,"avg_time_ms":157246},
  {"date":"2026-09-17","executions":1,"failures":0,"tokens":0,"avg_time_ms":0}
],
"tool_usage": [{"tool_name":"web_search","count":18,"avg_duration_ms":520}],
"avg_loop_iterations": 5.0,
"recent_failures": [ /* the 2026-09-16 run only */ ]
```

The 2026-09-16 row is run `138632a9`, the one `1d9f40f0` analysed: 18 searches,
flush cut off at the 90s per-hop client timeout, zero-character reply.

**The 2026-09-17 row is the problem.** It records an execution that is not a
failure, consumed **0 tokens**, took **0 ms**, and added **0** to `tool_usage`
(the count is still 18, all of it attributable to the 16th).

## 2. What that rules out

**It is not `structured_output_trigger`.** That was my first hypothesis and it
is wrong, stated here so nobody spends time on it. A tool-loop bypass still
makes one LLM call and answers from memory, which costs tokens and wall-clock.
Zero of both means no model call happened on that path at all.

**It is not the cache-read skip.** `carbon.rs:1358` returns before the
`workspace_action_log` insert and before `dispatch_rabble_action`, so a skipped
run creates no episode. I checked this specifically because it was my second
hypothesis.

So something recorded an execution for this agent without executing a model.

## 3. Why it matters more than one stray row

Three consequences, in increasing order of seriousness:

1. **It halves the measured cost.** `avg_time_ms` across the two rows reads as
   ~78s. The only real run took 157s. The carbon path's latency is the open
   question behind the iteration-budget decision, and this is the series that
   answers it.
2. **It reads as a 50% success rate.** "2 executions, 1 failure" invites the
   conclusion that the agent works half the time. No run has ever resolved a
   single factor.
3. **It is the same class of error the agent is gated against**, arriving in
   the telemetry rather than the statement. A row that asserts an execution
   occurred, when the mechanism that would make it an execution never ran, is
   `tool_no_match`-as-clearance one layer up — and `1d9f40f0` is the commit
   that just finished arguing exactly this about the provenance stamp.

## 4. Where I would look

I have no database access from this session, so these are ordered by my guess
at likelihood, not by evidence:

- **Another surface invoked the agent.** The companion action path, `mcp.rs`
  `tools/call`, or A2A. An execution that fails before the LLM call but is
  recorded `success` with null token counts would present exactly like this.
  `failures: 0` is the part that needs explaining — something decided this was
  a success.
- **A production probe.** `1d9f40f0`'s own investigation read production. If
  anything in that path constructs an `ExecutionContext` and writes an episode,
  it would land here. Worth eliminating first because it is the cheapest to
  check and would mean there is no product defect at all.
- **Null coalescing in the metrics aggregate.** `tokens: 0` and
  `avg_time_ms: 0` may be `COALESCE(..., 0)` over NULL columns rather than
  measured zeroes. That would make this a *reporting* defect rather than a
  spurious episode — still worth fixing, because a NULL-duration execution
  displayed as 0 ms drags an average that people are using to make a decision.

The query that settles it: the `episodes` row for 2026-09-17, its
`execution_status`, `tokens`, `duration_ms`, `session_id`, and whether a
`workspace_action_log` row with `action_type = 'calculate_carbon'` exists with
a matching timestamp. If there is no action row, it did not come through
`calculate_carbon_handler` and the question becomes which surface it came
through.

## 5. What I would not do

- **Do not delete the row.** If a surface can write an execution with no model
  call, deleting the evidence leaves the mechanism.
- **Do not add a heuristic over `apply_result`.** `1d9f40f0` already drafted
  and discarded one for firing on correct behaviour, and that reasoning holds
  here: a genuine zero-cost execution may exist for some surface, and a check
  that flags it will get switched off.
- **Do not treat `tool_usage` as per-run.** It is a 30-day per-agent aggregate.
  It happens to be attributable to one run today only because there has been
  one real run. The moment there are two, "did *this* run search" stops being
  answerable from this endpoint — which is the same gap `1d9f40f0` names when
  it says earning the stamp "needs the tool-invocation count where the
  statement is built, and `dispatch_rabble_action` returns
  `Result<String, String>`".

## 6. The one thing worth fixing regardless of cause

`1d9f40f0` established that `enforce_from_grounding_map` assigns
`tool_no_match` to any empty sourced block as a proxy for having asked. The
studio now renders a `coverage: none` run as "nothing concluded" and states
that the stamp is a proxy rather than a receipt, which is true in either world.

But the receipt exists — `tool_invocations` is populated in the tool loop and
`/metrics` aggregates it. What is missing is a path from there to the handler.
If `dispatch_rabble_action` returned the tool-invocation count alongside the
reply text, `calculate_carbon_handler` could write it into `apply_result`, and
then:

- a run that searched 18 times and concluded nothing is distinguishable from a
  run that searched 0 times and concluded nothing, which is the distinction
  this whole thread has been about;
- `tool_no_match` becomes assertable rather than proxied, for this agent and
  every other agent with `Sourced` fields;
- and the row above would have been unambiguous on sight.

That is a change to a shared signature, so it is not mine to make unilaterally.
Flagging it as the highest-leverage fix available, not proposing to land it.
