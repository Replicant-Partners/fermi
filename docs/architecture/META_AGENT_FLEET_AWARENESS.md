# Meta-agent fleet awareness

**Status:** Pattern documented; digest generator in `src/fleet_digest.rs`
**Date:** 2026-09-07
**Related:** `docs/architecture/AKP-ecology-design-doc - roadmap.md` §7,
             `docs/AGENT_MODEL.md` §3.3,
             `docs/plans/WHAT_THE_PLATFORM_CAN_REFUSE.md` §4.5

---

## The problem, stated generally

A **meta agent** is one whose subject matter is its own fleet: a navigator, an
orchestrator, a composition planner, a fleet observer. Every platform that grows
past a dozen agents produces one, and every one of them meets the same wall.

The naive implementation puts the fleet in the meta agent's system prompt. It
works at ten agents, strains at fifty, and at a hundred it has three defects at
once:

1. **The prompt grows linearly with the fleet.** O(n) tokens on every
   invocation, whether or not the question touches any of them.
2. **The digest is lossy in a way nobody chose.** One line per agent is what
   fits, so the line carries a description and drops the facts — model, tier,
   ports, tools. The omission is invisible to the reader of the prompt.
3. **It is stale by construction.** The prompt is authored; the fleet is live.
   Any sync mechanism is a second source of truth.

ABW hit all three. 102 agents were pasted into `xaman_ek`'s system prompt, kept
in sync by a test asserting each `agent_id` appears as `**agent_id**`. Every
agent added made the prompt longer, the digest lossier, and the test redder.

## The incident this pattern exists to prevent

Asked which model a free-tier creature would use for `biotech_analyst`,
`xaman_ek` answered:

> *"**biotech_analyst does not have a declared `model_ladder`** in its agent
> card. This means it is **tier-agnostic** — your free-tier creature will run it
> at the **card's default model**. The default model for biotech_analyst is
> **Claude Haiku**."*

| claimed | actual |
|---|---|
| no `model_ladder` declared | three rungs: premium, standard, free |
| tier-agnostic, runs the card default | free resolves to `openrouter/free` |
| default is Claude Haiku | default is `claude-sonnet-4-5` |

Three claims, all false, all one local `SELECT` from being checked.

**It could not have answered correctly.** Its prompt entry for that agent was a
one-line description with no model information, and `list_agents` returned
`{id, type, description, skills}` — no model information either. The question
was unanswerable from every source it could reach, so confabulating something
specific and plausible was the only way to answer at all.

> A meta agent's grounding problem is not that it fabricates. It is that the
> platform hands it **prose about its fleet instead of structured access to
> it** — and then fabrication is the only way to answer.

This is the load-bearing observation. The failure looks like a model problem and
is an architecture problem.

## The trap in "just use tools"

The obvious fix is to move the fleet out of the prompt and behind a tool. Done
naively it **relocates the cost and multiplies it**: a `list_agents` that returns
102 agents × 8 fields lands in context *per invocation*. O(n) once in a prompt
becomes O(n) on every call.

ABW walked into this. Widening `list_agents` to carry model, ladder, ports and
tools fixed the fabrication and enlarged the dump. The widened payload is the
right response for `describe_agent(id)` and the wrong one for `list_agents()`.

**A tool tier has to be queryable, not enumerable.**

## The pattern: three tiers

| tier | holds | cost |
|---|---|---|
| **prompt** | the fleet's *shape* — a derived, fixed-size digest | O(structure) |
| **tools** | per-question lookup: `describe_agent`, `who_answers`, `agents_of_type` | O(1) per question |
| **delegation** | cluster-resident experts the meta agent routes to | O(1) for the meta agent |

The design principle is already written down in `AKP-ecology-design-doc` §7:

> *"The meta-agent does not attempt to know everything every agent knows. Its
> awareness is ecological — it understands the shape, dynamics, and health of
> the agent ecosystem."*

The prompt carries the **map**. The tools serve the **territory**. Delegation is
what makes it scale past compression.

### Why the digest is O(structure), not O(agents)

The digest names *categories and counts*, never individual agents. Adding a
hundred agents moves the counts and leaves the row count almost unchanged.

This is the invariant that has to be defended by a test, because it is the one
that erodes: the natural edit is *"and here are the members"*, which silently
restores O(n).

## Choosing the digest axes

Not every attribute clusters. Measured over ABW's 102 cards:

| axis | cardinality | covers | grows with | verdict |
|---|---|---|---|---|
| `agent_type` | **13** | 102/102 | curated vocabulary | ✅ the reliable spine |
| type namespace (`abw/`, `fermi/`, `scro/`) | **6** | 21/102 | products/domains | ✅ thin but slow-growing |
| shared `accepts` cohorts | ~47 | most | converging questions | ✅ the "who answers what" map |
| `skills` | **366** | all | **agents** | ❌ useless — max 4 share one |

The test for a digest axis is: **does its cardinality grow with the fleet, or
with the fleet's structure?** `skills` fails it — 366 distinct skills across 102
agents, the modal skill shared by one. Anything with that shape belongs behind a
tool, never in a digest.

### The cohort axis needs a third reading

A shared label is only useful if it *narrows* the fleet. `query` is accepted by
24 of 102 agents; knowing an ask is a `query` excludes nothing, and reporting
those as interchangeable is true and useless. So cohorts classify in three
states rather than reporting a count — see `port_trust::Substitutes`:

```
query              24 of 102   Universal   the calling convention, not a seam
workspace-state     8 of 102   Cohort      all coordination/coherence agents
abw/genome-query/1  1 of 102   Bespoke     genome_profiler's alone
```

`Bespoke` is a real answer, not an absence: *"only this one"* is more useful
than *"I don't know"*.

## The two things that must ship with a thinner prompt

Compressing the fleet out of the prompt **increases** confabulation risk. The
incident above happened because the source was absent and answering was still
expected; a thinner prompt is a larger version of that gap. Two counterweights
are not optional.

### 1. Name what the agent does not know

> *"You know the fleet's shape, not per-agent facts. Model, ladder, tier and
> ports come from `describe_agent`. Do not state them from memory."*

If the platform can name what would close a gap, the name is the control. A
digest that only says what *is* known invites the model to fill the rest.

### 2. Make staleness self-detectable

The digest carries the fleet size it was built from. If a tool later reports a
different count, the meta agent knows its map is stale and can say so instead of
answering from it.

This is the cheapest honesty mechanism available and it is what keeps a
compressed map safe as the fleet moves. A digest without it is the authored
prompt again, with fewer lines.

## What this replaces

An O(n) prompt kept in sync by an O(n) test becomes a derived digest checked by
an O(1) assertion: *does the digest match the corpus it claims to describe?*

The prompt-sync test is not merely redundant afterwards — it **institutionalises
the encyclopedic model**, and it is the reason nobody noticed the design had
moved past it. A test is a claim, and this one claims the meta agent should know
every agent individually.

## Grounding the claims a meta agent makes

Once the fleet is a structured source rather than prose, the meta agent's claims
become typeable. The kind is **`Derived`** — computed by us or checked by us,
never merely asserted — and not `Sourced`:

`Sourced` asserts only that *a tool could answer*. ABW's `Antaxius beieri` case
is what that is worth: a bush-cricket reported as a longhorn beetle, present,
non-null, correctly typed, declared sourced, with the verified answer one table
over. Every automated check passed. A meta agent can call `list_agents` and then
answer from its prompt anyway, and nothing would notice.

The claim classes, and where each one's truth lives:

| claim | source of truth | kind |
|---|---|---|
| demographics — model, ladder, tier, tools | the registry | `Derived` |
| artifacts — `produces`, `produces_schema` | the card | `Derived` |
| substitutability — who else answers this ask | cards, grouped by `accepts` | `Derived` |
| chainability — what has actually fed what | the **run record**, not the card | `Derived` |
| recommendations — use X then Y for your problem | nothing | `Inferred` |
| explanation, framing | nothing | `Narrative` |

Two of those are easy to get wrong.

**Chainability is not in the cards.** Declared ports converge on *the artifact I
make* (`produces`) and *the question I answer* (`accepts`) — request/response,
not a pipe — so they never string-match. Measured over every hand-off in ABW's
production history, declared ports predict **none** of them. Computing
compatibility as an authority would refuse the entire real topology, and would
also freeze a fleet whose purpose is to reconfigure. It belongs in the trace,
observed, and reported rather than enforced.

**Recommendations are `Inferred`, not prose.** The distinction is operational: a
completeness gate treats an empty `Inferred` field as **owed** — the agent was
commissioned to produce a judgement and did not. Filing a recommendation as
`Narrative` silently excuses the meta agent from the thing it exists to do.

## The prerequisite that is easy to skip

A contract cannot name a source that does not exist. Declaring a claim `Sourced`
against a tool that cannot supply it, or `Derived` with nothing computing it, is
a check the platform cannot run — which is worse than no check, because the
declaration reads as coverage.

So the order is fixed:

1. **Sources first** — widen the listing, build the cohort grouping, expose the
   observed topology.
2. **Digest second** — derived from those sources, so it cannot drift.
3. **Contract last** — and only for meta agents that emit a *document*.

Step 3 has a hard precondition worth stating: a field contract over a prose
response enforces nothing. `xaman_ek` emits prose (4 of 18 recorded responses
contain even a brace), so its contract waits on an agent change, not a platform
one. `fermi` emits documents in two thirds of its responses and is the
contractable meta agent today.

## Generalising

For any fleet meta agent, on any platform:

1. **Measure your candidate digest axes.** Keep the ones whose cardinality
   tracks structure. Reject any whose cardinality tracks membership.
2. **Put the map in the prompt and the territory behind queryable tools.** Never
   an enumerating tool.
3. **State what the agent does not know**, in the prompt, next to the map.
4. **Anchor the digest to a fleet size** so staleness is self-detectable.
5. **Type the claims as `Derived`**, with the source built before the contract
   names it.
6. **Prefer delegation to compression** once the clusters have names — a digest
   is what makes routing possible, not the destination.
