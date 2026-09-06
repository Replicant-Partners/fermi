# Contract — the seam join, and five silent writers

**Between:** the trust surfaces UI and the team that owns episodes, workspaces and
the loop instrumentation.
**Status:** **closed contract.** One column is the whole of Part A. Part B is five
writes that already have their sinks built.
**Blocks:** the composition view, and every retrospective on a workspace.
**Companion to:** `UX_CONTRACT_belt_outcomes.md`, which this follows in shape
deliberately — that one was one payload change and landed in a single pass.

---

## The one idea

> **Every arrow in a workspace's workflow diagram is an artifact crossing a seam,
> and every seam should pass through the gates.**

That is the whole model, and the platform is one column away from being able to
show it.

We had claimed agent-to-agent collaboration was unrecorded. **That was wrong**, and
the correction is what makes this contract small. Verified on workspace
`fcc3bff3-ff10-4011-a74c-2eb3d35cc142`:

| `message_type` | count |
|---|---|
| `chat` | 919 |
| `agent_invocation` | **14** |
| `execution_result` | **14** |

Paired, and carrying **structured typed payloads**, not prose:

```
@supply_chain_oracle {"task":"resolve_external_costs","stage_id":"sweet_tea_prep",
                      "inputs":[{"name":"green_tea","role":"consumable",...
→ ```json { "items": [ { "name": "green_tea", "unit_cost": 0.018, ... } ] }
```

So the seam is captured. The defect is that **the same interaction is recorded
twice and joined never:**

* as a **message pair** — what the Workflow tab reads
* as an **episode** — what the gates, ladder, grades and ledger act on

496 episodes exist from agents in that workspace. Nothing connects them to the 14
invocations. The consequence is exact:

**The Workflow tab can show what happened. The trace can show whether it was
verified. Nothing can show both for the same interaction — so the gates do not
protect coordination, they protect a parallel record of it.**

---

## Part A — `workspace_messages.episode_id` **[B]**

```sql
ALTER TABLE workspace_messages ADD COLUMN episode_id uuid;
CREATE INDEX workspace_messages_episode_idx
    ON workspace_messages (episode_id) WHERE episode_id IS NOT NULL;

-- Same for the blackboard, same reason.
ALTER TABLE workspace_outputs ADD COLUMN episode_id uuid;
```

**Not a foreign key**, for the reason migration 220 already established and
tested: the message may be written before the episode row lands, and a batched
insert must not have one bad reference reject unrelated rows. Check it, don't
constrain it.

**Written on `execution_result`** — that is the message that has an episode.
`agent_invocation` may carry it too where the caller is itself an agent mid-run;
`NULL` there is correct and final when the caller is a human.

### What we render the day it lands

* Every arrow in the composition view becomes **clickable through to its trace** —
  the sequence diagram and the verification ladder become one drawing at two
  zoom levels.
* A **refused artifact becomes visibly refused where the team sees it**, instead
  of only on a per-artifact page nobody opens.
* Workspace retrospection gets a substrate: *14 verified interactions* is a
  usable denominator; *933 unparsed messages* is not.

### The invariant worth asserting

An `execution_result` whose agent persisted an episode **must** carry
`episode_id`. If it may legitimately be null, the reason has to be a token rather
than an absence — otherwise "this hop was never verified" and "this hop's join
was never written" render identically, which is the collapse both contracts exist
to prevent.

---

## Part A2 — and it must be universal **[B]**

`agent_invocation` / `execution_result` are written by the workspace path. **Fermi
forecasts do not appear in the workflow diagram**, and the likely cause is that
that path writes an episode and no messages at all — a second write path for one
concept.

This is the same class of defect as grounding applying on one route and not
another, and the paper's sentence covers it: *a contract that applies on one
route and not another is not a contract, it is a convention.*

**The ask:** every agent invocation that occurs inside a workspace writes the
message pair, whatever entry point it came through. One helper, called from every
invocation site, rather than a convention each site is trusted to follow. If a
path deliberately does not, it should say so where the scan can see it — the
pattern `tests/episode_lineage_coverage.rs` already established.

---

## Part B — five sinks with no writer **[W]**

All five have their tables built. All five are empty. Under the verification
ladder each is **`Silent`** — opportunities exist and the sink has no rows — and
`Silent` is not `Inert` and is certainly not a pass.

| # | sink | rows | opportunities | what it unblocks |
|---|---|---|---|---|
| 1 | `route:{reason}` episode tag | **0** | 3,581 episodes | `route_outcomes` is a correctly-shaped, permanently empty view. Loop 4.B cannot turn |
| 2 | `workspace_intentions` | **0** | 265 workspaces | the strategist re-derives intent from a transcript on every decision |
| 3 | `composition_versions` | **0** | 220 multi-member workspaces | Loop 4.A has nothing to evolve; no composition has ever been versioned |
| 4 | `teams.resolved_at` / `resolution_outcome` / `brier_score` | **0 of 265** | every finished workspace | **no workspace has ever been retrospected.** The domain-constrained MoE has no closing bracket |
| 5 | `assertion_verifications` | **0** | 42 episodes with assertions | rejection rate; "nobody checked" vs "checked and fine" |

Item 1 is the cheapest and the most diagnostic. `stamp_invocation` already writes
`qsrc:` (18 episodes) and `ibind:` (90). **`route:` is written zero times**, so
the routing decision — the one thing Loop 4.B measures — is the one tag missing.
FEEDBACK_LOOPS §2 records 4.B as *"provenance stamped; views unread."* The views
are not unread. They are empty.

Item 4 is the one to prioritise if only one is done. Without a resolution there is
no ground truth for a workspace, so there is nothing for calibration to score and
no way to compare two compositions. It is the difference between a product that
improves and a product that runs.

---

## Why these are one document

Part A makes a seam **observable**. Part B makes it **judgeable**. Neither is
useful alone:

* the join without the writers gives a clickable diagram over ungoverned hops
* the writers without the join give scores nobody can trace to an interaction

And both are prerequisites for the thing the platform is for — **composing
multi-agent teams that improve via loops, and seeing the loops work.** Today the
artifact axis works and the composition axis has no data, and that asymmetry is
not visible anywhere in the product, which is how it survived three weeks.

---

## What we are building against this now

A composition view that draws the workspace as its seams, with each arrow
carrying its verification state. Until Part A lands **every arrow will read
`ungoverned`, with the reason** — which is honest, and puts the missing column in
the one place where its absence is expensive rather than in a document.
