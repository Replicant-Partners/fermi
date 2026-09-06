# UX request — the object model, and the missing *between*

> **Vocabulary note (superseded term).** Kept as written. "Belt" is no longer the
> platform's word: the row of checkpoints an artifact crosses is `checkpoints`,
> and an edge between two agents is a **seam**.

**For:** the team that owns agents, compositions, workspaces, episodes and gates.
**From:** the trust surfaces UI, built against `UX_HANDOFF_trust_surfaces.md`.
**Status:** request. Two columns, one table, and two writers for tables that
already exist. Every schema claim below was run against the live database on
**2026-08-28** and is marked with what it returned.

**Convention**, matching the handoff:

* **[E]** — exists today, verified in this pass.
* **[W]** — the table or column exists and is **never written**.
* **[B]** — must be built.

**First, the receipts.** The last request asked for four things and three came
back: `hashes` (computed, not stored — and the `enforcement_changed_the_bytes`
caveat is exactly the kind of correction that makes a contract trustworthy),
the `parent_episode_id` correction with a coverage test behind it, and the
the `fact` assertion kind that finally lets the *Antaxius* case queue. All three
landed as described. **The fourth — `gate_decisions.episode_id` — landed while
this document was being drafted**, in migrations 220 and 221, together with the
retention promotion and review door that we were about to argue were the missing
half. See ③: the section is kept because what shipped changes the next ask
rather than closing the topic.

This document is the next layer down, and it exists because the governing
sentence turned out to have an unpaid consequence.

---

## The one idea

The sentence we agreed on solved the trace:

> **A loop is a path an artifact takes. A gate is a checkpoint on that path.**

It is right, and it is load-bearing, and it says something we have not yet
built for:

**A path is a sequence of edges. The platform has no edges.**

It has **nodes** — 761 rows in `agents`. It has **traversals** — 3,581 rows in
`episodes`. It has **checkpoints** — seven gates. What it does not have,
anywhere, in any table, is a statement that *this agent's output is supposed to
reach that agent's input*. Not one.

The consequence is precise and it is not a styling problem:

**An observed edge cannot be violated. Only a declared one can.**

A gate is a thing that can refuse. To refuse, it needs a rule; a rule needs a
declared route to be a rule *about*. Every route we can currently draw is
reconstructed after the fact from what happened, and a reconstruction is
self-justifying — whatever occurred is, by construction, what the diagram shows.
You cannot put a checkpoint on a road that is defined as *wherever the traffic
went*.

So the product spine —

> *make an agent, compose a complex agent, manage it, observe it participating
> in loops, and monitor the gates between agents*

— currently breaks at the word **between**. Not at "gates": there are seven and
they work. **There is no `between` for them to sit in.**

---

## The object model, as the database actually holds it

Each row was verified this pass. This is the table we would most like corrected
if any of it is wrong.

| the word we say | the thing in the DB | rows | state |
|---|---|---|---|
| **agent** | `agents` | 761 | **[E]** the one object that is fully real |
| **composition** | `composition_versions` | **0** | **[W]** the table exists and has never held a row |
| — *its membership* | `workspace_agents` | 2,550 | **[E]** `system` 2,513 · `owned` 26 · `hired` 11 |
| — *its edges* | **nothing** | — | **[B]** ← this document |
| **workspace** | `teams` | 263 (221 with members, 220 with >1) | **[E]** but see the naming note |
| **app** | `apps` | — | out of scope here |
| **episode** | `episodes` | 3,581 | **[E]** but see ② — it does not know its composition |
| **gate decision** | `gate_decisions` | **0** | **[W]** — but see ③: migrations 220/221 landed mid-draft and address it |

Three findings in that table are new since the last request and each one changes
an ask.

### `workspace_dependencies` is not the edge table

We found it and hoped. It has the right shape — `upstream_id`, `downstream_id`,
`dependency_type`, `key_filter` — and 48 rows.

**0 of the 48 `upstream_id` values resolve to `agents.agent_id`.** Zero also join
`workspace_agents`. `dependency_type` is `output` on all 48 and `key_filter` is
empty on all 48. Whatever those endpoints are, they are not agents, and the
table is not wired to the agent graph. We are flagging it rather than proposing
to reuse it, because a table that looks like the answer and is not is worse than
an empty schema — we nearly built against it.

### The wiring *is* drawn — as a picture, generated from the transcript

`teams.workflow_mermaid` is populated on **12 of 263** teams, alongside
`workflow_meta`. A sample:

```
sequenceDiagram
    participant Mario_Orellana
    participant efra_lens
    participant valuation_agent
    Mario_Orellana->>efra_lens:@efra_lens hi
    efra_lens-->>Mario_Orellana:Execution failed: Execution failed: Exec...
```

with `workflow_meta` = `{"generated_at": …, "participants": {…}, "message_count": 17}`.

This is the whole problem in one column. The platform **can** draw the topology,
it draws it well enough to be worth storing, and what it draws is a **rendering
of 5,559 rows of `workspace_messages`** — a transcript, generated after the
fact, in a language meant for humans to read rather than for a checker to
evaluate. It records that `efra_lens` was addressed and that it failed. It
cannot record that `efra_lens` was *supposed* to be addressed, so nothing about
it can be refused, and the diagram will look identical whether the composition
worked as designed or was never wired up at all.

**We are not asking you to delete this.** It is the right picture. We are asking
for the declaration it should have been generated *against*, so the two can
disagree — and the disagreement is the finding.

### `workspace_intentions` has zero rows

Migration 218 adds `declared_by` and `source` to it, and the reasoning in the
commit — *an agent's intention versus a belief about it, so the conflict checker
stops comparing one agent's guesses to each other* — is exactly the distinction
this document is asking for one level up. We flag only that the table it fixes
is currently empty, in case that is news; the fix is right either way, and we
have deliberately borrowed its two column names in ① below.

---

## ① The declared edge — `composition_edges` **[B]**

**The ask.** One table. An agent pair, scoped to a workspace, with who said so.

```sql
CREATE TABLE composition_edges (
  edge_id       uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  workspace_id  uuid NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
  upstream_id   uuid NOT NULL REFERENCES agents(agent_id),
  downstream_id uuid NOT NULL REFERENCES agents(agent_id),

  -- The label carried, from produces ∩ accepts. NULL is honest and expected:
  -- only 8 labels currently form a seam at all (see below).
  port          text,

  -- Borrowed verbatim from migration 218, for the same reason it was added
  -- there. An edge the strategist inferred from a transcript is a *belief
  -- about* the composition; an edge a person drew is the composition. A
  -- checker that cannot tell them apart will compare inferences to each
  -- other and report the result as the design.
  declared_by   text NOT NULL,
  source        text NOT NULL CHECK (source IN ('human','strategist_inferred','seeded')),

  created_at    timestamptz NOT NULL DEFAULT now(),
  UNIQUE (workspace_id, upstream_id, downstream_id, port)
);
```

`workspace_id` is not optional. Membership is already workspace-scoped and an
agent averages 3.3 memberships (2,550 over 761), so a global edge would assert
the same wiring in every composition an agent appears in, which is false by
construction.

**What we would render from it, and cannot render without it:**

* The **composition as a drawing you can point at** — `compositions_v16` stops
  being a simulation. This is the single screen the user has named as the
  conceptual target, and it is blocked on this table and nothing else.
* **The gate *between* two agents.** With an edge, a seam has an identity, so a
  refusal has a location: *`forage_scout → harvest_advisor` refused, on port
  `species_id`.* Today a refusal can only be attributed to an agent, which is
  why every gate surface we have built reads as a property of a node.
* **`declared but never travelled`** — an edge with no episodes. This is a
  `fault` in the exact sense the handoff defines: something that should have
  happened and did not. **We currently cannot produce a single true `fault`
  anywhere in the loop surface**, and that is not because the platform is
  healthy. It is because a fault requires an expectation, and expectations are
  the thing that is missing. Four of six loops read `no_reading` for this
  reason.
* **`travelled but never declared`** — traffic on an edge nobody drew. The
  inverse finding, and the one `workflow_mermaid` is accidentally already
  collecting evidence for.

**On the port type, and why we are not asking you to fix it yet.** Of 761
agents, **101 declare `accepts` and 101 declare `produces`**, across 274 distinct
`produces` labels and 237 distinct `accepts` labels — of which **8 appear on both
sides** and can therefore form a seam. (This is down from the 13 we recorded in
the last pass; the labels move.) That ratio is the real state of the type system
and we would rather show it honestly than have it improved before it is
measured. `port text NULL` is correct for now. Please do **not** constrain it to
the intersection — an edge someone drew across a type mismatch is a finding we
want to render, not an insert we want rejected.

---

## ② `episodes` does not know which composition it ran in **[B]**

**Verified:** `episodes` has `agent_id` and `parent_episode_id` and **no
workspace, team or composition column of any kind.** 2,321 of 3,581 episodes
were produced by an agent that belongs to at least one workspace — but *which*
workspace the run was for is not recorded anywhere, and since agents average 3.3
memberships, it is not recoverable by inference either.

**The ask.** One column, and a second if ① lands:

```sql
ALTER TABLE episodes ADD COLUMN workspace_id uuid REFERENCES teams(id);
-- and, once composition_edges exists, the strong form:
ALTER TABLE episodes ADD COLUMN edge_id uuid REFERENCES composition_edges(edge_id);
```

**Why this is second and not fifth.** The product spine says *observe it
participating in loops*. Participation is a relation between a run and a
composition, and that relation is the one fact not stored. Right now the honest
sentence under an agent on the observatory is *"this agent ran 3,576 times"* —
we cannot say *"it ran 40 times as part of this team pursuing this mission"*,
which is the only version anyone actually wants. Every per-composition view —
cost, refusal rate, drift, whether a team is doing anything at all — is blocked
on this single column, and all of them are cheap once it exists.

`edge_id` is the strong form because it makes the belt real: an episode that
names its edge can be checked against the port that edge declares, which is the
seam check the last request asked for and the hash digests correctly declined to
provide.

---

## ③ `gate_decisions.episode_id` — **delivered while this was being written**

Migrations **220** and **221** landed mid-draft. This section is kept rather than
deleted, because what shipped is more than was asked for and it changes the next
ask.

We were going to send this as *"the column, **and** a writer that reaches it"* —
having discovered that `gate_decisions` has **0 rows** and that the review surface
we shipped at `/gate/:gate_id` therefore renders an empty ledger on all seven
gates. 220 makes that argument itself, from measurement, before writing the
column: *every per-episode gate is `Retention::Counted`, and both `Recorded`
gates are not per-episode*, so the column would have been NULL on every row that
would ever exist **while making the trace's `not_recorded` look solved.** 221 then
promotes `grounding` and ships the review door in the same change.

That is the ratchet working, and from the correct end. Two notes for the record,
then the one gap and the two things it creates for us.

### ③a The write side landed; the read side does not join yet **[B]**

Flagging this because it is the difference between *stored* and *visible*, and
from the outside they look identical. Verified in the tree as of this writing:

* `artifact_trace::belt()` (`src/artifact_trace.rs:150`) still hardcodes
  `Outcome::NotRecorded` for **every** rung, with the reason string
  *“`gate_decisions` carries no `episode_id`”* — which is no longer true.
* `episode_trace_handler` (`src/handlers/loops.rs:730`) selects from `episodes`
  and `agents` only. **No query anywhere selects `gate_decisions` by
  `episode_id`** — `grep -rn "FROM gate_decisions"` returns `gate_review.rs`,
  `handlers/gates.rs` and `gate_api.rs`, all gate-scoped, none episode-scoped.
* The handler's own `contract` string still tells the client
  *“`gate_decisions` has no `episode_id` and nothing else can be joined.”*

So the ledger will now accumulate grounding decisions, and the trace will keep
reporting `not_recorded` for all of them. **This is the smallest possible piece
of work and it is the one that makes 220 and 221 visible** — one `SELECT gate,
decision, reason, decided_at FROM gate_decisions WHERE episode_id = $1`, folded
over the belt.

We have not written it ourselves because the outcome shape is your contract, not
ours, and inventing a definition on the read side is the failure this whole line
of work exists to prevent. But it is the last inch, and until it is walked the
two migrations have no observable effect.

* Recording the argument for a retention change in a `SELECT 1;` migration —
  because *"a decision made in a constant is a decision nobody can find"* — is a
  good pattern and we would like it to continue.
* The volume measurement (1–76 episodes/day, median ~20; 516 of 3,581 carrying an
  actual opinion, 3,065 recording `undetermined`) is the kind of number that
  should reach the UI. We would render `undetermined` as its own third thing, as
  we do everywhere else.

### ③b The belt now has three kinds of `not_recorded`, and they render identically **[B]**

This is the new ask, and it is small. 220 establishes that a missing ledger row
now means **three different things** depending on the gate:

| kind | which gates | what it means | how it should read |
|---|---|---|---|
| **fires before the artifact** | `credit`, `rate_limit` | NULL is *correct and final* — they decide whether to run at all, and there may never be an artifact | **not a gap.** Rendering it as one is a false finding |
| **could record, is not retained** | `input_binding`, `attachment`, `output_schema` — per-episode, still `Counted` | a gap a decision could close, exactly as `grounding` just was | a gap, with the decision named |
| **retained and absent** | `grounding`, from now on | either predates the promotion, or the recorder dropped it | genuinely `unknown` |

Today the trace returns `outcome: not_recorded` with a prose reason for all
three. Prose cannot be branched on, so we render one grey rung for three
unrelated states — which is the same collapse the handoff spends its first page
forbidding, one level further down. **`credit` reading as a gap is the harmful
one:** it is a permanent, correct NULL that will look like an unpaid debt on
every belt we ever draw.

**The ask:** a token beside the prose, from a closed set —
`fires_before_artifact` · `retention_counted` · `predates_retention` ·
`retained_but_absent`. The sentence you already send is the right sentence; we
just cannot colour by it.

### ③c `decided` and `recomputed` must not be merged **[B]**

221 contains the most useful sentence in either document:

> re-running the contract over retained responses finds **10 violations that
> `episodes.tags` never recorded**, because the contract was not wired to those
> paths when the episodes ran. A recorded decision that says `approved` beside a
> recomputation that says `2 violations` is not a contradiction — it is the
> contract having been tightened afterwards.

That disagreement is a *feature* and it is one of the few things this platform
can say that nothing else can: **the standard moved, and here is an artifact that
would not pass today.** It is also invisible unless both numbers survive to the
client as separate fields.

**The ask:** on the grounding rung, `decided` (what the ledger says the gate
concluded at the time) and `recomputed` (what the contract says now), always
both, never reconciled server-side. If they agree, we render one thing. If they
disagree, we render *"approved when it ran; 2 violations under today's
contract"* — which is a drift finding about the platform rather than about the
agent, and we have no other way to produce one.

We would rather have this than any other item in this document except ①.

---

## ④ `assertion_verifications` has the right shape and no writer **[W]**

**Verified: 0 rows.** Columns are `verification_id`, `assertion_id`,
`episode_id`, `verdict`, `source_citation`, `actor`, `actor_kind`, `evidence`,
`created_at` — and `actor_kind ∈ {tool, human, platform}` maps one-to-one onto
the `pending_tool` / `pending_human` routing the trace endpoint already emits.

**We are explicitly not asking for a second queue.** This one is correct. We are
asking for the writer that moves a `routed[]` entry into it when a verdict is
reached, so that a rejection *rate* exists. Until then the trace can show what
was **routed** but never what was **settled**, and "nobody has checked this yet"
and "somebody checked this and it was fine" are the same rendering — which is
the one collapse the handoff asks us hardest never to make.

This is the last dependency of the rejection-rate surface, which is otherwise
built.

---

## ⑤ `assertions[].basis` is present and always empty **[W]**

**Verified:** 42 episodes carry assertions; **42 contain the string `basis`; 0
have a `basis` array of non-zero length.** The field is emitted as `[]` on every
assertion, every time:

```json
{"raw": "Suggested p50: 0.80 (p5: 0.60, p95: 1.05)", "kind": "multiplier",
 "basis": [], "value": {"p5": 0.6, "p50": 0.8, "p95": 1.05},
 "extraction": {"path": "prose", "pattern": "multiplier_v2"},
 "target_hint": null, "assertion_id": "107f296c-…"}
```

An always-`[]` field is worse than an absent one: it type-checks, so a client
builds against it and renders "no basis" as a fact about the claim rather than
as a fact about the extractor. The per-claim provenance floor is uncomputable
while this holds, because the floor is a function over the basis.

**Either is a good answer:** populate it from the extraction site, or declare it
absent the way `hashes: null` was declared absent before it was built — which is
a pattern from your own handoff that worked, and it worked precisely because
nobody built against it.

---

## ⑥ `episodes.provenance` is single-valued **[E, and carries no information]**

**Verified: `auto_pass` on all 3,581 rows.** One distinct value, no nulls.

A column with one value is a column that answers no question, and it currently
sits in the episode payload looking like a verdict. Either something should
write the other values, or it should be retired from the API surface so it stops
implying a check was made. We have no preference; we would just like it to stop
being ambiguous. Note this is a different column from the `provenance` ladder in
`grounding_trust::PROVENANCE_VALUES`, which is real, live and load-bearing — the
name collision is itself a small hazard.

---

## ⑦ The delegation envelope hash **[B]** — your item, restated so it does not get lost

You named this one yourselves and the reasoning was right: the hashes cannot
verify a seam by equality with a parent's output, because a child receives a
prompt built around the task rather than the output verbatim. The place equality
*would* hold is **the envelope payload in the delegation hop, which is passed
through unchanged and which nothing hashes.**

We are restating it here only so it survives into the next pass, and to add one
thing: with ① and ② in place, the envelope hash becomes the check that an
artifact crossing a declared edge is *the same artifact* — which is the seam
check in its final form. It is last in priority and it is the one that closes
the loop.

---

## A small naming ask

The UI says **workspace**, the database says `teams`, and the columns say both
(`workspace_agents.workspace_id → teams.id`, verified 2,550 of 2,550). The
product spine has four nouns in it and one of them currently has two names
depending on which layer you are reading.

We are **not** asking for a rename — that is expensive and risky and buys
nothing at runtime. We are asking that **the API speak one word**, whichever one
you pick, so that the object model is legible from the outside. A view or an
alias in the serialiser is fine. The requirement being served is the first one
on the user's list: *the object model must be legible* — and a noun with two
names is the cheapest possible way to fail it.

---

## What we are not asking for

Stated explicitly, because the last pass showed how much already exists and how
easy it is to ask for it twice.

* **Not a second review queue.** `assertion_verifications` is the right table.
* **Not `prior_episode_id`.** `episodes.parent_episode_id` exists and is written;
  it is thin because delegation is rare, which you established and we accept.
* **Not a rebuild of `workflow_mermaid`.** It is the right picture. It needs
  something to be a picture *of*.
* **Not the port type system.** 8 seam-forming labels of 274 is the honest
  current state and we would rather render it than have it tidied first.
* **Not stored hashes.** Computed-on-read was the better call and the reasoning
  in the handoff convinced us.
* **Not a fix to `workspace_dependencies`.** We do not know what it is for. We
  only need to know it is not this.

---

## Priority

Leverage and cost point in different directions, so both are given.

| | ask | cost | unlocks |
|---|---|---|---|
| — | ~~**③** `gate_decisions.episode_id` + a writer~~ | — | **done** — migrations 220 + 221, with the door |
| 0 | **③a** join `gate_decisions` by `episode_id` in the trace handler | one query | **makes 220 and 221 observable.** Until this, they have no visible effect |
| 1 | **③b** a token for the three kinds of `not_recorded` | one enum | stops `credit` reading as a permanent unpaid debt on every belt |
| 2 | **③c** `decided` and `recomputed` as separate fields | two fields | *"the standard moved"* — a drift finding about the platform, not the agent |
| 3 | **②** `episodes.workspace_id` | one column | every per-composition view; *participation* becomes a fact |
| 4 | **①** `composition_edges` | one table | the composition as an object; the gate *between*; the first true `fault` |
| 5 | **④** a writer for `assertion_verifications` | one writer | rejection rate; settled vs unchecked stop colliding |
| 6 | **⑤** `basis`, populated or declared absent | either | the per-claim floor |
| 7 | **⑥** `provenance`, written or retired | trivial | removes a false verdict from the payload |
| 8 | **⑦** the envelope hash | real work | the seam check, in its final form |

Items 0–2 are all small and all fall out of what you just shipped; they are first
because 220/221 made them possible rather than because they outrank ①.

**What we shipped against this in the meantime.** The belt now distinguishes
*“keeps a ledger, no row for this artifact”* from *“counts in memory, no row
could exist”*, by reading each gate's `since` from `/api/gates` — **derived from
served data rather than a list held in the client**, precisely so that promoting
the next gate updates the UI without a front-end change. It is a stopgap for
③b's first two cases and it cannot reach the third: nothing served distinguishes
*fires before the artifact exists* (`credit`, `rate_limit`) from an ordinary
unretained gate, so those two still render as a gap they will never fill. That
remains the ask.

**If only one thing is done, we would take ①** — it is third by cost and first by
consequence. Items ②, ④, ⑤ and ⑥ are each a day's work that unblocks a screen.
Item ① is the one that changes what the platform can say about itself, because it
is the difference between *observing what happened* and *checking it against what
was supposed to*.

That difference is the whole product. Everything the trust surfaces do — every
reading, every caveat, every `does_not_show` — is an attempt to be honest about
the fact that we are currently only doing the first one.
