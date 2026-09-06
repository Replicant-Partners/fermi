# Response — belt rung outcomes

> **Vocabulary note (superseded term).** Kept as written. "Belt" is no longer the
> platform's word for the row of checkpoints an artifact crosses — the payload
> fields are `checkpoints` / `checkpoint_route`, the assembler is
> `artifact_trace::checkpoints()`, and the harness is
> `scripts/break_checkpoint_contract.py`.

**To:** the trust surfaces UI team
**Re:** `UX_CONTRACT_belt_outcomes.md`
**Status:** **implemented.** All seven invariants are asserted, and each was
deliberately broken and watched go red before this was written
(`scripts/break_checkpoint_contract.py`, 7 breaks, 7 caught).

Three things landed that you did not ask for. Two are corrections to your
contract; one is a disclosure of a defect your spec made visible. All three are
below, and **the third changes what you should draw.**

---

## 1. The shape, as specified

`Outcome` is gone. Each rung now carries `decided` / `decided_absent` /
`recomputed` as siblings, exactly as you drew it. `decision_id` is an `i64`
(`gate_decisions.id`), so
`POST /api/gates/:gate_id/decisions/:decision_id/review` is reachable from the
artifact.

Both nullable fields are `skip_serializing_if = "Option::is_none"`, so the key
is **absent** rather than `null` — your "exactly one of" branch can be a key
check.

Every invariant you named is a test, by the name you gave it:

| # | invariant | where |
|---|---|---|
| 1 | every declared rung appears | `every_rung_says_whether_it_can_actually_refuse` |
| 2 | exactly one of `decided`/`decided_absent` | `every_rung_reports_exactly_one_way` + `Rung::reports_exactly_one_way` |
| 3 | `fires_before_artifact` iff the gate fires before the artifact | `the_absence_token_comes_from_the_gate_registry` |
| 4 | `retention_counted` iff `retention == Counted` | same |
| 5 | `recomputed` only where re-runnable | `the_declared_belt_asserts_nothing_about_an_episode` |
| 6 | exactly three verdicts | same, asserted against `gate_trust::DECISIONS` |
| 7 | `because` non-empty | `the_absence_token_comes_from_the_gate_registry` (>40 chars) |

3 and 4 are asserted **against the gate registry**, not a literal list, as you
asked. Breaking `decides_before_the_artifact` turns out to be a *compile* error
at every construction site, which is a stronger guarantee than the scan.

---

## 2. Correction: there is no fifth token, and `credit` was never the risk

We were about to add one, and we were wrong.

We had an `Outcome::NotApplicable` on the grounding rung reading *"this agent
declares no field contract, so this rung had nothing to grade."* That state
covers **192 of 256 recent episodes**, so folding it into `retained_but_absent`
would have put a finding on three-quarters of the corpus, and we drafted a fifth
token to carry it.

Then we read `src/handlers/execution.rs:470`. It **already** calls
`decided_for_episode(Gate::Grounding, Decision::Undetermined)` when the agent has
no contract. So once the recorder has run, that episode gets a real ledger row
reading `undetermined` — the gate ran and could not decide — which is precisely
what your third verdict is for, and what migration 221 predicts ~3,065 of.

`NotApplicable` was **compensating for an empty database**, not describing the
system. It has been deleted rather than migrated. Your four tokens are correct
and complete; please do not build a fifth.

**What this means for you:** `undetermined` is not a rare third case to handle
for completeness. On today's corpus it is the **majority reading** of the
grounding rung, and it will stay that way until the agent fleet is rewritten. It
deserves a real visual, not a de-emphasised one.

---

## 3. Addition: `substrate` — the axis the belt was wrongly carrying

This is the one that should change your layout.

The reason a belt rung was trying to say *"the author declared no field
contract"* is that **two different questions were being answered in one
diagram**:

| | axis 1 — **substrate membership** | axis 2 — **the belt** |
|---|---|---|
| about | the **agent** | this **artifact** |
| answers | is this thing declared onto the platform at all? | what did each gate decide about this output? |
| new field | `substrate` | `belt` (unchanged) |

A reader had to understand field contracts before they could read a checkpoint,
and an agent-level backlog was being rendered inside a per-artifact diagram.

New top-level object. **`legibility` and `declared` have moved inside it** — they
were loose keys beside the belt, which invited reading `legibility` without
`disposition`:

```jsonc
"substrate": {
  "disposition": "prune",          // prune | retrofit | legible
  "legibility":  { "legibility": "opaque" },
  "declared":    [],
  "because":     "This is test cruft, not an agent anyone is going to declare…"
}
```

**`disposition` is the field to branch on, and `prune` is the one that changes
what you draw.** 110 of 206 producing agents are `test_agent_*` fixtures awaiting
deletion — they are *not* retrofit targets. Rendering them beside real agents
makes the authoring backlog look twice its true size and buries the agents worth
fixing.

- `prune` — a fixture. Arguably should not appear in a user-facing list at all.
- `retrofit` — a real agent not yet declared. Its grounding rung will read
  `undetermined`. **This is the legacy state**, and it is authoring work owned by
  the agent's author, not a platform fault and not a pass.
- `legible` — fully declared. Every rung can say something specific.

The platform position, for your copy: legacy agents are not a degraded belt.
They are **outside the substrate**, with a named owner and a worklist. Not green,
not red — *not yet in the system.*

---

## 4. Disclosure: the belt may show rungs the artifact never passed

Your invariant 1 made us check something we had asserted and never measured, and
it is wrong.

**The two execute routes do not declare the same belt.**

| command | rungs |
|---|---|
| `agent.execute` | `credit`, `attachment`, `grounding`, `input_binding` — **4** |
| `agent.execute_stream` | `credit`, `grounding` — **2** |

`episode_trace_handler` builds `belt("agent.execute")` for every artifact. The
comment justifying that said *"both declare the same rungs and
`grounding_execute_coverage` holds them to it."* That was false — the test only
ever held them to both declaring *grounding*. Two comments in two files asserted
it. Both are corrected.

It is **not fixable in the handler**: `episodes` carries no route discriminator,
so which route an artifact travelled is not recoverable. Fixing it properly needs
a column on `episodes`.

So the payload now says so:

```jsonc
"belt_route": {
  "assumed": "agent.execute",
  "recoverable": false,
  "because": "`episodes` records no route discriminator… if this was a streamed
              artifact then `attachment` and `input_binding` are shown here and
              its route never had them."
}
```

We serve the **wider** belt deliberately — the opposite error silently drops two
real checkpoints for the majority of artifacts, and a belt that omits checkpoints
looks shorter and safer than it is. Both directions are wrong; this one is wrong
in the direction that shows more.

**What we would like from you:** if `belt_route.recoverable` is `false`, please
mark the belt as unverified in some low-key way. It is an unverified safety claim
and you are the only place a person will ever see it. Tell us if the column on
`episodes` is worth prioritising — it is the real fix and it is small.

---

## 5. Deploy state — read this before you test against a live server

**Migrations 219, 220 and 221 are registered and have not run.** They apply at
boot; the server has not restarted.

Until it does:

- `gate_decisions` has **0 rows**, so every rung will report `decided_absent`;
- the token will be `predates_retention` for the recording gates, because
  `min(decided_at)` is null and the honest answer for a gate that has recorded
  nothing is that everything predates it. That is per your own spec and it is
  correct, but it means **you will not see a single `decided` until deploy**;
- `assertion_verifications` has 0 rows for the same reason, so
  `routed[]` will be empty and the rejection-rate surface will read `unknown`.

This is not a defect and there is nothing to work around. The writers are wired
and verified: `gate_trust::decided` fires from five sites in
`handlers/execution.rs` plus `observatory.rs` and `workflows/publish_pipeline.rs`;
`spawn_gate_recorder` flushes every 15s from `api_server.rs:2571`;
`verification_queue::enqueue` is on both execute paths. It fills on first traffic
after restart.

---

## 6. Your client is currently broken, and it is a small fix

`templates/trace.html` reads `o.outcome === "graded"`. `Outcome::Graded` was
removed before this change and `Outcome` itself is now gone. The file has been
left alone on the strength of *"we will land the client change in the same
window"* — flagging it so the window is not a surprise.

The `since` heuristic you mentioned can be deleted now; `decided_absent.token`
replaces it.

---

## 7. Also shipped, from the earlier list

- **`GET /api/verification-queue`** — latest row per assertion, `state` derived
  from the verdict, plus a `tally` of `pending` / `settled` / `refuted`. Serves
  `settleable_verdicts` so you do not have to hardcode a parallel list.
- **`POST /api/verification-queue/:assertion_id/settle`** — appends, never
  updates. A second reviewer disagreeing appends again rather than overwriting.
  Returns **400** with a specific sentence for a missing citation on
  `human_sourced`, **404** for a claim nobody queued, and **500** only where the
  platform is at fault. A reviewer may write exactly `human_sourced`,
  `human_endorsed`, `rejected` — never `tool_verified` or `derived`, which claim
  a tool call reproduces the value, and never `pending_*`, which would let an
  item be cleared by re-queueing it.

That was the last dependency of the rejection rate. **"Nobody checked" and
"checked and fine" no longer render identically** — after deploy.
