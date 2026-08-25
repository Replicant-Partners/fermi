# UX handoff — the trust surfaces

**For:** the team building the tools that make loops, gates and evaluators
visible.
**Status:** all three domains live — loops, gates, evaluators — plus two
per-agent endpoints.

```
GET /api/loops                                  six loops, doors, caveats
GET /api/loops/actions                          doors alone, no DB walk
GET /api/loops/:loop_id                         one loop
GET /api/gates                                  five gates
GET /api/evaluators                             six self-checks, with remedies
GET /api/observatory/agents/:id/loops            one agent's chain
GET /api/agents/:id/coordination-notes           Loop 3 → Loop 1, per agent
```

Every one of these carries the same `reading` vocabulary and the same two extra
parts — `doors` (what a person can do) and `caveats` (what a tick does not mean).
Branch once, reuse everywhere.

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

### `doors: []`

Empty, and we would like you to render the emptiness rather than hide the
section. There is currently **no endpoint anywhere that lets a person act on a
gate** — no review of what it refused, no override, no way to record that a
refusal was wrong. That may be correct (a gate a person can wave through is not
much of a gate) but nobody has decided it explicitly. A visible "no actions
available" is how that decision gets made.

---

## What is honest to build today

| screen | data behind it | expect |
|---|---|---|
| loop overview | live, all six | 2 turning, 4 stopped with no reading |
| loop detail + stage chain | live | doors on loop2/3/4 |
| gate board | live | counters are since-boot for 3 of 5 |
| **anomaly review queue** | `GET /api/observatory/hitl` — **already exists** | **empty.** No anomaly has ever been raised |
| coordination notes per agent | live | **empty** — no note has ever been sent |
| per-agent loop chain | live | 8 of 23 stages answerable per agent |
| evaluator board | live | 3 of 6 usually `inconclusive` |

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

Honestly empty today: `coordinator_observation` stands at **0 of 3,576 episodes**,
so `record_coordination_observation` has never been called. `detail` distinguishes
the two empties for you — *nobody anywhere has one* (not this agent's problem,
look at Loop 3's `brief` stage) versus *others have and this one has not*.

Note the proxy, because we would rather you knew: `consolidation_jobs` records a
window over an agent's episodes rather than a list of ids, so `consolidated` is
answered by "did a completed job finish after this note arrived". A job that ran
afterwards and processed nothing would read as having consolidated it.

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
* an unread loop reads `unknown`, never `idle`.

**Not guaranteed:**

* that a handler behind a door does what its `does` says — the path exists and is
  routed; the semantics behind some of these endpoints still need work, and we
  know it;
* that any queue has content. Most are empty and the API tells you why;
* stability of `reason` and `token` values across releases — they come from
  closed sets served in `vocabulary`, and the sets may grow. Branch on `reading`
  for anything load-bearing and treat an unrecognised token as `unknown`.

That last point is the one to design around: **an unknown token must render as
`unknown`, never as healthy.** Upstream, a new token falling through to a benign
default is exactly how "nothing has been watched" once came to display as "the
system is idle" on every panel backed by a loop.
