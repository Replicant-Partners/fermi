# UX handoff — the trust surfaces

**For:** the team reworking the front end.
**Status:** complete enough to build against. Thirteen endpoints, one vocabulary.
**Last change:** `UX_CONTRACT_belt_outcomes.md` is **implemented** — see
`UX_RESPONSE_belt_outcomes.md` for the reply, which contains two corrections to
that contract and one disclosure you should read before drawing the checkpoints.
Both of those documents were written while the row of checkpoints was still
called *the belt*; that metaphor is superseded and the payload field is
`checkpoints`.

> ### Read this first if you read nothing else
>
> **Two axes, and they must not be mixed.**
>
> * **`substrate`** is about the **agent** — is it declared onto the platform at
>   all? Its `disposition` is `prune` (a fixture awaiting deletion — 110 of 206
>   producing agents), `retrofit` (a real agent not yet declared — **this is the
>   legacy state**), or `legible`.
> * **`checkpoints`** is about **this artifact** — what did each gate decide?
>
> A legacy agent is **not a degraded route.** It is an agent that is not on the
> substrate yet, with a named owner and a worklist. Branch on
> `substrate.disposition` before you draw anything.
>
> Most of the fleet predates the contracts and will be **rewritten or deleted**.
> That is a known, scheduled, separate effort. It is not a platform fault, it is
> not a pass, and it should not read as either.

## Two directions, one structure

Your artifact-trace request was right, and it named the thing that was missing
rather than a styling problem:

> **A loop is a path an artifact takes. A gate is a checkpoint on that path.**

So there are now two ways into the same model, and which one a screen uses depends
only on what the reader came for.

**Population-first — *is the machine working?*** The operator's question.

```
GET /api/loops                             six loops, doors, caveats
GET /api/loops/actions                     doors alone, no DB walk
GET /api/loops/:loop_id                    one loop
GET /api/gates                             seven gates, three readings
GET /api/gates/:gate_id/decisions          what it refused, and who judged that
GET /api/evaluators                        six self-checks, with remedies
GET /api/declarations                      why everything else says `unknown`
```

**Artifact-first — *what happened to this thing?*** Everyone else's question.

```
GET /api/episodes/:episode_id/trace        one artifact, its checks ← NEW
GET /api/observatory/agents/:id/loops       one agent's chain
GET /api/agents/:id/coordination-notes      Loop 3 → Loop 1, per agent
```

**Write side — *a person acting.*** Two, and both record rather than override.

```
POST /api/gates/:gate_id/decisions/:decision_id/review
POST /api/observatory/hitl/:event_id/action                 (already existed)
```

Every one of these carries the same `reading` vocabulary and the same two extra
parts — `doors` (what a person can do) and `caveats` (what a tick does not mean).
**Branch once, reuse everywhere.** That is not a coincidence: `src/surface.rs`
declares the shared parts and one router contract checks every declared door
actually exists, so a door that 404s cannot ship.

---

## The one idea

Every one of these domains reports on machinery that **is usually empty**, and
an empty panel has three completely different meanings:

| reading | means | render as |
|---|---|---|
| `idle` | correctly empty. Nothing that should have happened has failed to | calm, neutral |
| `fault` | something should have happened and did not | a finding |
| `unknown` | **no contract can say.** Not healthy, not broken | explicitly indeterminate |

**`unknown` is not a pass, and it is not a failure.** It is the state where the
platform is telling you it does not know, and the single most important thing
this UI can do is render it as its own third thing rather than picking a side.
Every previous version of these screens picked a side, and every time it picked
wrong the effect was that nobody looked.

There is a corollary you can rely on: **you will never have to guess.** Every
payload carries a `reading` and a human sentence for every empty thing. If you
find yourself about to render a bare `0`, the API has failed and we want to hear
about it.

---

## `GET /api/loops`

Six declared feedback loops, each a chain of stages.

```
{
  "tally":  { "total": 6, "turning": 2, "stalled_by_fault": 0,
              "stalled_idle": 0, "no_reading": 4, "unreadable": 0 },
  "loops":  [ … ],
  "vocabulary": { … },
  "contract": "Never render a bare zero. …"
}
```

### The header has five buckets, not one number

Do not collapse them. "2 of 6 turning" invites the reader to conclude four are
broken; "0 stalled" invites the opposite. Both are wrong right now.

| bucket | means |
|---|---|
| `turning` | every stage produced |
| `stalled_by_fault` | stopped, and the reason is in the code |
| `stalled_idle` | stopped, correctly — nothing has had occasion |
| `no_reading` | stopped, and no contract can say why |
| `unreadable` | a probe did not run, so the chain supports no verdict |

They partition the set; a test asserts they sum to `total`. **`no_reading` and
`unreadable` must not be coloured as either good or bad.**

### Each loop

```
{
  "id": "loop2",
  "name": "Human-gated behavioural correction",
  "claim": "Agent behaviour aligns with human judgement on anomalous cases, …",
  "status": "stalled",            // turning | stalled | unmeasured
  "stops_at": "anomaly",
  "reason": "unobserved",
  "reading": "unknown",
  "detail": "Human-gated behavioural correction stops at `anomaly` …",
  "stages":   [ … ],
  "outcomes": [ … ]
}
```

* **`claim`** is what the architecture asserts this loop achieves. Show it. A
  stalled loop should display the claim it is failing, not just a row count.
* **`detail`** is a written sentence. Prefer it over composing your own from the
  fields — it is generated to name the loop and the link together.
* **`reason`** is a token from a closed set (served in `vocabulary.stall_reason`).
  You may branch on it, but `reading` is the field to colour by.

### Each stage

```
{
  "id": "reviewed",
  "what": "a reviewer acts on it",
  "writer": "handlers::observatory::record_hitl_action_handler",
  "trigger_label": "manual",
  "rows": 0,
  "measured": true,
  "is_first_empty": false,
  "action": { "subject": "loop2.reviewed", "method": "POST",
              "path": "/api/observatory/hitl/:event_id/action",
              "does": "Record a reviewer's decision on one anomaly: …",
              "why_manual": "The correction is a judgement about whether …" }
}
```

Three rules, and they are the ones we would ask you to hold most firmly:

1. **`measured: false` means show nothing, not zero.** `rows` will be `-1`. That
   is a sentinel, never a value.
2. **Highlight exactly the stage with `is_first_empty: true`.** Everything below
   it is empty *because of* it. A screen that flags all four turns one finding
   into four, which is how these reports stopped being read.
3. **`trigger_label: "nothing_calls_it"`** is a finding in its own right and
   should read as one even when the loop's own `reading` is something else — it
   means no code path invokes that writer at all.

`trigger_label` values: `request`, `sweeper`, `upstream`, `manual`, `prompted`,
`nothing_calls_it`.

`prompted` is worth a note: it means *a prompt asks an agent to do this and the
agent decides*. A `prompted` stage at zero has two indistinguishable causes — the
prompt has not run, or the model declined. That is why its reason is
`awaiting_agent` and its reading is `unknown`.

### `outcomes` — and please show `does_not_show`

```
{ "stage": "scored",
  "proposition": "The per-agent calibration signal takes different values for …",
  "does_not_show": "Nothing about whether any agent is well calibrated, …",
  "declared_gap": "`attribution::counterfactual` already computes …" }
```

This is the part we most need help with. Every check we run is **narrower than
the claim it serves.** `proposition` is what was actually verified;
`does_not_show` is what a green tick here fails to establish.

If a screen renders a tick against `claim`, it is lying, and it is the kind of
lie that is very hard to notice. Please surface `does_not_show` wherever a tick
appears — a tooltip is fine, silence is not.

`declared_gap` present means: we know this is a finding, we cannot fix it yet,
and here is what would clear it. Render it as an acknowledged debt, not a
failure.

---

## `GET /api/loops/actions`

The doors alone, with no database walk. **Build your buttons from this at
startup** — you do not need six `count(*)` queries to know what actions exist.

Deliberately separate from the loop payload: the actions are the same whether or
not a loop is turning, and a surface that only showed a door when the queue was
non-empty would hide the door precisely when someone wanted to ask why the queue
was empty.

Every entry carries `why_manual`, and it is required by a test. **Show it.** A
reviewer deciding whether a queue is worth working needs the argument, not just
the button. Four doors exist today; two of them are Loop 2's review queue, which
has never been used.

---

## `GET /api/loops/:loop_id`

One loop, identical shape to an element of `loops`. A 404 lists the declared loop
ids, so a wrong request tells you what you could have asked for.

---

## `GET /api/gates`

Same pattern, different domain. A gate is a control that refuses things.

```
{ "tally":   { "total": 5, "discriminating": …, "inverted": …,
               "never_refused": …, "unexercised": … },
  "gates":   [ … ],
  "doors":   [],
  "caveats": [ … ] }
```

### The counters are a trap and the `token` is the way out

`approved: 0, refused: 0` and `approved: 40, refused: 0` are **both** "no
refusals" and they mean opposite things — a control nobody has exercised, and one
that has run forty times and stopped nothing. The gate audit exists because a
screen rendered the counters and left a reader to notice the difference.

| `token` | `reading` | means |
|---|---|---|
| `discriminating` | `idle` | has both approved and refused |
| `refuses_everything` | `fault` | asked, and approved nothing. Inverted control |
| `never_asked` | `unknown` | not exercised. **Not a pass** |
| `admits_everything` | `unknown` | run, and refused nothing. Reported, never asserted |

Two tokens share `unknown` and mean different things, so **branch on `token`,
colour on `reading`**, and show the matching entry from `caveats`.

### `since` — the field that stops a lie

`"boot"` means the counters are process-local and reset on restart. `"ledger"`
means a durable record backs them. **A gate with `since: "boot"` and `refused: 0`
has not "never refused anything" — it has not refused anything since the last
deploy.** Please put that in the UI text, not only in a tooltip.

### `doors` — no longer empty, and still not an override

This list was `[]`, and the emptiness was a finding we asked you to render. It now
has one entry, on **two of the seven gates**, and the distinction matters enough
to state before the payload:

* there is still **no override**. Nothing re-runs a gate, reverses a decision, or
  admits what one refused. A gate a person can wave through is not much of a gate
  and that half of the old note stands;
* what was missing was a **judgement**. Every reading on the gate board is
  computed from approve/refuse *counts*, and `refuses_everything` only catches the
  extreme — asked, and approved nothing. A gate that approves 90% of what it sees
  and refuses the other 10% **wrongly** reads `discriminating`, which this board
  renders as the healthy state, and every counter in the platform agrees with it.
  Correctness is not a property of a count. A reviewer is not a convenience on top
  of the measurement; it is the only instrument that can see that failure.

So please do not label this control "override" or "approve". It records an opinion
about something that already happened.

**Only `coherence` and `admission` carry the door**, because only those two are
`Retention::Recorded` and write a decision a review can point at. The other five
are in-memory counters whose individual decisions do not survive the process, so
there is nothing to review — and `/api/gates/:gate_id/decisions` filters `doors`
to the gate you asked about, so a `rate_limit` screen correctly gets `[]`. Render
that empty case with the reason rather than hiding the section: a reviewer shown
no control on `rate_limit` should be told its decisions were never recorded, not
left to conclude it has never refused anything.

## `GET /api/gates/:gate_id/decisions`

What this gate actually refused, from the durable ledger, **and what anybody has
said about it.** Refusals are ordered first — what was stopped is what a reader
came for, and an approval stream is the wrong thing to page through.

```
{ "gate": "coherence", "ledger_total": 0,
  "decisions": [ { "id": 41,
                   "decision": "refused", "subject": "…", "reason": "…",
                   "decided_at": "…",
                   "review": { "verdict": "overturned", "rationale": "…",
                               "actor": "…", "actor_kind": "human",
                               "reviewed_at": "…" } } ],
  "reading": "unknown", "detail": "…",
  "review": { "standing": { "standing": "unreviewed", "decisions": 400 },
              "reading": "unknown", "token": "unreviewed" },
  "review_error": null,
  "doors": [ … ], "caveats": [ … ] }
```

`id` is new and it is the handle the review POST needs. `review` on a decision is
`null` when nobody has judged it — **which is not the same as nobody having found
anything wrong with it**, and those two must not render the same.

### Three independent readings on one screen

This is the part most likely to be flattened by accident. There are three
different `reading` fields in play and they answer three different questions:

| where | question |
|---|---|
| `/api/gates` → `reading` | does this gate discriminate? (from counts) |
| here → `reading` | does `since: "ledger"` mean anything? (are there rows?) |
| here → `review.reading` | **has anybody said the decisions were right?** |

A gate can be `discriminating`, `idle` on the ledger, and `unreviewed` all at
once, and all three are true. Please do not reduce them to one dot.

### `review.standing` — five states, three readings

| `token` | reading | means |
|---|---|---|
| `has_overturned` | `fault` | **at least one decision was judged wrong.** The finding |
| `inconclusive` | `unknown` | reviewed, and *not one* review could reach a verdict. A finding about the **ledger**: `reason` is not recording enough to judge a decision from |
| `unreviewed` | `unknown` | decisions on file, none reviewed. **Not a pass** — this is where every gate starts and where the platform sat for its whole life |
| `nothing_to_review` | `unknown` | the ledger is empty, so nothing could have been reviewed. Says nothing about the gate |
| `all_upheld` | `idle` | reviewed, nothing overturned, at least one judged |

The tokens are deliberately **not** the same words as the verdicts — `all_upheld`,
not `upheld`. A standing is a statement about a *gate*; a verdict is a statement
about one *decision*, and a client branching on `"overturned"` could not tell
which it had been handed.

`review_error` is non-null only if the column holds a verdict token our own types
do not know. That is a platform defect on our side, not an absence of reviews, and
it is reported rather than folded into a bucket.

## `POST /api/gates/:gate_id/decisions/:decision_id/review`

```
{ "verdict": "upheld" | "overturned" | "unclear",
  "rationale": "…",                     // REQUIRED for overturned
  "actor_kind": "human" | "tool" | "platform",   // default human
  "evidence": { … } }                   // optional, free JSON
```

Three things the form has to get right:

1. **`overturned` requires a rationale, and whitespace does not count.** The
   database enforces it and we deliberately did not duplicate the rule in Rust, so
   a missing one comes back as a **400 with a sentence you can show verbatim**.
   Please validate client-side too for the round trip, but the 400 text is written
   to be displayed.
2. **`upheld` requires nothing, on purpose.** Do not add a required-reason field
   to it for symmetry. Making the routine confirmation as expensive as the finding
   means nobody reviews the routine decisions, and then the *denominator* is gone
   — "3 overturned" and "3 overturned of 400 reviewed" are different findings and
   only the second is actionable. A one-click **Upheld** is the design.
3. **`unclear` is a real answer and should be as easy to press as the other two.**
   It means *the record does not say enough to judge this*. If reviewers cannot
   say that, they will pick one of the other two and we will have manufactured
   agreement. Every review coming back `unclear` is how we learn that
   `gate_decisions.reason` needs to carry more.

**Append-only.** Reviewing the same decision twice does not edit the first review;
the newest is the current verdict and the earlier ones stay on file. Two reviewers
disagreeing about one refusal is the most informative thing this table can hold,
so if you build a history view, that is what it is for.

**Attribution is the acting human.** Under admin impersonation the review is
recorded against the *admin*, not the user being viewed as — it is an audit
record and impersonation must not launder a judgement. Worth a line in the UI so
nobody is surprised.

Errors, all with displayable text: `404` unknown gate / no such decision / gate
has no review door; `400` missing rationale, or the decision belongs to a
different gate; `500` only if our CHECK and our types have drifted, which is ours.

### What the ledger reading means

**`reading` here is about the ledger, not about the gate.** It answers one
question: does `since: "ledger"` on the gate board mean anything?

| reading | means |
|---|---|
| `idle` | recorded, asked, and the rows are there. The durability claim holds |
| `fault` | **recorded, asked, and the ledger is empty.** The board is telling you these counters survive a restart and they do not |
| `unknown` | either not asked yet, or counted-in-memory-only, which never claimed durability |

`gate_decisions` holds **0 rows** today. Migration 214 created it and until it ran
the platform had a record of every request it served and none of any it refused —
so this is exactly the check that would have caught that, pointed at the table
built to fix it.

---

## `GET /api/episodes/:episode_id/trace` — the one you asked for

One artifact, and the checkpoints it passed. **This is the inversion**: the
primary object is the episode, and the loops are the routes it can take.

```
{ "episode_id": "…", "parent_episode_id": null,
  "agent":  { "id": "…", "name": "prey_locator" },
  "model":  { "model_used": "…", "provider_used": "…",
              "persona_version_at_write": 3 },
  "corpus_eligible": true,
  "at": "2026-08-…",

  "input":  { "query": "…" },
  "hashes": { "algorithm": "sha256",
              "input":  "sha256:…",
              "output": "sha256:…",
              "output_grounded": "sha256:…",
              "enforcement_changed_the_bytes": true },

  "substrate": { "disposition": "retrofit",
                 "legibility": { "legibility": "partial", … },
                 "declared": ["ports"],
                 "because": "`prey_locator` is a real agent that has not been…" },

  "checkpoint_route": { "assumed": "agent.execute", "recoverable": false,
                        "because": "`episodes` records no route discriminator…" },

  "checkpoints": [ { "rung": "credit", "clock": "invocation",
              "enforcement": "control", "why_not_control": null,
              "refuses": "an action whose principal cannot pay for it",
              "site": "handlers::execution, gas::charge_gas",
              "decided_absent": { "token": "fires_before_artifact",
                                  "because": "…" } },
            { "rung": "grounding", "clock": "invocation",
              "enforcement": "metric",
              "why_not_control": "`enforce` mutates a local doc that is dropped…",
              "decided": { "decision": "approved", "reason": null,
                           "at": "2026-08-28T10:07:51Z", "decision_id": 4412 },
              "recomputed": { "fields": 12, "violations": 2 } } ],

  "fields": [ { "name": "intercept.bearing_deg", "value": 137,
                "grade": "unavailable_no_tool_source", "strength": 0,
                "settleable_by": null } ],
  "floor": "unavailable_no_tool_source", "floor_strength": 0,

  "routed": [ { "assertion_id": "…", "verdict": "pending_human_check",
                "actor": "grounding_contract", "actor_kind": "platform",
                "citation": null, "evidence": { "path": "…", "claimed": 137 },
                "at": "…" } ],

  "reading": "fault", "token": "violations",
  "silence": { "silence": "unresolved" }, "owner": "platform",
  "caveats": [ … ] }
```

### The checkpoints are the drawing

`checkpoints[]` in order **is** the row of checkpoints. Two fields decide how each
one renders and neither is optional:

* **`enforcement`** — `control` means it can refuse; `metric` means it only
  records. **Please draw these differently.** A checkpoint drawn identically
  whether or not it can stop anything is a diagram that lies about the platform's
  safety properties — and grounding on `/execute` is currently a `metric`, with
  `why_not_control` carrying our own words about why.
* **`decided` or `decided_absent`** — **exactly one is present, never both,
  never neither.** Both are omitted rather than `null` when absent, so a key
  check is a safe branch.

`decided` is what the **ledger** recorded for this artifact:

| field | |
|---|---|
| `decision` | `approved` \| `refused` \| `undetermined` — exactly three |
| `reason` | verbatim; `null` on approvals by design |
| `at` | when the gate decided, not when the row landed |
| `decision_id` | `gate_decisions.id`, so the review POST is reachable from here |

**`undetermined` is not an edge case.** It means the gate ran and *could not
decide*, and on today's corpus it is the **majority reading** of the grounding
rung — an agent with no field contract has nothing to grade. Please give it a
real third visual. Folding it into either neighbour is how an absent check
becomes indistinguishable from a passing one.

`decided_absent` is why there is no row, as a closed set of four:

| token | permanent? | a finding? |
|---|---|---|
| `fires_before_artifact` | **yes, by design** — `credit`, `rate_limit` | **no.** Not a gap; not a debt |
| `retention_counted` | no — a decision could change it | no. A declared design choice |
| `predates_retention` | for this artifact, yes | no. Nothing is backfilled |
| `retained_but_absent` | no | **yes — the only one of the four** |

`because` is always present and is the sentence a person reads. An unrecognised
token should render as `unknown`, never healthy.

* **`recomputed`** — an **independent axis**, non-null only where the contract can
  be re-run (`grounding` today). **Do not reconcile it with `decided`.** A
  recorded `approved` beside a recomputed `2 violations` means the contract was
  tightened after the episode ran, and it is the only finding the platform can
  produce about its own drift. It survives only unreconciled — which is why there
  is deliberately no `agrees` boolean.

We list every declared rung even when we can say nothing about it — a route that
drops the checkpoints it cannot report on looks shorter and safer than it is.

### `substrate` — the agent, not the artifact. Branch on this first.

**This is the field that tells you whether the checkpoints are worth drawing at
all**, and it is a different question from anything on them.

Until now a rung could say *"this agent declares no field contract, so this
rung had nothing to grade"* — which is an **agent-level backlog rendered inside a
per-artifact diagram**, and it forced a reader to understand field contracts
before they could read a checkpoint. That is gone. The question moved here.

`disposition` has three values and they want different treatment:

| | what it is | what to do |
|---|---|---|
| `prune` | Test cruft. **110 of 206 producing agents** are `test_agent_*` fixtures awaiting deletion | Arguably do not show these to a user at all. They are not a retrofit target and counting them makes the real backlog look twice its size |
| `retrofit` | A real agent not yet declared onto the substrate. **This is the legacy state.** Its grounding rung reads `undetermined` | Show it as *not yet on the substrate* — authoring work owned by the agent's author. **Not a platform fault, and not a pass** |
| `legible` | Every rung on the declaration ladder present | The trace can say something specific about every checkpoint |

The platform's position, which you are welcome to quote: **legacy agents are not
a degraded route. They are outside the substrate**, with a named owner and a
worklist. Not green, not red — not yet in the system.

`legibility` and `declared` now live inside this object. They used to be loose
keys beside `checkpoints`, which invited reading `legibility` without
`disposition` and putting a fixture on somebody's worklist.

### `checkpoint_route` — the trace may show rungs this artifact never passed

An honest disclosure rather than a feature, and we would rather you knew.

The two routes that persist an episode **do not declare the same checkpoints**:

| command | rungs |
|---|---|
| `agent.execute` | `credit`, `attachment`, `grounding`, `input_binding` — **4** |
| `agent.execute_stream` | `credit`, `grounding` — **2** |

The trace builds `agent.execute`'s checkpoints for every artifact. A comment in the
handler claimed the two declared the same rungs and that either was therefore
correct; it was false, and asserting your invariant 1 is what measured it.

It is **not fixable in the handler** — `episodes` carries no route discriminator,
so which route an artifact travelled is not recoverable. We serve the wider route
deliberately: the opposite error drops two real checkpoints for the majority of
artifacts. Both directions are wrong; this one is wrong in the direction that
shows more.

**What we would like:** if `recoverable` is `false`, mark the route as unverified
in some low-key way. It is an unverified safety claim and your screen is the only
place a person will ever see it. Tell us if a route column on `episodes` is worth
prioritising — it is the real fix and it is small.

### `fields[]` — read `strength`, not `grade`

`value` is **what the agent actually claimed, never stripped.** You asked us not
to null it and we did not: it survives because enforcement runs on a copy. It is
the only evidence that could ever answer which model fabricates what, and a null
cannot be labelled.

**`strength` is the number to render, not `grade`.** `tool_no_match` sorts above
`unavailable_no_tool_source` and **both are strength 0** — different words for the
same amount of reliance. We made exactly that mistake in our own measurement probe
and caught it late, so the field is served to stop you repeating it.

| strength | means |
|---|---|
| 2 | reproducible — run the tool, apply the transform, or follow the citation |
| 1 | a judgement. Legitimate, and not a retrieval |
| 0 | nothing to rely on yet |

`floor` is the document's weakest link. **`settleable_by: null` is two things at
once** — a work item for a person *and* a prioritised request for the data
integration that would close it. Please render it as both.

### The reading, and the state you will see most

| token | reading | means |
|---|---|---|
| `violations` | `fault` | the contract found a field that could have no source, populated anyway. **The thing a reader came for** |
| `checked_clean` | `idle` | the fields under contract were graded and none violated. Narrow — see the caveat |
| `nothing_checked` | `unknown` | this agent declares no field contract, so no rung could grade anything |

Measured over the 256 most recent episodes with a retained response:

```
nothing_checked   192      owner: agent_author   172
checked_clean      54      owner: no_one          54
violations         10      owner: platform        30
```

**`nothing_checked` is the default screen.** 75% of traces, and it must not look
like an error page or a spinner. It arrives with `owner: "agent_author"`, a
`silence.rung` naming the cheapest missing declaration, and
`/api/declarations` behind it for the full worklist — so there is a real answer to
draw, not a blank.

**`owner` is the field to build the action on.** Three values and only one is
ours: `platform` (our backlog), `agent_author` (a declaration the agent has not
made), `no_one` (nothing to do). Getting this wrong does not produce a wrong
number, it produces a wrong backlog.

### There are real anomalies in there today

You asked to see *a sourced anomaly you can correct*. Re-running the contract over
retained responses finds **10 violations** — `prey_locator` (9) and `enemy_sensor`
(1) — each with a named agent, a named field, and the claimed value retained.
**None of them was recorded when it happened**: `episodes.tags` carries
`grounding:violations` on 0 rows, because the contract was not wired to those
paths when those episodes ran. Migration 199's retention of `response_text` is why
they are recoverable at all.

So the trace has content on arrival. Not much, and it is honest about that.

### `hashes` — there, and honest about what they are not

Four digests, **computed from retained text on every read rather than stored.**
`query` and `response_text` are both kept (the latter since migration 199,
deliberately), so a digest of them is a pure function of what we already hold —
and a computed digest **cannot drift from the text it claims to describe**, which a
stored one can.

| field | over |
|---|---|
| `input` | the query as given to the agent |
| `output` | the response **verbatim**, before grounding touched it |
| `output_grounded` | the document **after** enforcement. `null` when there was no document, or no contract |
| `enforcement_changed_the_bytes` | did enforcement alter the document at all |

**What they can do:** tell you two episodes got the identical input, or produced
the identical output — that is the drift and determinism question. Give a reviewer a
stable handle for *this exact text*, so a verdict recorded against it can be
re-checked rather than trusted.

**What they cannot do, and this corrects the request:** verify a seam by equality
with a parent's output. A delegated child does not receive its parent's output
verbatim — it receives a **prompt built around** the task, so the hashes will
differ for entirely correct runs. The place equality *would* hold is the envelope
payload in the delegation hop, which is passed through unchanged and which nothing
hashes yet. That is the honest next step for a seam check and it is separate work.

⚠️ **`enforcement_changed_the_bytes` is not "fabrication was stripped."** We named
it that way first and a live cross-check against the contract's own violation count
disagreed on **21 episodes** — `weather_oracle` and `enemy_sensor` responses where
the bytes changed and the violation count was **zero**. `enforce` does two things:
it nulls a refused field (a finding) *and* it stamps `<block>_provenance` siblings
onto the document (bookkeeping, on every contracted response). A digest cannot tell
those apart. **For "was a claim removed", read the violation count on the grounding
rung** — the contract knows, and that is the one implementation of the question.

### `parent_episode_id` — the writer was never missing

We told you the column existed and nothing wrote it. That was wrong, and the real
answer matters for what you build: `tools_legacy` **does** write it, both execute
paths populate the context that feeds it, and the chain is thin (4 of 3,576)
because **delegation is rare** — not because the plumbing is absent.

Four of the ten places that could name a parent pass `None`, and all four are
correct: they persist no episode of their own, so there is nothing for a child to
point at. Three said so; one did not, and that silence was indistinguishable from
an oversight. It says so now, and a scan
(`tests/episode_lineage_coverage.rs`) requires every future one to.

**So the correction-chain screen is buildable and will be nearly empty**, and the
reason is a product fact rather than a missing field. If you want it populated, the
lever is delegation volume.

### Non-numeric claims now queue — the Antaxius case is covered

We said this was the remaining gap and that it needed a schema change. It is done.

`taxonomy.order = "Coleoptera"` — the bush-cricket `Antaxius beieri` reported as a
longhorn beetle, every check passing because the field was present, non-null,
correctly typed and declared sourced — was the claim **most** worth verifying and
the one shape the queue could not hold, because an assertion's value was a
numeric spread. There is now a fourth assertion kind, `fact`, and a claim can carry
any JSON.

For you that means `routed[]` can contain a string claim, and it routes the same
way everything else does:

```
{ "assertion_id": "…", "verdict": "pending_tool_check",
  "actor_kind": "platform",
  "evidence": { "path": "taxonomy.order", "claimed": "Coleoptera",
                "settleable_by": "gbif_lookup" } }
```

`claimed` is the value **verbatim**, including its quotes — `"2.4"` and `2.4` are
different claims and a reviewer has to see which was made.

**No migration.** The stored representation is unchanged, so nothing that reads
`episodes.assertions` today needs updating, and a test asserts the exact stored
bytes still round-trip.

### The checkpoints now record a real outcome for grounding

The last item on your list. **The column alone would not have worked**, and the
reason is worth having:

> Every per-episode gate was `Retention::Counted`, and both `Recorded` gates are
> not per-episode. `coherence` fires on an AgentWide correction and `admission` at
> publish; `grounding`, `credit`, `rate_limit`, `attachment`, `input_binding` and
> `output_schema` wrote no ledger row at all.

So `episode_id` would have been NULL on every row that would ever exist, while
making `not_recorded` **look** solved. The blocker was retention, not the key.

Both landed, **and the trace now reads them.** For a while it did not: the
column and the promotion shipped while `artifact_trace::checkpoints()` still
hardcoded an absence on every rung, so 220 and 221 had no observable effect. You
caught that. It is fixed — the handler queries `gate_decisions WHERE episode_id =
$1`, folds the result over the checkpoints, and carries `decision_id` so you can
review from the artifact.

That distinction is the interesting part for a screen. The trace already re-runs
the contract over the retained response, so it can show per-field grades without
any ledger. What it could not show is **what the gate decided at the time** — and
those differ: re-running the contract today finds 10 violations that were never
recorded, because the contract was not wired to those paths when the episodes ran.
A recorded `approved` beside a recomputed `2 violations` is not a contradiction;
it is the contract having been tightened afterwards, and it is the only way to see
that from outside.

**A new door comes with it.** `gate_api::a_review_door_only_exists_where_the
_decisions_do` asserts both directions, so promoting grounding required giving it
a review door — a ledger nobody can judge is the state this work exists to end.
`grounding` now appears in `doors` on `/api/gates/grounding/decisions`, and its
`why_manual` is the `Antaxius beieri` argument: the contract asserts what *could*
have supplied a field, not what did.

Two things not to be surprised by:

* **`credit` and `rate_limit` will always have `episode_id: null`.** They decide
  whether to run at all, before any artifact exists and possibly instead of one.
  That is correct and final, not a gap to be filled.
* **The ledger starts empty.** `grounding` records from the next execute onward;
  nothing is backfilled.

### What it still does not have

* **`input_binding`, `attachment` and `output_schema` are still `Counted`**, so
  their rungs read `decided_absent.token: "retention_counted"`. Each is now a
  one-line change plus a door, and the pattern is established — but each is also
  a decision about durable write volume, and we would rather make them one at a
  time with a reason than promote the set. The token renders that honestly and
  updates itself the day we promote one.
* **A route discriminator on `episodes`** — see `checkpoint_route` above.

---

## `GET /api/verification-queue`

Claims awaiting a verdict, and the last dependency of the rejection rate. Until
something could write a verdict, **"nobody checked" and "checked and fine"
rendered identically.**

```
{ "items": [ { "assertion_id": "…", "episode_id": "…",
               "verdict": "pending_human_check", "state": "pending",
               "citation": null, "actor": "grounding_contract",
               "actor_kind": "platform", "evidence": { … }, "at": "…" } ],
  "tally": { "pending": 12, "settled": 3, "refuted": 1 },
  "settleable_verdicts": ["human_sourced", "human_endorsed", "rejected"],
  "reading": "idle", "detail": "…", "contract": "…" }
```

`state` is **derived** from the verdict, not stored: anything `pending_*` is
awaiting a verdict, everything else is settled. The log is append-only, so this
is the *latest* row per assertion and the earlier ones remain — two reviewers
disagreeing about one claim is a disagreement, not a correction.

`refuted` is the **numerator** of the rejection rate; the denominator is
`settled`. A rate without it is a lie at low volume, so please show both.

`settleable_verdicts` is served rather than hardcoded on your side. Copying it is
copying a declaration; inventing a parallel list is how the two drift.

## `POST /api/verification-queue/:assertion_id/settle`

```
{ "verdict": "human_sourced", "source_citation": "…", "evidence": { … } }
```

**Appends; never updates.** The earlier pending row remains, so the claim reads
as queued *and then* settled — and a second reviewer disagreeing appends again
rather than overwriting the first.

**A reviewer may write exactly three verdicts**, and the refusals are worth
surfacing in your copy rather than as a generic error:

* `human_sourced` — requires a `source_citation`, enforced by Postgres. It scores
  as high as `tool_verified` in the provenance ladder **because** someone else can
  follow the citation to the same source. The citation is what earns the score.
* `human_endorsed` — the honest uncited alternative, at the strength of a model
  inference.
* `rejected` — the claim is wrong.

Not `tool_verified` or `derived`: those mean a tool call or a transform
*reproduces* the value, which a person cannot bring about by saying so. Not
`pending_*`: that is what a claim is queued **as**, so accepting it would let an
item be cleared from the queue by re-queueing it.

Status codes are meaningful and were separated on purpose:

| | |
|---|---|
| **400** | Your fault and fixable — a missing citation, or a verdict a reviewer may not write. The body is a specific sentence, safe to show verbatim |
| **404** | No queued claim for that assertion. A verdict can only settle something that was asked |
| **500** | **Ours.** Notably a verdict the ladder declares and the database refuses, which means a platform seam has drifted. We deliberately do *not* return 400 for that — a 4xx is not paged on, and a human would be blamed once per attempt for a defect they cannot fix |

---

## What is honest to build today

| screen | data behind it | expect |
|---|---|---|
| loop overview | live, all six | 2 turning, 4 stopped with no reading |
| loop detail + stage chain | live | doors on loop2/3/4 |
| gate board | live | counters are since-boot for 3 of 5 |
| gate refusal ledger | live | **empty until the deploy lands**, then fills on first traffic. The recorder flushes every 15s |
| **gate decision review** | live, after the next deploy | **empty**, and every gate reads `unreviewed`. The write path is verified against a real Postgres — constraint names, the rationale rule, whitespace — so the emptiness is a queue nobody has worked, not a rejected write |
| **artifact trace** | live | **has content.** 75% `nothing_checked`, 21% `checked_clean`, **10 real violations** with named agent, field and claimed value. Checkpoint rungs read `decided_absent` until the deploy, then `decided` fills from the ledger |
| **verification queue + settle** | live, after the deploy | **empty.** The writer is wired on both execute paths; it fills as contracted agents run |
| **declaration census / retrofit worklist** | live | 96 real agents, 110 fixtures. Ports 93/96, field contracts 7/96 |
| **anomaly review queue** | `GET /api/observatory/hitl` — **already exists** | **empty.** No anomaly has ever been raised through the exception channel — but see the trace, which finds 10 the channel never saw |
| coordination notes per agent | live | **empty today, and the platform now delivers a floor** — see below |
| per-agent loop chain | live | 8 of 23 stages answerable per agent |
| evaluator board | live | 3 of 6 usually `inconclusive` |

**If you build one screen first, build the artifact trace.** It is the only one
with real content on arrival, it is the one whose primary object a
non-author recognises, and every other screen here is easier to explain once
somebody has seen one artifact cross one route's checkpoints.

The gate review queue needs the same treatment as the anomaly queue: build it
against an empty table, and say *why* it is empty. `review.standing` gives you the
sentence, and `unreviewed` with a decision count is a queue rather than a clean
sheet. One caution specific to this one — it is the only place on these surfaces
where **a person's action changes a reading**, so it is the one screen where a
stale cache is actively misleading. Re-read `/api/gates/:gate_id/decisions` after
a successful POST rather than patching the standing client-side; the standing is
derived from the whole append-only log and is not a counter you can increment.

The anomaly queue is worth calling out. The endpoints exist and work
(`/api/observatory/hitl`, `.../hitl/:event_id/action`,
`.../hitl/consensus/:request_id`). The queue is empty because no anomaly has ever
been raised, and that is not a bug in the pipe — 1,431 timeline entries carry 276
flags and every one is a category we deliberately treat as bookkeeping. Building
the screen against an empty queue is fine and correct; it should say *why* it is
empty, and `/api/loops/loop2` gives you the sentence.

---

## `GET /api/observatory/agents/:agent_id/loops` — one agent

**Repointed.** This path used to be served by 610 lines of separate SQL whose own
comment recorded the defect: *"two rows of which were hardcoded constants
rendered under a live status column."* It is now assembled from the same model as
`/api/loops`, and the old handler is unrouted (a test asserts it).

The important thing it adds is **which stages can be asked about an agent at
all.** Fifteen of twenty-three cannot: a forecast resolves, a workspace coheres,
a sensor reads — none of those is an agent.

```
{ "agent_id": "…",
  "coverage": { "answerable": 8, "total": 23, "note": "…" },
  "loops": [ { "id": "loop1", "answerable": 4, "total": 4,
               "stops_at": "retrieved",
               "stages": [ { "id": "retrieved", "scope": "per_agent",
                             "rows": 0, "platform_rows": 39,
                             "platform_measured": true }, … ] } ] }
```

Three rules:

1. **`rows: null` means the question does not apply** — render nothing. `rows: 0`
   means it applies and the answer is none. They are different states and this is
   the one place the distinction is easiest to lose.
2. **`scope: "platform"` carries a `because`.** Show it. That sentence is the
   difference between "this agent has done nothing here" and "this stage is not
   about agents".
3. **`platform_rows` is context and is never this agent's figure.** Rendering it
   as one is precisely the defect being replaced. It is there so a reader can
   tell "nothing here and nothing anywhere" from "nothing here and plenty
   elsewhere" — different questions about the same zero.

`stops_at` skips platform-scoped stages: a stage that is not about this agent
cannot be where this agent's chain stops.

## `GET /api/agents/:agent_id/coordination-notes`

The one you asked for: coherence notes reaching an individual agent, and whether
it has dreamt on them.

```
{ "agent_id": "…",
  "notes": [ { "episode_id": "…", "received_at": "…", "about": "…",
               "note": "…", "consolidated": false } ],
  "reading": "unknown",
  "detail": "No agent anywhere has received a coordination note. …" }
```

**`consolidated` is the field that matters.** A note sitting in an agent's memory
that consolidation has not run over has changed nothing about the agent's
behaviour — it is indistinguishable, in what the agent does, from a note nobody
sent. Dreaming reads episodes, not workspace git, which is why the note is an
episode at all.

Empty today, and the reason it was empty has changed — which matters for what you
should expect this panel to do.

`coordinator_observation` stood at **0 of 3,576 episodes** because the mechanism
asked a language model to perform a side effect: both the strategist's Stage 3
prompt and the agent card asked it to call `record_coordination_observation` for
each member, and it never did. There is no version of a prompt that makes a side
effect a guarantee.

That is now split the way it should have been. The *content* of a coordination
finding is a judgement and stays the model's; the *delivery* is bookkeeping and is
the platform's. So after any `depth=recommendations` coherence run, the platform
itself delivers the brief into every workspace member's episodic memory — skipping
the strategist, and skipping any member the strategist already wrote a targeted
note to **during that run**. The model's targeted note is better and still wins;
the platform's is the floor.

Practically, for you: **this panel fills on the next `depth=recommendations`
coherence run on a workspace the agent belongs to**, and it no longer depends on
an LLM electing to make a tool call. `detail` still distinguishes the two empties
— *nobody anywhere has one* (not this agent's problem; look at Loop 3's `brief`
stage) versus *others have and this one has not*.

Note the proxy, because we would rather you knew: `consolidation_jobs` records a
window over an agent's episodes rather than a list of ids, so `consolidated` is
answered by "did a completed job finish after this note arrived". A job that ran
afterwards and processed nothing would read as having consolidated it.

## `GET /api/declarations` — why everything else says `unknown`

**New, and it is probably the answer to the question you have been asking with the
artifact-trace request.** We measured why `unknown` dominates every surface here,
and it is not a stalled loop, a cold counter, or a check we failed to write.

Of the 206 agents that have produced an episode:

| | agents | ports | output type | checkable schema | field contract |
|---|---|---|---|---|---|
| **real** | **96** | 93 | 8 | **2** | **7** |
| **`test_agent_*`** | **110** | 0 | 0 | 0 | 0 |

**`unknown` is overwhelmingly the *agent* declaring no structure to check
against.** 3,571 of 3,576 episodes carry no grounding stamp because 89 of 96 real
agents have no field contract, and a further 110 rows are test fixtures that
declare nothing and never will.

```
{ "census": { "producing": 206, "real": 96, "cruft": 110,
              "by_rung": [["ports", 93], ["output_type", 8],
                          ["output_schema", 2], ["field_contract", 7]],
              "opaque": 0, "declared": 0 },
  "ladder":  [ { "rung": "field_contract",
                 "declares": "…", "owner": "…",
                 "unlocks": "the grounding rung on the artifact trace, …",
                 "without_it": "Nothing can say whether the agent fabricated …" } ],
  "retrofit": [ { "agent": "weather_oracle",
                  "legibility": { "legibility": "partial",
                                  "present": ["ports"],
                                  "missing": ["output_type", …] },
                  "next":  { "silence": "undeclared", "rung": "output_type" },
                  "owner": "agent_author" } ],
  "prune_count": 110 }
```

### Three things to build with, and one not to

1. **`ladder[].without_it` is the sentence to show in place of a blank panel.**
   It is written for that job — *"nothing can say whether this agent fabricated a
   value"* rather than *"no field contract"*. The first is a finding; the second is
   a fact about a Rust const.
2. **`retrofit[].next` is the cheapest missing rung, and `owner` says whose job
   it is.** The ladder is ordered by what it costs an author, so `next` is always
   the least expensive useful step. Telling someone to write a field contract for
   an agent that has not declared its ports is the most expensive step first.
3. **`owner` has three values and only one of them is ours.** `agent_author` is
   the retrofit; `platform` is our backlog; and a silence can also be nobody's —
   a cold counter resolves itself on the next request. Before this existed all
   three were the same word, which made 89 agents' missing declarations look like
   89 contracts we owed. We owe none of them.

**Do not add `retrofit.length` to `prune_count`.** They are 96 and 110, and they
are different jobs: pruning a fixture is a delete behind an existing safety gate,
retrofitting a real agent is authoring work with a domain expert. Summed, the
retrofit looks twice its real size, and its real size is what decides whether it
gets done.

### What this means for the artifact trace

Your ① will return a journey with **no checkpoints for 3,571 of 3,576 episodes**,
and that is not a bug in the trace — it is the declaration gap. The good news is
that the empty case now arrives with a cause, an owner and a named next step
instead of as a blank, so there is something real to render. The bad news is that
it is the *default* screen rather than an edge case, and we would rather agree how
it looks before we build ① than after.

And there is no percentage target here on purpose. Coverage is reported, never
compared to a figure: new agents arrive undeclared by definition, so any target or
ratchet would fire on entirely correct behaviour. Please do not render it as
progress toward 100%.

## `GET /api/evaluators`

Six checks the platform runs on its own machinery. These already carried a
`remedy` field and were reachable only inside `/api/admin/schema-health`.

```
{ "tally": { "total": 6, "healthy": …, "findings": …, "notices": …,
             "inconclusive": … },
  "evaluators": [ { "id": "loop_stalled_in_code",
                    "asks": "…",
                    "reading": "idle", "token": "healthy",
                    "detail": "2 of 6 loop(s) turning; the rest are idle …",
                    "remedy": null, "subjects": [],
                    "caveat": { "checked": "…", "does_not_show": "…" } } ] }
```

**`inconclusive` is not a pass, and three of six are usually in it.** Most of
these counters are process-local and reset on restart, so a cold snapshot
honestly concludes nothing. A surface that renders `unknown` as green reports a
healthy platform on every fresh boot — the one moment it is least entitled to.

Five tokens over three readings: `healthy`, `critical`, `warning`, `notice`,
`inconclusive`. `notice` and `inconclusive` both read `unknown` and mean
different things, so **branch on `token`, colour on `reading`**.

`doors` is empty here, and for a better reason than the gates': an evaluator is a
pure function over a snapshot. There is nothing to approve or override, and a
verdict a person could wave away would not be worth computing. Act on the
*subject* a finding names — a sink, a gate, a loop — each of which has its own
door.

### One caveat we especially want shown

`loop_stalled_in_code` returns `Healthy` with the detail *"2 of 6 loop(s)
turning; the rest are idle rather than broken."* The second half is an
over-claim: four of the six are stopped with reasons classified `unknown`
precisely because no contract can say. Its `caveat.does_not_show` says so.

We have recorded it rather than fixed it — narrowing the evaluator flips a live
verdict platform-wide and a test pins its current shape. **Please render the
caveat**, because in this instance the evaluator's own sentence is the thing that
is wrong.

## What we guarantee, and what we do not

**Guaranteed, by tests that fail the build:**

* every advertised door's path exists in the router (this caught an invented path
  on its first run);
* every declared door names a real stage or gate;
* no door sits on something the platform drives itself;
* every `why_manual` and `does_not_show` is a real sentence, not a label;
* the tally buckets partition the set;
* an unread loop reads `unknown`, never `idle`;
* **a trace with no graded fields never reads `idle`**, and every trace names an
  owner — both asserted against 256 live episodes, not just fixtures;
* **both execute paths queue their contracted claims.** This pair has silently
  diverged twice before (grounding was wired to one, claims to the other), so it
  is now a build failure rather than a comment;
* every one of these checks has been **deliberately broken and watched go red** —
  41 breaks across the seven harnesses in `scripts/break_*.py`, plus five more run
  ad hoc. Three checks in this codebase's history turned out to be incapable of
  catching the case they were written for, which is why that is a rule and not a
  habit — and two of the breaks in this batch came back green on the first attempt
  and needed the check fixed rather than the break.

**Not guaranteed:**

* that a handler behind a door does what its `does` says — the path exists and is
  routed; the semantics behind some of these endpoints still need work, and we
  know it;
* that any queue has content. Most are empty and the API tells you why;
* **that a `not_recorded` rung will ever become `graded`.** It needs one column
  on `gate_decisions`, which is a decision we have not made yet;
* that `fields[]` covers an agent's whole output. A field contract covers the
  fields somebody wrote an entry for — 98 across 10 agents — and everything else
  in the document is unexamined. The caveat on `trace.checked_clean` says this in
  the payload;
* stability of `reason` and `token` values across releases — they come from
  closed sets served in `vocabulary`, and the sets may grow. Branch on `reading`
  for anything load-bearing and treat an unrecognised token as `unknown`.

That last point is the one to design around: **an unknown token must render as
`unknown`, never as healthy.** Upstream, a new token falling through to a benign
default is exactly how "nothing has been watched" once came to display as "the
system is idle" on every panel backed by a loop.
