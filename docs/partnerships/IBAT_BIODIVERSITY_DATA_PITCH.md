# Paying for biodiversity data by proving we used it

**A proposal to the IBAT Alliance**
**From:** Agent Bestiary World / Rabble · **Date:** 2026-08-16 · **Status:** draft for outreach

---

## The short version

We run a game in which the creatures are **real species**. 130 creatures across 97
distinct species are live in our database today. Players unlock modules that tell them
where a species sits in the tree of life, what threatens it, and how it is doing.

Two months ago that last part was a lie. Our agent confidently reported IUCN Red List
statuses for species that have never been assessed — manufacturing the *appearance* of
having consulted the Red List. We caught it, we can show you exactly what it said, and we
have rebuilt the platform so that it cannot happen again.

That rebuild produced something we think is unusual and worth your attention: **every
field our agents emit now carries machine-readable provenance naming the tool that
supplied it, and every call is individually metered and auditable.**

Which means we can offer a data partnership on terms that are normally impossible: not a
flat licence fee negotiated against guessed volume, but **a share of revenue computed
from a verifiable record of what we actually consumed, field by field.**

---

## Part 1 — The problem we found, because it is your problem too

Our `genome_profiler` agent was asked to report conservation status. It had two tools,
both returning taxonomy only. No Red List access whatsoever.

It answered anyway. Verbatim, from our production database:

| species | reported `iucn_status` |
|---|---|
| *Apatura iris* | "Not Evaluated (presumed Least Concern)" |
| *Anatis mali* | "Not Evaluated (common, widespread North American species)" |
| *Reclavaspis evexa* | "Not formally assessed (minor agricultural pest species)" |
| *Sphingonotus personatus* | "Not Evaluated (NE)" |

Fifty-six runs produced output like this. Thirteen documents were cached and shown to
users.

Look closely at what makes these dangerous. They are not obviously wrong. "Not Evaluated"
is a **real IUCN category**, and for most insects it is probably even the correct answer.
The fabrication is not in the value — it is in the **implied act of assessment**. A reader
cannot distinguish a fabricated "Not Evaluated" from a genuine Red List lookup. Neither
can a downstream system. Neither could we, until we built something that could.

In the same output, our agent reported genome sizes derived from a family-level average
that had been written into its own instructions. For the monarch butterfly it would claim
400–500 Mb. The actual assembled genome is **245 Mb**. Wrong by roughly a factor of two,
stated with complete confidence.

**This is the argument we want to make to you, and we think it is the important one:**

> The alternative to licensing real biodiversity data is not "an application with no
> biodiversity data". It is an application full of *plausible fabrications that are
> indistinguishable from your data* — and every one of them quietly spends down the
> authority that makes your data worth having.

Language models are extremely good at producing text shaped like an IUCN assessment. As
that capability spreads into consumer products, the scarce and valuable thing stops being
*a conservation status* and becomes *a conservation status you can prove came from the
people who assessed it*. That provenance is an asset you own and currently cannot monetise,
because no consumer of your data can demonstrate they honoured it.

We can.

---

## Part 2 — What we built, and why it matters commercially

Over the last weeks we implemented a four-level verification system. Stated plainly, it
answers four escalating questions about every value an agent produces:

| | Question |
|---|---|
| 1 | Does the field exist? |
| 2 | Does it hold the value it claims? |
| 3 | **Could this value have come from anywhere at all?** |
| 4 | Is the caller sending what the interface declared? |

Level 3 is the one relevant to you. Every output field of every agent is now mapped to the
specific tool that could legitimately supply it. A field with no such tool is **forced to
null** before anything is cached, rendered, or passed to another agent, and the attempt is
recorded as an anomaly. The prose summary is scanned too, because clearing a fabricated
number from a data field while leaving it in a sentence merely moves it to where humans
actually read.

The vocabulary distinguishes four states, and the distinctions are the point:

```
tool_verified                a named tool returned this
tool_no_match                we asked, and there is genuinely nothing
platform_derived             computed deterministically from a verified value
unavailable_no_tool_source   nothing could supply it — so it is null
```

`tool_no_match` versus `unavailable_no_tool_source` is the distinction your data makes
possible. "We queried the Red List and this species has not been assessed" is a *fact about
the world*. "Nobody asked" is a *gap in our platform*. Today, without you, every
conservation field in our system reads as the second. With you, they read as the first —
and that is worth paying for.

Alongside this, we already operate:

- **Per-call metering.** 3,260 agent executions recorded to date, each with model,
  provider, token split, and cost. Not estimated — measured, with a per-row field recording
  how much to trust the figure.
- **An append-only audit log** for verification events, including every blocked attempt to
  populate an unsourced field.
- **A per-agent credit ledger.** Players spend credits per action; our accounting is already
  transaction-level because our own economics require it.

None of this was built for a data partnership. It was built because we did not trust our own
agents. But it happens to be exactly the infrastructure a data licensor would want and
almost never gets.

---

## Part 3 — The proposal

### A win-win modality: pay per verified use

Conventional data licensing is a flat fee against estimated volume. The licensor cannot see
actual consumption, cannot verify attribution was honoured, and captures no upside if the
licensee succeeds.

We propose the inverse:

1. **Every field sourced from IBAT is tagged `tool_verified` with the endpoint that
   supplied it**, at the record level, in an append-only log.
2. **We publish a consumption report** — per period, per dataset, per endpoint, per field —
   derived from that log rather than from our own assertions. You can audit it against your
   own API logs; the two should reconcile exactly.
3. **A defined percentage of relevant revenue flows to the Alliance**, computed from that
   report. Our credit system already prices individual actions (a species profile costs a
   player 2 credits, currently $0.02), so the share is a computation, not a negotiation.
4. **Attribution is rendered, not buried.** Where a player sees a conservation status, they
   see the assessment date, the assessor, and the Alliance's attribution — because our
   provenance system carries those fields anyway, and a status without its date is a weaker
   claim we no longer want to make.

### What this gives the Alliance that a licence fee does not

- **Revenue that scales with our success** rather than being fixed at the moment of least
  information.
- **Verified attribution.** You can demonstrate to your own constituents that a commercial
  consumer honoured citation terms, with an audit trail rather than a promise.
- **Demand signal.** Which species do hundreds of thousands of players actually ask about?
  That is a dataset you do not currently have and cannot easily buy: public attention,
  species by species, measured rather than surveyed.
- **A defence against fabrication.** We become a demonstration that a consumer AI product
  can be built which *refuses* to invent biodiversity claims. Right now the industry is
  producing the opposite example at scale.

### Where the awareness argument actually lands

We are not going to claim a game saves species. What we will claim is narrower and true:

97 real species are already in front of players, each one a creature they have chosen to
care about. When a player unlocks a conservation module, the honest answer for most insects
is "Not Evaluated" — **and that is the single most useful thing a member of the public could
learn about invertebrate conservation.** The gap in the Red List for invertebrates is
invisible to almost everyone outside the field. Our system is now architecturally incapable
of papering over it, because an unassessed species must render as unassessed.

An MMOG that teaches players that most of the small animals around them have never been
assessed, and shows them the real assessment where one exists, is biodiversity literacy
reaching an audience that will never open a Red List species page.

---

## Part 4 — Honest constraints

We would rather raise these than have you find them.

**We are pre-revenue.** Beta players are on free credits, so current actual revenue is
**$0**. Our internal modelling puts credit revenue at roughly $158/month at beta scale,
~$3,700/month at growth, ~$57,000/month at scale. A revenue share today is a small number
multiplied by a percentage. We are proposing the *modality* now precisely so it is in place
before it matters, and so the architecture is built around attribution rather than retrofitted.

**Your API is the wrong shape for our primary need, and we know it.** IBAT v2 is
spatial-first: `/redlist/intersect/species` returns species within a geometry. We mostly
hold a species and want its status. We can work around it via our creatures' coordinates,
but we would rather be honest that the free IUCN Red List API is the better fit for
species-keyed lookups, and that IBAT's distinctive value to us is **spatial** context —
protected areas, Key Biodiversity Areas, and what threatened species share a location with
a player's creature. That is a capability no agent in our system can currently claim at all.

**Location sensitivity is real and we will respect it.** Precise coordinates of threatened
species carry poaching risk; we note IBAT applies a 50 km buffer for exactly this reason. We
will not surface finer resolution than you provide, will not let players query arbitrary
coordinates to enumerate threatened species, and are happy to have that constraint written
into the agreement rather than left to our discretion.

**Rate limits.** A game can generate load an assessment API was never built for. We already
cache species-level results per creature, and our provenance model treats a cached value as
a first-class citizen with its retrieval date attached. We would agree call ceilings and
cache lifetimes up front.

**Enterprise Plus is a real cost.** At our current stage the subscription likely exceeds the
revenue share it would generate. If that makes a commercial agreement premature, we would
still value a conversation about the *model*, and a limited pilot.

---

## Part 5 — A concrete pilot

Small, bounded, and designed to prove the mechanism rather than the market.

| | |
|---|---|
| **Scope** | One spatial endpoint, our ~97 species, capped call volume |
| **Duration** | 90 days |
| **We deliver** | A per-field consumption report reconcilable against your API logs; rendered attribution with assessment date and assessor; a public write-up of the verification architecture crediting the Alliance |
| **You deliver** | Time-limited API credentials at pilot volume |
| **Success test** | Your logs and our provenance log reconcile exactly; zero fabricated conservation claims reach a player; the revenue-share calculation runs end to end even if the amount is negligible |

If the reconciliation holds, we have jointly demonstrated something genuinely new: a
consumer application that pays for biodiversity data **in proportion to provable use**, with
attribution enforced by architecture rather than by good intentions.

If it does not hold, you will have learned something concrete about a commercial consumer's
actual consumption, at no risk.

---

## Appendix — Why you should believe the provenance claim

Everything above rests on us actually enforcing what we say we enforce. Two things we would
put in front of any technical reviewer:

**We found this in ourselves and published it.** The fabricated statuses in Part 1 are our
own agent's output, from our own production database. Our internal reconciliation documents
the failure, the 56 affected runs, and the remediation, including the parts still outstanding.

**Our checks are required to demonstrate they can fail.** Every verification contract in the
system has been deliberately broken to confirm it goes red, because a green check and an
inert check are indistinguishable from the outside. That discipline caught a bug in this very
work: our prose scanner initially searched for `" gb"` to detect fabricated genome sizes, and
matched the string **"GBIF"** — flagging an honest summary that cited its own source. A check
that fires on correct behaviour gets deleted by the first person it inconveniences, and the
deletion looks like cleanup.

We would rather show you that class of mistake than a clean deck.
