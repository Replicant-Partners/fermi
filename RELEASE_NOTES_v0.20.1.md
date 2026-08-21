# v0.20.1 — a permission gate reading a column, and an invoker asserting its value

v0.20.0 fixed four defects in Loop 1 and closed with a claim about method:
every hop had been verified to have an executing call site. This release is
what happened when that claim was tested one hop further.

One code fix, in Loop 3. The more useful output is the revision to
`docs/architecture/FEEDBACK_LOOPS.md`, which names why the previous audit could
not have found any of the last four defects.

---

## The fix — the coherence shelf invoked a strategist the workspace never registered

`evaluate_coherence_handler` hardcoded `"cohere_and_coordinate"` in four places:
the registry lookup, the `AgentStmt` name, the credential resolution, and the
transcript attribution. It never read `teams.coordination_strategist_id` — the
column the rest of Loop 3 authorises on.

The first consequence is a missing feature. The platform ships three
alternatives — `pipeline_strategist` (ordered stages), `vote_strategist`
(consensus), `debate_strategist` (adversarial crux) — and all three were
unreachable. A workspace could be assigned one and the shelf would still invoke
the default.

The second is a live hazard, and it is the reason this is a fix rather than a
cleanup. `record_coordination_observation` is gated on *the caller must be this
workspace's registered strategist*, deliberately: writing into another agent's
episodic memory is a poisoning primitive and is the one tool where a missing
check is unrecoverable. So assigning any non-default strategist would make the
shelf invoke the wrong agent, and the coordination cascade would then **refuse**.

There is no error in that sequence. A permission denial is the system working
correctly. The feature would break at exactly the moment someone used the
configurability the column exists to provide, and in no other circumstance.

Measured today:

| | |
|---|---|
| Workspaces with a strategist | **260 / 260** |
| Distinct strategists in use | **1** |

The first confirms the v0.20.0-era assignment fix is holding — 11 workspaces
have been created since migration 211 backfilled 249, and all 11 were assigned.
The second is why the hardcode was invisible: the constant and the column have
never once disagreed.

The shelf now resolves the registered strategist, falls back to
`DEFAULT_COORDINATION_STRATEGIST`, logs a lookup failure rather than degrading
silently, and attributes the transcript message to whoever actually ran.

The regression test is a source check, for the same reason
`both_workspace_creation_paths_assign_a_strategist` is one. The failure was
never a wrong value — a value assertion would have passed on all 260
workspaces. It was a literal where a lookup belonged, so the test asserts that
only the constant may name the agent, and a companion test asserts the lookup
is still there, so the first cannot be satisfied by deleting the invocation.

---

## The revision — three ways a broken hop reports success

The 2026-08-15 audit verified that every hop in every loop had an executing
call site. That reads stronger than it is, and all four defects found since
slipped through it. `FEEDBACK_LOOPS.md` now names the three distinct
mechanisms:

**7. Called ≠ succeeded.** `create_snapshot` was called on every consolidation
and has no successes in the platform's history. A deliberately non-fatal
failure path, plus a bug that only fires on the first call, equals a function
that has never worked while every layer above reports that it did.

**8. One dependency, two resolutions.** When two code paths independently answer
*how is the extractor funded?*, only the path you happen to test is correct.
The remedy is not to fix the second copy but to delete it.

**9. Gated by data, invoked by a constant.** The defect above. Undetectable
while the data equals the constant — which is not reassurance, it is the reason
it survived.

A fourth status marker was added because the state had no name: **✖ Never
succeeded** — reached on every cycle, zero successes. It existed because
*wiring closed* was being read as *working*.

## A correction

`FEEDBACK_LOOPS.md` previously recorded *"snapshots: still 1, newest February —
`create_snapshot` fires on a consolidation cycle; none has run since deploy."*

That reasoning was wrong, and it is marked as wrong rather than replaced.
Consolidations had run, and had called it. The plausible explanation was
available and no one checked the stronger query:

```sql
SELECT count(*) FROM consolidation_jobs WHERE ontology_snapshot_id IS NOT NULL;
-- 0, across every job ever run
```

That is the cleanest number in the document. It does not say *no cycle has run
recently*; it says **nothing that has ever run on this platform produced a
snapshot.**

---

## Verification

```
fermi lib          591    api-server        197
```

The two new tests are the strategist source checks. `api-server` moved 195 → 197
and nothing else changed.

**What is not yet proven.** Same caveat as v0.20.0, and it is still the honest
one: none of the Loop 1 work has been observed to produce a snapshot in
production, because that needs a deploy and then a consolidation.
`FEEDBACK_LOOPS.md` §5 now states the post-deploy checks as queries that can
*refute* the diagnosis rather than as expectations:

1. `ontology_snapshots` — 1 row, `seed-034`, 2026-02-15. Expect a version 1.
2. `consolidation_jobs` with a non-NULL `ontology_snapshot_id` — currently 0,
   and the number that matters more, since it links a snapshot to the cycle
   that made it.
3. `prey_locator` — 93 episodes against 0 entities, 0 facts, 0 rules. The
   clearest available test of the creature-extractor fix.

Loop 1's observation and drift legs remain wired and waiting on traffic rather
than repair: 1,170 of 1,245 timeline entries sit at `persona_version = 1`,
which the drift monitor skips by design.
