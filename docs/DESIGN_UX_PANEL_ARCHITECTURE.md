# Design — the panel architecture, and the UI as an instrument

**Date:** 2026-08-23 · **Status:** design. Nothing here is built.

**Convention.** Every claim about the current system is marked:

* **[E]** — exists today, verified against the code in this pass, with file or symbol.
* **[B]** — must be built.

**Assumes** the gate decision ledger from `docs/AUDIT_loops_and_gates.md` §8 step 2
lands. `gate_trust::Retention::Recorded` already promises one row per decision in
`gate_decisions` **[E]**; no migration creates that table **[E — no match in
`migrations/`]**. §5 of this document is unbuildable until it exists, and says so
rather than designing around it.

---

## 0. The question, restated

Three questions arrived together, and they have one answer:

1. How does the UI survive moving to a phone, to AR, to a waveguide?
2. What register should it speak — naturalist, or control-system?
3. Can the design *catch the completing work* — can a panel written today against
   a loop that has never turned be correct today and still correct when it turns?

The third is the load-bearing one, and it inverts the usual order. The normal way
to build a UI is to render what the system has. The requirement here is the
opposite: **the absence of a reading must itself be a reading.** A loop panel that
shows nothing has made a claim, and the claim must be true, specific, and
attributable — otherwise the UI is a place where verification goes to die quietly,
which is the exact defect class this codebase has spent six months naming.

Answer §3 correctly and §1 and §2 mostly fall out of it.

---

## 1. The UI is an instrument, not a view

### 1.1 The failure classes are already named

`docs/architecture/FEEDBACK_LOOPS.md` enumerates nine ways a loop can look fine
and not be fine **[E]**. Four of them are *invisible at the surface by
construction*:

| Class | What the surface shows | Why the UI cannot tell |
|---|---|---|
| **Severed read path** | an empty panel | the loop wrote to `semantic_rules`; the panel reads `ontology_snapshots` |
| **Closed ≠ turning** | an empty panel | every hop has a call site; the corpus is not eligible yet |
| **Reachable ≠ reached** | an empty panel | a gate declined, correctly, 248 times |
| **Called ≠ succeeded** | an empty panel | the callee failed non-fatally on every call since inception |

Four different diagnoses. One rendering. **[E]** — and that single rendering is
today's `"No data yet"`.

That is the whole problem. A UI that renders these four identically is not
neutral about them; it actively converts a verification signal into a shrug.

### 1.2 No panel authors its own emptiness

**The rule.** A panel never writes its own empty-state string. It receives a
stamped `absence` from the server, and the server derives it from the trust
contract family — the same substrate the audit interrogates.

The family exists **[E]**:

| Contract | Question | Answers the absence-question |
|---|---|---|
| `src/schema_trust.rs` | is the column present? | *the panel reads a column that does not exist* |
| `src/rollup_trust.rs` | is the column telling the truth? | *the value is present and false* |
| `src/grounding_trust.rs` | could this value have been known? | *the value is unsourced* |
| `src/port_trust.rs` | does the invocation match the declared port? | *the caller was sending the wrong thing* |
| `src/liveness_trust.rs` | does the write path ever run? | *the table is empty because nothing ever wrote* |
| `src/gate_trust.rs` | what refused, and what does silence mean? | *the loop was reachable and never reached* |

`liveness_trust`'s own header states the case better than this document can:
*"None of them can see a table that is empty because nothing ever wrote to it."*
**[E]** That is precisely the distinction between *severed read path* and
*closed-not-turning*, and it is already computed.

`gate_trust` goes further and pre-authors the prose. `GateSpec.if_never_refuses`
is a **required** field — *"What it means if this gate refuses nothing… A count of
zero refusals is only alarming if someone has written down why"* **[E]**. And
`gate_trust::reading()` already returns one of
`never_asked | refuses_everything | admits_everything | null` **[E]**.

So for gates, the hardest empty state in the system is already a stamped enum with
a human sentence attached. The UI's job is to render it, not to invent it.

**[B]** is the generalisation: every panel kind declares which contract answers
its absence, and the stamp carries the answer.

### 1.3 The completion test

The criterion that makes §0.3 checkable:

> **A panel written today against a loop that has never turned must render
> something true and useful today, and require no code change when the loop starts
> turning.**

If the panel has to change when the loop turns, the panel encoded an assumption
instead of reading a state.

Applied to the live numbers in `AUDIT_loops_and_gates.md` §1 **[E]**:

| Loop | Today | What the panel must say today | On first turn |
|---|---|---|---|
| 1 agent learning | turning | rate, volume, yield | unchanged, numbers move |
| 2 HITL correction | zero rows at **every** stage | *"never turned — the seed cannot write"*, naming the FK violation, linking the site | unchanged, numbers move |
| 3 workspace coherence | first stage only | *"turns to `coherence_evaluations`, stops there"* — the blockage located on the chain | unchanged |
| 4 composition evolution | zero rows at every stage | *"never turned"* + the phantom-tool note | unchanged |
| 5a Brier | turning | rate, skill vs base rate | unchanged |
| 5b projection | never had an input | *"no input has arrived"* — distinct from *"broken"* | unchanged |

Six loops, six different true sentences, one renderer, no rewrite pending. If a
proposed panel cannot pass this table it is not ready to build.

### 1.4 The corollary nobody likes

The UI acquires the right to fail loudly. If the stamp cannot be produced — the
probe errored, the contract is unimplemented for that panel kind — the panel
renders `⊘ unmeasured` and says which probe failed. It does **not** fall back to a
blank, a spinner, or a zero. The Observatory already does this and says so out
loud: *"Loop health could not be read…: no verdict is shown rather than a guessed
one."* **[E]** That sentence is the design principle for the whole product.

---

## 2. State vocabulary

> **Built 2026-08-23.** This section proposed two new enums. On reading the tree,
> `src/loop_model.rs` already owned most of the vocabulary — a chain model with a
> six-reason `diagnose()` delegating to `gate_trust` and `write_accounting`.
> Adding a parallel vocabulary beside it would have been *one dependency, two
> resolutions*, the defect class `FEEDBACK_LOOPS.md` §8 names. So the work landed
> as an extension of `loop_model` instead, and §2.4 records what changed.

Two orthogonal axes. Conflating them is what produces `0.0%` for both *"measured
zero"* and *"no source for this number"* on the same panel today **[E —
`agent_detail.html` Performance Statistics mixes measured `execution_stats` with
hand-authored `agent_card.json` `performance`]**.

### 2.1 Mechanism state — is the thing wired?

| State | Meaning | Example today |
|---|---|---|
| `enforced` | it runs and it can refuse | grounding on the creature handlers **[E]** |
| `observed` | it runs and is discarded | grounding on `/execute` — *"a metric, not a control"* **[E]** |
| `declared` | typed, persisted, exposed, never compared | `min_tier` / `capability_gates` **[E]** |
| `absent` | no implementation | — |

This axis is the gate register's most important column, and it generalises past
gates: the whole `hud_contract` is `absent` in production while being `enforced` in
tests **[E]**.

### 2.2 Reading state — what does the measurement say?

| State | Meaning | Must not be confused with |
|---|---|---|
| `measured` | a value, with `n` | — |
| `zero` | measured, and the answer is zero | `awaiting_input` |
| `thin` | `n` below where the value means anything | `measured` |
| `awaiting_input` | mechanism sound, corpus not yet eligible | `broken` |
| `unmeasured` | the probe errored | `zero` |
| `inapplicable` | correctly empty for this subject | `unproductive` |

The last row is not hypothetical. The fleet dreaming panel already distinguishes
**unproductive** (a fault) from **unused** (a correct empty result) and prints a
note explaining the difference **[E]**. That distinction is right, and it is
currently made in exactly one panel. Promote it to the type system.

### 2.3 The cross-product is the diagnosis

The pair `(mechanism, reading)` maps onto the named defect classes without further
invention:

| Pair | Diagnosis |
|---|---|
| `enforced` + upstream `measured`, downstream `zero` | **blockage** — locate on the chain |
| `enforced` + `zero` at every hop | **never turned** — read `if_never_refuses` |
| `observed` where the paper claims a control | **metric masquerading as gate** |
| `declared` + any | **dead gate** |
| `measured` upstream + `unmeasured` downstream | **severed read path** |
| `enforced` + `awaiting_input` | healthy and idle — say so, calmly |

Six diagnoses, each with an existing precedent in the audit, each renderable at
every density including two words on a waveguide.

### 2.4 What landed, and the hole it closed

`loop_model` carried the sentinel `StageState::rows = -1` for *"the count query
did not run"*, documented as **"never confused with zero"** — and then the walk
confused it with *success*. `rows == 0` was the only condition that stopped a
chain, so:

* a loop whose first probe errored while later stages held rows reported
  **`turning`**, with no stall and no reason;
* a genuinely empty stage under an unread one was diagnosed **`awaiting_upstream`**
  — a confident claim about a stage nobody had read, pointing a reader at the
  wrong link.

Both are the defect the module exists to prevent, committed in its own walk. And
both propagated: `native_evaluators` counted turning loops with
`stops_at.is_none()`, so an unread loop was evidence of health in `PositiveControl`
and was absorbed by `LoopStalledInCode`'s *"the rest are idle rather than broken"*
— a positive claim about every loop it had not named.

Changed **[E, this pass]**:

| | |
|---|---|
| `loop_model::Upstream` | tri-state `Produced / Empty / Unknown`, replacing the `bool` in `diagnose()` |
| two reasons | `probe_failed` (this stage's probe), `upstream_unmeasured` (a stage above) |
| `LoopState::status` | now `turning \| stalled \| unmeasured` — the third is neither of the first two |
| `StageState::measured()`, `LoopState::measured()` | so no caller re-derives the sentinel |
| `LoopStalledInCode` | returns `Inconclusive` naming the unread loops rather than claiming idleness |
| `PositiveControl` | an unread loop is not evidence that anything works |
| `/admin` ops endpoint | `loops_unread`, named beside `loops_stalled_in_code` |

Validated against the live chain: 3/3 contract tests pass, 2 of 6 loops turning
(`loop1`, `loop5a`), every probe running — so the change is a no-op on today's
data and diverges only when a probe fails, which is the correct shape for it.

### 2.5 The ledger, and what a bounded audit queue costs

`Retention::Recorded` had promised a row in `gate_decisions` since the gate
audit; no migration created the table, so the promise had no referent and every
counter died at each deploy. Migration 214 creates it. **[E, this pass]**

The recording path had one hard constraint: `decided()` documents itself as
*"never fails, never blocks, no I/O"*, and every gate in the system calls it.
So Recorded-tier decisions go onto a bounded in-memory queue, drained by a
background recorder in one `UNNEST` batch through `write_accounting` — the rung
composing in the direction the module docs already asked for: *the thing that
watches the gates is watched by the thing that watches the writes*.

Four decisions, each of which is a cost made explicit rather than avoided:

| | |
|---|---|
| the queue is **bounded** (4096) | an unbounded audit queue behind a dead recorder is a memory leak that presents as a healthy gate |
| it drops the **oldest** | a full queue means the recorder is behind or dead, and in that state the recent refusals are what someone is about to look for |
| every drop is **counted** | a bounded queue that drops silently is worse than an unbounded one: the ledger reads as complete while holding a hole |
| a failed flush **requeues** | a transient database error losing the batch is the exact failure the table exists to stop |
| the recorder is **non-fatal** | a gate must not fail because its audit trail cannot write — that turns an observability outage into a refusal of service |

Both closed columns are registered in `seam_vocabulary` against
`gate_trust::DECISIONS` and `GATE_IDS`, indexed rather than restated. An
unregistered `CHECK` on a closed set is the `severity = "L1"` setup exactly, and
this writer is non-fatal, so the drift would be swallowed. Verified against the
live schema in a rolled-back transaction: the migration produces exactly
`gate_decisions_decision_check` and `gate_decisions_gate_check`, the two names
the registry asserts.

`gates.decisions` came off the §1.2 ratchet as a result — six unresolved panels
down to five. It now resolves from the ledger's own status: dropped ⇒ **fault**
(outranking everything, with a remediation), pending ⇒ idle but trailing the
counters, otherwise idle *while naming that five of seven gates are memory-only
and never reach that panel at all*.

### 2.6 `unobserved`, and the benign default

Written after §2.9 was acted on — by a second author, concurrently, which is why
it is worth recording.

§2.9 said the diagnosis is uptime-dependent and under-reports in the benign
direction. `loop_model` now answers that at the right level: a stage whose
write-accounting sink has **zero attempts** reports `unobserved` rather than
`no_input`, because *"the trigger had its chance and there was nothing to do"* is
a positive claim that a freshly booted process has no evidence for. On the live
walk this immediately reclassified Loops 2, 4 and 5b.

It also exposed a defect in `panel_absence`. The reason-to-`Reading` map ended
`_ => Reading::Idle`, so the new token fell through to **idle** — turning
*"nothing has been watched"* into *"the system is fine"* on every loop-backed
panel. A benign catch-all is how an upstream vocabulary change comes to report
health.

Two remedies, and the second is the general one:

1. `unobserved` is classified `Unknown`, with `probe_failed` and
   `upstream_unmeasured`.
2. `loop_model::STALL_REASONS` now **declares** the token set, and
   `every_stall_reason_is_classified` fails if a reason has no explicit arm.
   Because a deliberate `Unknown` and a fallthrough are identical by return
   value — which is exactly how this hid — classification returns `Option` so
   the test can tell them apart, and an unrecognised token now resolves to
   `Unknown` rather than to optimism.

The lesson generalises past this instance: **a closed token set that crosses a
module boundary needs a declaration and a ratchet, not a match arm.** That is
the seam registry's argument, one layer up from the database.

### 2.7 A coarse contract cannot answer a scoped question

Found by running the live report for the first time — the third instance of the
same defect in one session, and the reason it is worth naming as a class.

`observatory.learned` rendered:

> The write path has run (253 rows); this panel is empty for a narrower reason.
> **Consolidation has produced no rules.**

Both halves true, the conjunction nonsense. The cause is a scope mismatch:
**liveness contracts and the loop model both count rows platform-wide.** They
answer *"does this writer ever run"*. Most panels are agent-scoped and ask
*"should THIS agent have a row"*. For a platform-scoped panel those coincide;
for an agent-scoped one they do not, and a healthy platform verdict was being
read as an answer.

The reading it produced was `idle` — the benign default, for the third time. The
pattern across all three:

| instance | coarse thing | benign default it produced |
|---|---|---|
| §2.4 | `rows == -1` treated as not-zero | a loop with a failed probe read `turning` |
| §2.6 | a `_ =>` match arm | a new upstream token read `idle` |
| §2.7 | platform-wide row counts | a scoped panel read `idle` |

Each time the mechanism differs and the direction does not: **when a check
cannot answer, the cheapest thing to return is the reassuring one.** That is
what the whole module is against, and it re-enters through a different door
every time.

The fix is `answers_scope`: a platform-scoped contract resolves only
platform- and account-scoped panels. Everything narrower gets `out_of_scope` /
`Unknown` until a scoped resolver exists. A `SILENT` writer is still a fault at
every scope — nothing anywhere is evidence about everywhere.

**The honest cost, measured:** unexplained panels went from 7 to 13 of 18. Six
readings that looked like knowledge were not. That is the number moving in the
right direction, and §8 predicted it.

### 2.8 Scoped resolution, and the failure that runs the other way

§2.7 left 6 of 18 panels `out_of_scope` — correct, and useless, since nearly
every panel in the real UI is agent-scoped. `ScopedProbe` closes it: the same
two counts as a liveness contract, parameterised by subject, with the verdict
going through `liveness_trust::classify` so the **arithmetic keeps one
implementation** and only the SQL is per panel. `resolve_for_subject` runs the
probe; the pure snapshot path stays the testable core. **[E, this pass]**

Ten probes now cover every scoped panel except two, both pinned with reasons:
`observatory.loops` (the loop model counts platform-wide and cannot yet be
walked per agent) and `observatory.anomalies` (platform-SILENT against 1,418
opportunities — a fault at every scope, needing no narrowing).

**Then it produced two false faults, and the direction is the point.**

The first live scoped run reported `agent.claims` and `agent.assertions` as
`silent` for an agent with 300 episodes. Both were wrong. My probes counted *all
300 episodes* as chances to record a quantified judgement; the platform contract
counts only episodes whose response carries a `Suggested p50` bound to a
workspace or forecast. The real count is **zero**, so a correct `inert` was
reported as a fault.

This is the mirror of §§2.4/2.6/2.7. Those defaulted to reassurance; this one
defaulted to alarm. Both come from the same root — **a second definition of a
question that already had one** — and the alarming direction is not the safer
one: the paper's §5.2 is explicit that a check which cries wolf gets deleted, and
the deletion looks like cleanup.

The rule, now enforced: **a scoped probe inherits the platform contract's
opportunity definition and adds only a subject filter.**
`ScopedProbe::inherits_opportunity_from` names the sink it narrows, and a test
fails if that sink is not declared. Six of the ten probes were misaligned on
first writing; all six now inherit. Both false faults became `inert`.

The live scoped report for `fermi` (300 episodes) is the payoff: dreaming,
learning, timeline, rule retrieval, evals and dyads all read `ok` with their
write/opportunity ratios visible, and the two empty panels read `inert` with the
reason they are empty stated in the sentence.

### 2.9 The diagnosis is uptime-dependent, and the UI must say so

Found while validating. `write_accounting` and `gate_trust` counters are
`AtomicU64` statics — **in-memory, per-process, reset on restart** **[E]**, and
deliberately so: *"a ledger that is itself a fallible database write is most
silent when it is most needed."*

The consequence for `diagnose()` is that `writes_refused` and
`gate_refuses_everything` are only reachable in a process that has attempted the
write. The live run above reported Loop 2's `anomaly` stage as **`no_input`**,
when `AUDIT_loops_and_gates.md` §2.1 establishes that the seed **cannot write** —
an FK violation on every attempt. Both readings are honest; they differ because a
fresh test process has attempted nothing.

So a panel served by a recently-restarted web process will under-diagnose, and it
will do so *silently and in the benign direction* — the worst available direction.
Two requirements fall out, and they are not optional:

1. Every panel carrying a `no_input` or `no_trigger` reading must also carry
   **process uptime and attempts-since-boot**. `no_input` after four seconds is
   not the same claim as `no_input` after four days.
2. `gate_decisions` (§7 step 3) is what makes the strong readings survive a
   restart. Until it exists, the Gates page reports *since boot* and must say
   `since boot` on its face.

---

## 3. The panel contract

### 3.1 Where it lives

`src/hud_contract.rs` **[E]** — ~1,000 lines, pure, tested, zero production call
sites. It already computes markers, provenance tags and confidence bands
server-side and ships a stamped card that a dumb shell copies to screen. That is
the panel contract, built for one card kind and one device.

From `glasses/hud_field_scout/README.md` **[E]**:

> **It decides nothing.** Every marker, provenance tag and confidence band is
> computed by `src/hud_contract.rs` server-side and arrives already stamped. The
> shell copies them to the screen. That split is the point. […] The glasses are I/O.

**[B]** Generalise it from one kind to eight, and from one density to three. Two
things fall out: the largest piece of dead capability in the tree acquires a real
caller, and the web UI stops being the privileged renderer.

Note the honest hazard, stated in `DESIGN_gates_as_a_service.md` §1 about the same
file: *"a gate whose first user is a paying stranger has not been operated."*
**[E]** Making the **web UI** its first user is the correct order — operate it
where we can see it fail.

### 3.2 Eight kinds

Closed set. An open vocabulary costs one implementation per renderer per addition.

| Kind | What it is | Absorbs today |
|---|---|---|
| `card` | identity · contract · evidence · availability | agent card, creature card, app tile, workspace tile |
| `register` | sortable rows under a lens | catalogue, ecology register, patient register |
| `reading-grid` | *n* Readings | vital signs, performance stats, census strip |
| `verdict-list` | glyph + word + provenance | loops tab, ABW conformance, publish checks |
| `flow` | topology with measured rates | **[B]** — §4.1 |
| `fitting` | slotted assembly under constraint | **[B]** — §4.2 |
| `queue` | things awaiting a decision | HITL, consensus, proposals, unreviewed members |
| `record` | time-ordered events | execution history, eval runs, timeline |

Every surface in the current product is one of these eight.

### 3.3 Three densities, not breakpoints

Every Reading declares itself at three densities. The server stamps all three; the
renderer picks by surface.

| Density | Budget | Target |
|---|---|---|
| `glance` | one glyph, one number, ≤60 chars | waveguide, AR world-label, watch |
| `scan` | label · value · verdict glyph · sparkline | phone, register rows |
| `study` | mechanism badge · `n` · spread · `← source` · `?` explainer | desktop |

The glasses shell is therefore not a degraded web page; it is the `glance` tier of
the same stamp, honest by construction because the same code produced both.
`tests/glasses_shell_parity.rs` **[E]** becomes a *renderer parity* suite: one
stamp, asserted across renderers, with the six reading states of §2.2 as explicit
cases.

Designing for `glance` first is a discipline, not a device concession. A reading
that cannot survive two words has not been understood.

### 3.4 Relations, not coordinates

Panels declare relations — `summarises`, `evidence_for`, `blocks`, `feeds` — never
positions. Each renderer resolves relations its own way: desktop docks and grids,
phone stacks with a bottom sheet, AR anchors the summary on the object and pulls
evidence to a panel. Layout is authored once per renderer, not once per screen.

### 3.5 One verb registry

Every action is a named command. Ctrl+K on desktop (Xaman Ek exists **[E]**),
long-press radial on phone, gaze+pinch on glasses. Buttons are shortcuts *into* the
registry, never a parallel path.

Two payoffs, and the second matters more: actions stay consistent across surfaces
for free, and there is exactly **one chokepoint** at which to run gate checks and
emit receipts. Today the gate call sites are scattered and the refusals are
unrecorded; a single verb path is the cheapest place to fix that permanently.

### 3.6 The contract was built beside `hud_contract`, not inside it

§3.1 said to generalise `hud_contract` from one kind to eight and one density to
three. On reading it, that was wrong, and the reason is the one this document
keeps rediscovering.

`hud_contract` answers exactly one question — *"can the wearer SEE which answer
is which"* — and answers it well: `Treatment` maps the platform's five-value
provenance ladder to ASCII markers chosen because the target optics reproduce
"one luminous green channel over pure black", and because the same string has to
survive TTS and a log file. Panel **kind** and **density** are a different axis.
Welding them together would give the platform two vocabularies for the
provenance question, and the module's own header says why that is the specific
thing not to do: *"Minting a second vocabulary would give the platform two
provenance channels, and the one that nothing checks is the one a fabrication
moves to."*

So `panel_contract` owns the ladder and **delegates every treatment decision**.
It still achieves the goal that motivated §3.1 — `hud_contract` now has a
production caller, operated where we can watch it, which
`DESIGN_gates_as_a_service.md` §1 argues is the right order for a gate whose
alternative first user is a paying stranger.

**The rule the module inherited.** `Treatment::marker` documents it:

> `Verified` gets no marker: the unmarked case must be the trustworthy one, so
> that a marker always means "read this more carefully" and a renderer that
> drops markers degrades to *less* confident rather than more.

Density is that rule on a second dimension. `Glance` discards almost everything,
and what survives must never imply more confidence than `Study` would. Two
tests hold it: `an_absence_is_never_unmarked` (an empty panel can never reach
the unmarked trustworthy case — the presentation-layer form of the
`genome_profiler` incident) and `glance_never_reads_safer_than_study` (reading,
marker and token are identical across densities; a fault stays a fault at one
line).

`Reading` maps onto `Treatment` rather than minting glyphs, and the fit is close
to exact: `Idle` → `NoMatch` (*consulted, and had nothing for this subject*),
`Fault` → `Rejected` (*checked, and found wrong*), `Unknown` → `Unavailable`
(*nothing could supply it*).

**The glance tier, live.** 60 characters is enough:

```
| x anomalies: silent          |   ← the one thing actually wrong
| ? claims: inert              |   ← correctly empty
| ! coherence: awaiting_agent  |   ← nobody can say
```

Truncation is visible (`…`) on purpose: a sentence that silently loses its
qualifying clause reads as a flatter claim than it is.

### 3.7 The registry declares before it routes

§3.5 wants one verb path so every surface invokes the same named command and
gate checks have a single chokepoint. That is two pieces of work, and only the
first is built. **[E, this pass]**

The declaration turned out to be the valuable half on its own, for a reason that
has nothing to do with palettes. The router already knows every route; what it
cannot say is **which routes change something, and which gate stands in front of
that change**. That is the question the gate audit had to answer by reading
code, and its §3 table — *gates that are computed and discarded* — was assembled
by hand, correct on the day, and kept current by nothing.

Each of those findings is a property of a **verb**, not of a gate and not of a
route, and there was nowhere to write it down:

| finding | now |
|---|---|
| grounding is a control on the creature handlers and a **metric** on the two execute endpoints a third party calls | `Enforcement::Metric` on `agent.execute` and `agent.execute_stream`, with the cost stated |
| `input_binding` is advisory by design | `Metric`, with the reason it is not fatal |
| a write nobody gated | `ungoverned_writes()`, asserted empty |

`Enforcement` is §2.1's mechanism axis, which had been specified and never
built. This is where it belongs: the axis is a property of how a gate is applied
*on a path*, so a verb is the thing that carries it.

Three rules are enforced:

1. **A write names a gate that can refuse it, or says why it needs none.**
   `agent.archive` says withdrawal is the safe direction and a gate that could
   refuse it would trap an owner with a bad agent in production. `billing.checkout`
   says the gate is Stripe's, and that a credit gate in front of buying credits
   is the deadlock it sounds like.
2. **A demoted gate says what the demotion costs.** A gate turned into a metric
   is a decision somebody made; an undocumented one is indistinguishable from
   drift.
3. **The discarded-verdict list is pinned and may only shrink** — seeded
   non-empty with the three real cases, because a governance registry whose
   first run is green has not been pointed at anything.

And one fence, because this table is exactly the kind that rots: every declared
route must appear in the router's source. A registry pointing at a renamed route
reads as current while governing nothing — the `fermi_leaderboard` shape, where a
probe that could never return healthy was ignored for eight releases.

**What is not built:** the chokepoint. Gate decisions are still made at four
scattered handler sites, and surfaces still build their own buttons. Routing
through the registry is the step that makes receipts automatic, and it should
not be attempted until the declaration is trusted.

### 3.8 What Rabble is, and what it is not

Rabble is a **client**, not a renderer of this contract. It consumes the workspace
API and decides its own UX; that was the design intent and it is the right
boundary. Consequences:

* No cross-language model, no Dart codegen, no shared component library. **The API
  is the contract with Rabble; the panel contract is internal to our renderers.**
* The phone target for *this* product is a responsive web view until evidence says
  otherwise.
* Therefore there are two renderers to build against, not three: web (`study` +
  `scan`) and glasses (`glance`).

If Rabble later wants panel semantics rather than raw API, it can consume the
stamp as JSON — which is the point of stamping server-side.

---

## 4. The two panels that carry the feel

The design brief is MMOG thinking: agents playable like cards, compositions
snapping like Lego, ports and maps making building fun, flows visible so the system
has life. Two panel kinds do nearly all of that work.

### 4.1 `flow` — loops as living topology

`AUDIT_loops_and_gates.md` §1 already holds each loop as a chain with volumes:
`episodes → consolidation_jobs 282 → semantic_rules 253 → application_count 37`
**[E]**. Render the chain, not a list of verdicts.

* edge **thickness** = volume through the hop
* edge **motion rate** = measured rows/day
* a hop with volume in and zero out is a **visible blockage** — Loop 2's
  `anomaly_events 0`, Loop 3's stop after `coherence_evaluations`
* **gates sit on the edges as valves**, coloured by allow/refuse ratio, opening the
  receipt on selection

This makes gates and loops one picture rather than two dashboards — which is the
true relationship, since a gate is the point in a control cycle where the
correction is permitted or refused. Loop 2 has never turned *because* the gates on
its path have no surface.

At `glance` it collapses to one number: turns/day.

### 4.2 `fitting` — composition as assembly under constraint

Hull = workspace. Slots = roles. Modules = agents. Powergrid = credit budget.
Studs = `accepts` / `produces`.

Drag an agent in and Γ(C) moves live, the budget bar depletes, an incompatible port
refuses **visibly, before commit**. That is the control gate as direct manipulation
rather than an HTTP 422 after the fact, and it gives Loops 3 and 4 their first home
with actual verbs.

Two honesty constraints, both from `scripts/port_census.py` as recorded in
`templates/ecology.html` **[E]**:

1. **Studs are mostly labels, not types — and the ratio has already moved.**
   The `ecology.html` census (2026-08-15, 100 curated cards) recorded seven
   declaring an `output_contract` and **none carrying a schema**; they carried
   `produces_schema`, a string *naming* one. The panel that rendered that name
   under the heading "Schema" certified seven cards as typed when none were,
   *inside the caveat box built to prevent exactly that* **[E]**.

   Measured live on 2026-08-24, across all published agents: **10 declare a
   contract and 3 carry a real, resolvable JSON Schema** — `genome_profiler`
   has a complete draft-2020-12 document with `enum`-constrained provenance
   fields. **[E]**

   That the number moved and nobody noticed is the argument for this whole
   section. A census in a comment is a measurement taken once and then believed
   indefinitely; §4.2 said the ratio has to move on the panel when the
   vocabulary converges, and it had already moved before the panel existed to
   show it. The Bestiary's Population lens now measures it on every load.

   So: render **asserted studs hollow and verified studs filled**, and key
   "filled" on a *resolvable* schema, never on the presence of the block — which
   now actually lights up for three specimens instead of being dead code.

2. **The vocabulary is fragmented, and this is the finding that must land first.**
   99 cards declare `accepts` and 99 declare `produces`, across **513 distinct
   labels — of which only 14 appear on both sides. 499 labels cannot form a seam
   with anything** **[E]**.

The second number is a trap for this design. A naive Lego rendering would present
a bin of bricks that overwhelmingly do not connect, and read as a broken panel
rather than as a fragmented vocabulary. So `fitting` must not ship before the
Population lens surfaces label-set health as a first-class reading: *orphan labels*,
*seam-forming labels*, and the ratio. **The composability problem is a naming
problem, and the UI's first job is to say so** — not to offer a drag-and-drop
surface that silently fails to snap.

Composability you can see is the feature; composability whose confidence you can
see is the product. When `card_contract` typing lands and the label set converges,
studs fill in and seams appear with no UI change — §1.3 applies to feel as well as
to data.

### 4.3 The card grammar

Fixed positional grammar, learned once, readable on any card:

| Zone | Content | Note |
|---|---|---|
| cost corner | tier · `min_tier` · credits | always the same corner, so a register is scannable by cost alone |
| type line | `agent_type` — taxonomy | the seven-rank `taxonomy` is on `AgentCard` and **never reaches the client** **[E]** — this is where it goes |
| body | I/O contract, as studs | currently buried at the foot of a specimen sheet under a disclaimer **[E]** |
| evidence pair | evolution rank · forecast skill | with `capped_by` and *"Untried, not failing"* preserved |
| provenance mark | approved / curated_seed / admin_grant / implicit | Ecology already draws this dot **[E]** |

The test is the Magic test: **the card is sufficient to decide with, and there is
no separate manual.**

### 4.4 The motion rule

> **Nothing animates unless something measured moved, and the animation rate is the
> measured rate.**

A loop that is not turning is *still*. A gate never asked is *grey*. Pulse encodes
rows/day and nothing else.

This is what keeps game-feel from degrading into decoration, and it is continuous
with the epistemic discipline already running through these documents — `[V]` vs
`[R]`, *closed ≠ turning*, *wired ≠ reached*. It is also, incidentally, the most
game-like decision available: in an MMOG a dead region **looks** dead. Stillness is
a readout.

---

## 5. The register — naturalist and cybernetic are one language

### 5.1 They are not in tension

Systems ecology *is* cybernetics. Bateson's *Steps to an Ecology of Mind*, Odum's
systems ecology, Holling's adaptive cycle, Ashby's requisite variety — observational
natural history is where the control vocabulary for complex adaptive systems came
from. The naturalist register is not a costume over a control system; it is the
older name for the same discipline.

So neither language hides. They divide by job:

* **Naturalist words for things and observations** — specimen, register, habitat,
  niche, stratum, population, field notes, care plan, dyad, *incertae sedis*
* **Cybernetic words for mechanisms and verdicts** — loop, gate, refusal, drift,
  coherence, closed / partial / open / broken / unmeasured, turn rate

A specimen sheet reports a care plan; a loop reports a verdict. Both on the same
page, neither translated into the other.

### 5.2 The real finding

This vocabulary already exists and is good. The failure is that **only Ecology and
the Observatory speak it.** The dashboard speaks SaaS: *Portfolio*, *Recent
Transactions*, *Platform Activity* **[E]**.

Consistency of register across the whole product is the cheapest anti-enterprise
win available. It costs renaming.

### 5.3 Lexicon

| Concept | Term | Register | Status |
|---|---|---|---|
| the population | Bestiary | naturalist | exists, is the product name |
| one agent | specimen | naturalist | exists in Ecology only |
| the list | register | naturalist | exists in Observatory only |
| health surface | Observatory | naturalist | exists |
| per-agent prognosis | care plan | naturalist | exists |
| control points | Gates | cybernetic | **[B]** — no surface |
| a refusal | receipt | cybernetic | **[B]** — `gate_decisions` unbuilt |
| the control cycles | loops | cybernetic | exists |
| loop verdicts | closed / partial / open / broken / unmeasured | cybernetic | exists — keep verbatim |
| the queue + resume | **Rounds** | clinical | **[B]** — settled 2026-08-24 |
| embedding sales | *(tabled)* | — | see §5.4 |

**Rounds**, settled. A clinician's round is an ordered visit to whoever needs
attention, repeated on a cadence — both halves of this surface in one word, where
"Dashboard" carries neither. The Observatory already declares the register this
implies: its subtitle is *"Clinical practice — agent health & behaviour"*, its
agent list is a **patient register**, and each agent has a **care plan**.
`Rounds` is the verb that vocabulary was already implying and never named, so it
costs no new concepts.

It also puts the information architecture in the noun. A dashboard is a wall of
equal-weight panels — which is the diagnosis, ~30 actions at identical visual
weight. A round is *sequenced by who needs you most*.

Plural: "round" is one pass, "rounds" is the practice.

The rejected alternative was `Bench` — warmer, pairs with Bestiary and Ecology
rather than Observatory, and weak on exactly the half that matters: a bench holds
what you left on it, it does not tell you what is urgent. This surface exists
because actions were unclear, so the ordering word wins.

**The spine is therefore: Bestiary · Observatory · Gates · Rounds.**

### 5.4 `marketplace` is tabled, and what that costs later

Settled 2026-08-24: `/api/marketplace` and `templates/marketplace.html`
(currently surfaced as *Similarity Lab*) are a **placeholder for selling
embeddings**, and the feature is tabled until the platform works as it should.
It takes no spine slot.

Recorded here rather than left implicit because tabling is a decision with two
consequences, and the first is immediate:

1. **It keeps a nav slot today.** `nav.js` ships Catalogue · Apps · Similarity
   Lab · Docs while the Observatory, Ecology, workspaces and the review queue
   appear in no navigation at all **[E]**. A tabled placeholder outranking the
   strongest surface in the product is the clearest single instance of the
   sprawl this document was opened to address. Removing it is part of step 6.
2. **When it is untabled it is a money-and-data write, and it has no entry in
   `command_registry`.** Selling an embedding moves credits *and* exports a
   corpus, so it will need a Credit gate and something the platform does not yet
   have: a gate on embedding provenance. `docs/EMBEDDING_PROVENANCE.md` and the
   agent-detail export path already distinguish *safe* (source corpus +
   provenance) from *invertible* (raw vectors, consent-gated) — and a
   marketplace is precisely where that distinction stops being an export dialog
   and becomes a product surface. Whoever untables it should add the command
   first and let `no_write_is_silently_ungoverned` fail until the gates exist.

That second point is the reason to write this down at all. A tabled feature
returns without its context, and this one returns as an ungoverned write.

### 5.5 Rename, don't invent

Do not import Magic or EVE vocabulary on top. The references are for *grammar* —
fixed card positions, fitting-under-constraint, the overview as primary instrument,
immediate connection — not for words. The words are already right.

---

## 6. The spine

Four destinations, in register, all four named as of 2026-08-24. (The
information-architecture argument behind this table is not yet written up as its
own document; this section records the conclusion, not the derivation.)

| Destination | Question | Absorbs |
|---|---|---|
| **Bestiary** | what is running? | catalogue, ecology register, dashboard's agents/compositions/apps tiles — one register, three lenses (Discover · Population · Health) |
| **Observatory** | is it converging? | unchanged in grammar, promoted to nav, extended to composition scope |
| **Gates** | what needs deciding? | HITL, consensus, publish blocks, proposals, membership review + the gate register |
| **Rounds** | what needs me, in what order? | replaces the dashboard-as-directory — the decision queue, what changed since you last looked, and resume. See §5.3 |

Currently the nav ships Catalogue · Apps · Similarity Lab · Docs, and the
Observatory, Ecology, workspaces, the projector and the HITL queue appear in **no
navigation at all** **[E]**.

---

## 7. Build order

1. ~~**State vocabulary in Rust** (§2)~~ — **done 2026-08-23**, as an extension of
   `loop_model` rather than a new module. See §2.4.
   Follow-on, not yet built: a **turn rate** per stage. `status: "turning"` is
   currently derived from a cumulative count, so it cannot distinguish *turned
   200 times last month, nothing since* from *turning now* — the `closed ≠
   turning` distinction one level deeper, and a hard prerequisite for the motion
   rule in §4.4. It needs a `rate_sql` per stage, so it is ~20 authored queries
   and a ratchet test, not a refactor.
2. ~~**Absence resolution** (§1.2)~~ — **done 2026-08-23**, `src/panel_absence.rs`.
   A routing table from each surface that can be blank to the contract that
   explains it, plus a `Reading` of `idle | fault | unknown` as the only thing a
   renderer branches on. 18 panels declared; 6 currently unresolvable, listed
   with reasons under a may-only-shrink ratchet. Served from the `/admin` ops
   endpoint as `panels` and `panels_unexplained`.

   Two design decisions changed on contact with the code. §2.2's six reading
   states were **dropped**: `loop_model` already has eight stall reasons and
   `liveness_trust` five statuses, so a third overlapping set would have been a
   second answer to the same question. `Absence` therefore carries the source
   contract's own token verbatim and adds only the tri-state. And the resolver
   consumes `native_evaluators::Observation` rather than gathering its own
   snapshot, so the evaluators and the panels cannot disagree about which instant
   they describe.
3. ~~**Gate decision ledger**~~ — **done 2026-08-24**, `migrations/214_gate_decisions.sql`
   plus the recorder in `gate_trust`. See §2.5.
4. ~~**Panel contract** (§3)~~ — **done 2026-08-24**, `src/panel_contract.rs`.
   Not by generalising `hud_contract` — see §3.6 for why that was the wrong
   move — but by building the density ladder on top of it. Still outstanding:
   extending `glasses_shell_parity.rs` to renderer parity, which needs a second
   renderer to be parity *with*.
5. ~~**Command registry** (§3.5)~~ — **half done 2026-08-24**,
   `src/command_registry.rs`. The **declaration** landed; routing every surface
   through it did not, and the split is deliberate — see §3.7. A registry that
   lied about what governs a verb would be worse than none, so the declaration
   is fenced against the router before anything is asked to depend on it.
6. **Spine + queue surface.** Cheap, and un-buries the Observatory immediately.
7. ~~**Register with three lenses.**~~ — **done 2026-08-24**, `/bestiary` +
   `GET /api/bestiary`. One payload, three lenses (Discover · Population ·
   Health); the lens changes columns and sort, never the page. Replaces three
   separately-implemented lists of the same agents.

   Two things it fixed on contact with the data. The seven-rank `taxonomy` has
   been on the row since migration 186 and **had never reached the client** — it
   is now the card's type line, populated on 158 of 761 agents, with the rest
   grouped as `Incertae sedis` rather than hidden. And the live seam measurement
   corrected a stale census: 512 distinct port labels, **13 seam-forming**, so
   499 cannot compose with anything, and 3 of 10 contracts carry a real schema
   where the comment said none did. The Population lens states that as the
   headline, because §4.2 requires it to ship before any drag-and-snap composer.
8. ~~**Gates page**~~ — **done 2026-08-24**, `/gates`. Three blocks: the
   enforcement map (which verbs a gate can actually stop — the finding counting
   cannot produce), the register with `if_never_refuses` printed on every silent
   gate, and the receipts, which state that only two of seven gates can produce
   one so an empty list is not a record of no refusals.
9. ~~**Agent detail collapse**~~ — **done 2026-08-24**, `/specimen/:name`.
   8 tabs → Profile · Record · Health, plus a Configure drawer, because editing
   is a mode and not a tab. Composed from **one** endpoint: the thirteen
   duplicated metrics were caused by fetching the same quantity from a dozen
   producers under whatever name each chose, so one producer per number is the
   fix. Where a value cannot be measured it is **absent, not zero** — the old
   Performance Statistics rendered a measured zero and an unsourced field
   identically as `0.0%`. Health is the first UI for `resolve_for_subject`.
10. **`flow` and `fitting`.** The payoff, after the contract exists.

Steps 1–3 are substrate and produce no visible change. That ordering is
deliberate and should be defended when it is questioned in week two.

---

## 8. What this design cannot do

* **It cannot make an unturned loop turn.** It can only make the fact legible and
  attributable. Loops 2 and 4 will render as *never turned* for as long as they are,
  and the panel being correct is not the same as the system being well.
* **The `glance` tier is unvalidated.** `glasses/hud_field_scout/README.md` states
  plainly that nothing there has ever rendered — the runtime may not parse the page,
  `fetch` may not survive the Bluetooth proxy, 60 characters may not fit **[E]**.
  Design for `glance` because it disciplines the hierarchy; do not let an
  unvalidated runtime dictate the contract's shape.
* **It cannot verify studs, and it cannot fix the label set.** Until
  `output_contract` carries resolvable schemas, `fitting` reports asserted
  compatibility. And with 499 of 513 labels forming no seam **[E]**, a composition
  surface will look empty for reasons that are upstream of it. Rendering both
  distinctions is the most it can honestly do; converging the vocabulary is
  someone else's work item, and the UI's contribution is to make its cost visible.
* **It does not address Rabble's UX.** By design — Rabble decides its own.
* **The absence-resolution rule is only as good as the trust contracts.** A panel
  whose absence maps to a contract that does not probe that path will report
  `unmeasured`, which is honest and unhelpful. Expect a period where `unmeasured`
  is the most common state on the Gates page, and treat each instance as a work
  item rather than a rendering defect.
* **It cannot make an in-memory counter durable.** Per §2.9, the strongest stall
  diagnoses are lost on every deploy. Until `gate_decisions` lands, the UI's
  honest ceiling is *"nothing refused since boot"*, and a design that forgets to
  print `since boot` is worse than one that shows nothing.
