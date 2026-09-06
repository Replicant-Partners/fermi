# What the platform can refuse

**Written 2026-09-04, from the question "what's missing so the platform *could*
reject — otherwise this is just decoration."**

It was decoration on the route that matters most, and the codebase had already
written down why, in the one place nobody reads: the `why_not_control` string on
`Gate::Grounding`.

**Companion:** `docs/plans/PROMPT_CONTRACT_AND_THE_SHELF.md` §7 — the same
disease one layer up, where the *displays* confused three subjects. This
document is about the *mechanism* underneath them.

---

## 1. The finding

`Pulse::grade` produces two documents: `claimed`, and `enforced` with every
ungrounded field nulled. On `POST /api/agents/:id/execute`, `enforced` was used
for **exactly one thing** — validating the declared schema at
`execution.rs:473`. The body returned to the caller was the raw model text,
fabrication included.

The gate registry said so, as the recorded reason grounding is a `Metric` there:

> *"`enforce` mutates a document the handler keeps only to check a schema
> against; the persisted response_text and the rendered body are both
> un-stripped. … it means **the endpoint a third party calls reports fabrication
> rather than preventing it**."*

Meanwhile `envelope::build` on the delegation hop **always** returned the
enforced payload, and `grounding_is_enforced_at_the_hop` proves it.

**Agent→agent was protected. Person→API was not. The artifact trace drew the
same belt for both.**

Ledger at the time of writing:

```
grounding | approved      |  9
grounding | refused       |  7
grounding | undetermined  | 26     ← agents with no contract at all
gate_decision_reviews     |  0     ← nothing has ever checked a verdict
```

## 2. Why "could anything have stopped it?" is the wrong question

Grounding can never *prevent* a fabrication. A field's grounding is unknowable
until the model has written it, so there is no moment at which refusing the
**effect** is available — the run has happened and the credits are spent by the
time there is anything to judge.

Asking a post-hoc gate whether it could refuse guarantees a red cell for ever,
which is exactly what question five did on every artifact this platform has
produced.

The reachable ceiling is stopping the bad part from **travelling**.

| verb | when it can act | who has it |
|---|---|---|
| **prevent** — refuse to run | before | `credit`, `rate_limit`, `attachment` ✅ |
| **prevent** — refuse the input | before | `input_binding` — declared Metric, *could* be Control |
| **amend** — strip and deliver the rest | after | delegation hop ✅ · execute ✅ · stream ✅ |
| **report** — verdict to the caller | after | `completeness` ✅ |
| **refuse** — deliver nothing | after | nobody, and see §4 |

Every route that grades now delivers what grading produced. `AMENDS_LATER` is
empty and may only stay so.

**The discarded-verdict ratchet: 3 → 2 → 1.** The one that remains is
§4.4, and it is not moving soon.

## 3. What changed

### `Enforcement::Amend` — the rung the vocabulary was missing

`command_registry` had `Control` (refuses) and `Metric` (records). Amendment is
neither: a metric changes nothing a caller sees, and a control refuses the verb.
Calling amendment `Metric` for the life of the feature is what let the execute
route return fabricated values while the trace drew a checkpoint over it.

`Enforcement::alters_the_artifact()` is the new question — the one `refuses()`
could not answer, and the one that actually separates a control from a
decoration. `gates_computed_and_discarded()` is keyed on it now, because listing
an amend as *discarded* would report a working control as a dead one.

### The execute body carries the enforced document

`envelope::amend_document(text, enforced)` replaces the document span **and
nothing else**. Three channels, deliberately not treated alike:

| channel | what it carries | why |
|---|---|---|
| `document` *(new)* | the enforced artifact | first-class, so a machine never scrapes JSON out of prose to get a trustworthy answer |
| `metadata.reasoning` | prose, document span replaced | the model's sentences are not ours to rewrite — a leaking `summary` is nulled *inside* the document by `NARRATIVE_LEAKS`, not edited here |
| `grounding.stripped` *(new)* | the paths removed | a caller who cannot see an amendment cannot tell a clean document from a repaired one, and has no reason to look at what went |
| `episodes.response_text` | **raw, on purpose** | the only evidence of what the model claimed. Amending the record would destroy the finding while appearing to fix it. |

`extract_json` and `amend_document` share one scan (`document_span`), because
two scans of the same text agree on almost every input — which is the kind of
drift that surfaces once, in production, on the one response shaped differently
from the fixtures.

### The trace says so

Question five gained `strips and records`, and it is **not red**. The verdict
line for a repaired artifact now reads *"This artifact was repaired before it
left: N unsourced claim(s) were removed and the rest was delivered"* instead of
*"Nothing stopped this artifact, and it carries unsourced claims."* The second
sentence was false: something did stop the claims, and it was this gate.

## 4. What is still missing, in order

### 4.1 A refusal contract for callers — **the parts existed; the conclusion did not**

If the platform refuses, the caller receives **what**? Without an answer to
that, refusal converts a degraded answer into no answer, callers route around
it, and a control people route around gets switched off. This has to exist
before anything is promoted, not after.

This section used to say *"the delegation hop has an envelope — `payload_status`,
`violations`, the partial payload. HTTP has none."* That is no longer true, and
the way it stopped being true is the point. The execute response now carries
the enforced `document`, `grounding.stripped`, `completeness.owed`, and
`validation.status` with its violations. There is plenty there.

What there was not is **one thing to branch on**. A caller asking the only
question a caller has — *can I use this answer?* — had to read four sub-objects
in four vocabularies:

```
status              Success | Failed                     did the RUN finish
validation.status   valid | invalid | unverified_*        does it match its type
grounding.amended   bool, plus stripped[]                 did we remove fabrication
completeness.owed   [{path, why}]                         did the agent skip work
```

…and then implement the platform's own judgement about how they combine.

That is the defect this repository keeps finding in its own surfaces. The trace
strip re-derived in JavaScript the gate it was drawing; the page computed
question three from the values because no checkpoint computed it. Both were
fixed by having **one** producer of the verdict and everything else read it. A
caller recombining four fields is the same shape one process further out, where
we cannot see them get it wrong — and cannot fix it when we change the
precedence.

#### `src/reliance.rs`

One derived token, worst-first, declared in the order it is evaluated so the
vocabulary and the precedence cannot drift:

| token | means |
|---|---|
| `unusable` | no structured document at all — prose, or nothing |
| `malformed` | a document that contradicts the type the agent declared |
| `amended` | the platform removed values no tool could have supplied |
| `incomplete` | the agent did not fill fields it was asked for |
| `unchecked` | a document, and **no contract was applied** — not a pass |
| `clean` | applied, nothing stripped, nothing owed, matches its type |

Derived from the verdicts the route already computed, never recomputed, so it
cannot disagree with the blocks it summarises. Both HTTP routes call the same
function and carry `reliance.status` plus `reliance.why` — the sentence said
once, keyed by the token, rather than glossed by each consumer.

Three of these rankings are arguable and so they are argued in the module:

- **`amended` outranks `incomplete`** because an absent field is a gap while an
  invented one is evidence about the model, and a caller may reasonably trust
  the rest of *that* document less.
- **`malformed` outranks `amended`** because a caller that planned its parse
  around `produces_schema` is already wrong, which is worse than a missing value.
- **`unverified_*` does not lower reliance at all.** Most cards declare no
  schema; a token that read "nothing was checked" as a fault would report
  almost the whole corpus as malformed and be switched off within a day. Absent
  must look different from bad.

#### There is deliberately no `refused`

A refusal envelope with no refusal behind it would be a claim that the platform
can refuse, which is false. Inventing the shape now is precisely how the three
stale claims in this subsystem got written — the same defect wearing optimism
instead of age.

`the_vocabulary_has_no_refusal_it_cannot_emit` holds the line, and it is keyed
to the ladder rather than to prose: it reads `command_registry` for the three
gates `reliance` summarises (`grounding`, `completeness`, `output_schema`) and
fails the moment one of them starts refusing. `credit`, `rate_limit` and
`attachment` are excluded because they are already `Control` and always will be
— they refuse before there is an answer, so the caller gets a status code and no
body to carry a token.

So the promotion sequence is now: add `refused` to `RELIANCE`, delete that
assertion, and change the enforcement rung — in one commit. Callers are already
branching on the field before the value appears in it, which is the whole of
what this section was asking for.

### 4.2 A measured false-positive rate — **the door was not the missing piece**

**7 refusals, 0 reviews.** You cannot justify promoting anything to `Control` on
a control nobody has ever checked. The number that unlocks promotion is
*overturned / decided*, per gate.

The obvious reading was that the review path had not been built. It had. All of
it:

| piece | where | state |
|---|---|---|
| table | migration 216 | exists |
| SQL, tally, standing, refusal classes | `src/gate_review.rs` | exists |
| three doors | `gate_api::GATE_DOORS` | declared |
| route + handler | `api_server.rs:3030`, `handlers::loops::review_gate_decision_handler` | wired |

So the zero is not neglect. It is arithmetic, and the ledger says why:

```
gate       | decision     |  n | non-null subject | distinct reasons
-----------+--------------+----+------------------+-----------------
grounding  | approved     |  9 |                0 | 0  (null)
grounding  | refused      |  7 |                0 | 1  "1 violation(s)"
grounding  | undetermined | 26 |                0 | 0  (null)
```

Two facts, both fatal to review:

1. **Every row is anonymous.** `subject` is null on all 42. The ledger cannot
   say which agent any decision was about. `gate_api::LEDGER_SQL` selects
   `subject` and has always been selecting a null column.
2. **Every refusal is a count.** One distinct reason across the whole table:
   `1 violation(s)`. Not which field was taken, not what class of fault, not
   what was claimed.

A reviewer is asked *"was this refusal right?"*. Against an anonymous count the
only honest verdict is `unclear` — so `Standing::Inconclusive` was the **only
reachable state**, and migration 216 predicted this exactly: *"the ledger does
not record enough to review its own decisions."* It was right, and it was
written before the writer existed to be checked against.

And it is not one gate. `gate_decisions` contains **only grounding
rows**. `coherence` and `admission` are both `Recorded`, both have a declared
door, and neither has ever written a row — so two of the three doors lead to a
guaranteed 404. The door count was never the measure of the review path.

Worse: both were writing the **same anonymous row**. `decided(Gate::Coherence,
…)` with `agent` loaded twelve lines above it; `decided(Gate::Admission, …)`
with `agent.agent_name` in scope. Their reasons were fine — admission's is
`failing: typed_interface, …`, which is genuinely judgeable — and both would
have produced a row no reviewer could attach to an agent. The defect was
invisible because the paths are **cold**: an agent-wide intervention is a rare
operator action, and the curated corpus is loaded from disk rather than
published through the pipeline.

> A cold defect is still a defect. A `Counted` gate may be anonymous for ever,
> because it never becomes a row anybody opens. A `Recorded` gate exists in
> order to be reviewed later, and a row with no subject cannot be.

All three now record through a subject-carrying entry point, and
`gate_trust_coverage::every_recorded_gate_names_what_it_decided_about` keeps it
that way — it reads which `decided_*` entry points take a `subject` out of
`gate_trust.rs`'s own signatures, so a fifth entry point is understood without
an edit. `agent_name` and never `agent_id`, so the column holds one kind of
thing: a ledger whose `subject` is sometimes a slug and sometimes a UUID cannot
be grouped, sorted or read.

That scan had to read the file as text rather than line by line, because the
shape that hid `admission` spans lines — entry point on one, `Gate::Admission`
on the next. The sibling check in the same file reported admission as
*recorded*, and it was: recorded anonymously.

> A gate that records **that** it refused, but not **what** it refused, cannot
> be reviewed — and a control that cannot be reviewed cannot be promoted. The
> review door was a door onto a room with no light in it.

#### What changed

- **`ViolationKind::id()`** — the doc comment on `Violation::kind` said
  *"machine-stable reason, for anomaly payloads"* and there was no `Display`,
  no `Serialize`, no `id`. The promise was kept nowhere, so the one consumer
  that needed to name a violation wrote a count instead.
- **`Report::refusal_reason()`** — names the fault and the paths, grouped by
  kind: `ungrounded_field genome.chromosome_count, taxonomy.divergence_mya;
  narrative_leak summary`. Tokens and paths only (rule 1); what the token
  *means* lives once on `ViolationKind` (rule 2). Grouping is load-bearing —
  `reason` truncates at 400 chars, and an ungrouped list would silently drop
  the last paths on a document that fails a dozen fields.
- **`subject` is now required** on `gate_trust::decided_for_episode`. Not
  widened to `Option<&str>` — a `&str`. Both call sites had the slug in scope
  and were passing `None` past it, and a decision that reached the artifact
  stage always knows what it was about. An invariant the compiler holds needs
  no test to notice when it stops being true.
- **`Graded::agent`** — read by `assess_completeness` off the grade rather than
  taken as a second argument, so the two `gate_decisions` rows one pulse writes
  cannot be filed under different agents.

`Violation::removed` — the value the model actually wrote — is deliberately
**not** in the reason. It is the most useful thing for judging a refusal and it
does not belong in a 400-char text column shared by every gate; it is also the
only field here that carries arbitrary model output into a durable log. The
reviewer's own table has `evidence JSONB` for it, and the artifact trace already
shows the claim beside the contract. Naming the path is what lets a reviewer
*reach* the value; carrying the value would be the ledger trying to be the
artifact.

#### Still missing, and now unblocked

This makes a decision **judgeable**. It does not make one **judged**. What
remains before *overturned / decided* is a number is **rows written under the
new writer**. The 42 existing rows are retrospectively unjudgeable and stay that
way — `subject` and `reason` are not backfillable, because what was stripped was
never recorded. The denominator starts from the next pulse, and the honest
reading of the old rows is `unclear`.

**The queue was already built, and finding that out was the point of looking.**
The obvious next task read *"nothing lists the refusals worth opening"*. It
does: `nav → /gates → /gate/:gate_id` renders every decision for a gate with
`review: null` marking the unjudged, a standing summary, and a review form
(`templates/gate.html`, `handlers::loops`). The whole path is reachable from the
navigation bar.

So building a cross-gate queue would have been a **new surface over seven
unjudgeable rows** — this document's own disease, committed while writing the
document about it. The blocker on §4.2 was never the absence of a queue; it was
that the rows in it could not be judged. That is fixed, and the next honest
move is §4.6, which produces decisions worth reviewing rather than another
place to look at the ones that are not.

Note the shape of this, because it is the same shape as §4.4: the work that
looked like *build the review door* turned out to be *make the ledger worth
opening*. Both were found by measuring instead of building.

### 4.3 ~~A gate for question three~~ — **done**

`Gate::Completeness`, in `src/completeness.rs`, filed by
`Pulse::assess_completeness`. Question three names a gate now and the `no gate`
caveat is gone, because it stopped being true.

The distinction it makes is the one the row grammar already made and the strip
did not:

| the field | whose | verdict |
|---|---|---|
| `unsourced`, or `derived` | nobody's — the contract requires null | excused |
| `sourced`, tool asked and had nothing | the **world's** | excused, counted |
| `sourced`, tool never called | the **agent's** | owed |
| `inferred`/`narrative`, empty | the **agent's** — commissioned work | owed |

**It needs the run record, and that is why it could not live in `enforce`.**
Grounding is pure over the document; completeness turns entirely on whether the
tool was *called*, which lives on `AgentOutput::tool_invocations`. The same four
null genome fields are a compliant run or a negligent one depending on it, and
no inspection of the document can tell them apart.

**What it deliberately does not judge:** *the tool answered with substance and
the field is still empty.* Telling that from an honest miss needs a judgement
about whether the answer was inside that response — which the trace makes with a
byte count and says only re-running settles. A gate must not accuse on a
judgement, so `owed` counts only the unambiguous: a named tool never called, and
commissioned work absent. **The count is a floor, not a total**, and the
response says so.

It is `Retention::Counted` and `Enforcement::Report`. Counted because §4.2 still
holds — nothing has earned a per-decision ledger while the review count is zero.
Promoting it to `Recorded` needs a migration widening
`gate_decisions_gate_check` **in the same commit**, or every decision becomes
unwritable in a batch insert whose error is swallowed by design.

#### The two things that commit left behind

Filling the hole did not close everything that pointed at it, and both leftovers
are the same disease this document is about — a claim that outlived the code it
described.

**A test asserting the absence of a gate that exists.**
`trace_verification_fold::the_question_with_no_gate_still_has_no_gate` pinned
question three's empty gate list, and its own failure message said *"if one is
built, delete this assertion in the same commit that builds it."* One was built
and the assertion was not deleted, so the suite went red and **stayed** red
through two commits — including one whose message claimed 61 green integration
suites. It is now
`the_question_with_no_gate_is_found_rather_than_named`, and it pins the
invariant that survives: question three must name `completeness` and must **not**
be given `grounding`, which is the tidying that would restore the original
defect.

**A false sentence nobody could see.** `templates/trace.html` hardcoded
`<b>Question 3 has no gate.</b>` under the condition
`rows.some(r => Array.isArray(r.g) && !r.g.length)`. When question three got a
gate the condition went false, so the sentence stopped rendering — and stayed in
the page, ready to blame question three for whichever question next lost its
gate. The note now **derives** its subject
(`rows.findIndex(...)`) and names it by number and label, with general prose,
because what makes it worth saying is not anything about completeness: it is
that one answer on the page has no checkpoint behind it.

A conditional claim nobody can see is false is the display-layer form of a gate
whose verdict nobody reads.

While fixing the second, the sibling guard turned out to be measuring nothing:
`the_summary_is_a_strip_and_the_expert_views_carry_their_headlines` searched a
**6,000-character window** from the start of `questions(d)`, and every token it
looks for sits between 8,600 and 9,600 characters in. It had been green on a
window that no longer reached the thing it checked, and only failed when
completeness's prose pushed the strip further out. It is bounded by the next
function declaration now — a magic number was measuring the length of the prose
above the strip and calling it the presence of the strip.

#### `Enforcement::Report` — the fourth kind

Completeness can neither refuse nor amend: there is nothing to strip from a
field the agent left empty, and refusing would deny the caller fourteen good
fields because of one missing one. The remedy is the agent.

But its verdict **is returned to the caller**, in `response.completeness`, so it
is not a `Metric` either — `Metric` means the verdict is discarded, and *"on the
surface a caller sees, one of those is indistinguishable from having no gate at
all."*

So `gates_computed_and_discarded` is keyed on `reaches_the_caller()` rather than
`alters_the_artifact()`. Declaring completeness a `Metric` would have grown that
list from two to three and reported a brand-new visible check as a regression.

### 4.4 `input_binding` — measured, and **blocked**

This was the cheapest promotion on the platform: refuse a malformed input before
a credit is spent, protecting the payer rather than the reader. Its
`why_not_control` said the mismatch **rate** was *"the number that would justify
making it fatal."* Nobody had computed it.

Computed:

```
published agents                     110
  declared a text input               54   49.1%
  no accepts at all                    9    8.2%
  accepts, none textual               47   42.7%   <- promotion would REFUSE these
```

**It has since gone to 48**, and the ratchet is what made that visible.
`regulatory_lens_translator` was authored in the same pattern — `accepts`
listing the slots its prompt needs told, nothing textual — and the guard failed
on the commit that added it. Nothing is wrong with that card, and that is
precisely the finding: this is what agents on this platform naturally do, the
pattern is still spreading, and **every new one makes the gate less promotable
rather than more**. The rise is recorded in the guard's own comment, naming the
card, because a ratchet that absorbs rises silently is the thing it exists to
prevent.

And the refused list is working agents:

```
prey_locator      94 pulses
enemy_sensor      62 pulses    accepts: creature_id, species_data, location_context
naturalist        47 pulses    accepts: creature_name, scientific_name, species_group
species_resolver  15 pulses
forage_scout      15 pulses
```

**`bind_input` never sees the query.** It is pure over the agent's own
`accepts`, and asks only whether some declared label *looks like free text*. So
`NoTextInput` does not mean a caller sent the wrong thing — it means this agent
lists **the semantic slots its prompt needs told**, not a transport shape. None
of those agents refuses prose; every one is invoked with a query.

So the blocker is not a threshold. **`accepts` is doing two jobs** — what an
agent can be *handed*, and what its prompt needs to *know* — and while that is
true a mismatch count is not evidence about callers at all. Resolving it is the
ports rung's question (`docs/plans/PORTS_RUNG_EDITOR.md`), not this one's.

Held by `port_trust::promoting_input_binding_to_a_control_would_refuse_half_the_corpus`,
which pins the count as a ratchet that may fall freely and not rise — so the
promotion cannot be argued for again without meeting the number.

**This is what step 2 of the plan was for**, and it is the outcome that
justifies having done it: the cheapest-looking work on the list turned out to be
the most blocked, and the measurement cost one read-only query.

### 4.5 The ceiling

Grounding can only refuse what it can **name**. It says *"no tool could have
supplied this."* It cannot say *"this is wrong."* `ContradictsCanonical` exists
and only three agents carry a `cross_check_sql`.

Refusal on **correctness** needs the assertion queue to receive verdicts, which
is Loop 2, which is starved because `anomaly_events` is its only input and most
episodes raise nothing. That is the real ceiling on all of this and it is a much
larger piece of work.

#### The meta agents invert it

Everything above silently assumes **the canonical record is elsewhere** — GBIF,
NCBI, a weather API — so contradicting a claim means an external round trip, a
queue, and Loop 2. For `xaman_ek`, `fermi` and `simops_companion` it is not.
Their subject matter is *this platform*, and the truth is one local `SELECT`.

So the agents easiest to dismiss as "prose, nothing to ground" are the ones
where `ContradictsCanonical` — the only kind that says **wrong** rather than
**unsourceable** — is cheapest. Four agents carry a `cross_check_sql` today and
all four reach for external truth. The agents whose subject we own carry none.

#### The incident

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

Three claims, all false, all one query from being checked. A user asking about
their own cost and quality tradeoff was told the opposite of the truth.

**And it could not have answered correctly.** Its two sources of fleet knowledge
were a one-line prose digest per agent in its own system prompt, which carries
no model information at all, and `list_agents`, which returned
`{id, type, description, skills}` and no model information either. The question
was unanswerable from every source it can reach, so confabulating something
specific and plausible was the only way to answer at all.

> A meta agent's grounding problem is not that it fabricates. It is that the
> platform hands it **prose about its fleet instead of structured access to
> it**, and then fabrication is the only way to answer.

#### Encyclopedic where the design says ecological

`docs/architecture/AKP-ecology-design-doc` §7 is explicit: *"The meta-agent does
not attempt to know everything every agent knows. Its awareness is ecological —
it understands the shape, dynamics, and health of the agent ecosystem."* And
`AGENT_MODEL.md` §3.3 already diagnoses the symptom: *"The current
unsatisfying-ness of xamanEK is a symptom of it carrying weight that should be
distributed to surface-resident agents."*

The mechanism by which it carries that weight is a **string**. Every one of 102
agents is pasted into its system prompt, kept in sync by
`test_all_agents_registered_with_xaman_ek`, which asserts each `agent_id`
appears as `**agent_id**`. That test institutionalises the encyclopedic model:
every agent added makes the prompt longer, the digest lossier, and the test
redder. It is red right now, for `regulatory_lens_translator`.

It is enforcing the invariant the design moved past — the same shape as every
other stale claim in this document, except that this one is a *test*, which is
why nobody reads it as a claim at all.

#### What changed here, and what deliberately did not

`list_agents` now returns what a navigator is actually asked about: `model`,
`provider`, `min_tier`, `model_ladder` as tier → model, `accepts`, `produces`,
`produces_schema`, and `tools`. The projection is a pure function,
`platform::fleet_entry`, so it can be checked against a real card without a
registry.

`the_fleet_listing_answers_what_the_navigator_got_wrong` asserts
**recoverability, not strings**: from what the tool returns, a caller must be
able to establish that a ladder exists and that the free rung differs from the
card default. Pinning `openrouter/free` would go red every time a model is
swapped — routine — while saying nothing about whether the question is
answerable. Mutated twice: back to the original four fields, and with the ladder
present but always empty (the plausible half-fix). Both red.

**No contract for `xaman_ek` yet, on purpose.** Marking these claims `Sourced`
against a tool that could not supply them would declare a check the platform
cannot run — which is the exact failure this rung spent its whole length
removing, from `Derived` fields with no caller to `grade` discarding the
contract it was handed. The source has to exist before the contract can name it.

#### The kind these claims want

Not `Sourced`: that asserts only that *a tool could answer*, and the whole
`Antaxius beieri` lesson is how weak a claim that is when the model can call the
tool and answer from memory anyway. Platform-state claims are **`Derived`** —
computed by us or checked by us, never merely asserted — which is already
enforced by `every_derived_field_is_computed_or_checked`, and which the parallel
work adopted the same week for `regulatory_lens_translator`'s
handler-computed fields.

The scoping machinery for the staleness half also exists: `COHORT_PREDICATE`
restricts a cross-check to episodes produced by the prompt the agent
*currently* has, by comparing `card_prompt_hash` against
`sha256(system_prompt)`. For an agent whose fleet knowledge **is** its prompt,
that is exactly the right scope, and it was built for something else.

#### Composability splits in two, and only one half was ever hard

The obvious move for a navigator is to compute whether one agent can feed
another. It is wrong, and the measurement says so more sharply than the
argument: **every hand-off that has actually happened in production has zero
declared label overlap.**

```
weather_oracle -> weather_ensemble_forecaster   produces fermi/weather_market_call
                                                accepts  forecast-question
                                                overlap  NONE          (4 hops)
weather_oracle -> weather_calibrator                     NONE          (1 hop)
simops_companion -> supply_chain_oracle                  NONE          (1 hop)
```

A computed compatibility check would have a **100% false-negative rate** on the
topology that exists. That is the third appearance of one finding: a check over
a vocabulary that has not converged refuses working behaviour. §4.4 measured it
at 48 of 102 cards for `input_binding`; here it is 3 of 3 compositions.

But the vocabulary **has** converged — into a shape where equality is the wrong
relation. Across the twelve fully typed agents:

| side | form | examples |
|---|---|---|
| `produces` | **the artifact I make** — 100% namespaced | `fermi/weather_market_call`, `rabble/species_card` |
| `accepts` | **the question I answer** | `abw/genome-query/1`, `scro/bom-query/1` |
| `accepts` | …or slots the prompt needs told | `ticker`, `company`, `species-name` |

Those are different kinds of thing and will never string-match, by design.
`weather_oracle` does not pipe a market call into the forecaster; it **asks the
forecaster a question** and receives an artifact. The cards encode
**request/response**, and looking for a pipe finds nothing — the nothing means
the query was wrong.

So:

- **Substitutability** — who else answers the same ask — is computable from
  cards **today**. `src/port_trust.rs::answerers`.
- **Chainability** — whether X's artifact answers Y's question — is not in the
  labels and cannot be derived from them. It lives in
  `episodes.parent_episode_id`: **observed** rather than declared, which is the
  right shape for a fleet that reconfigures itself. Computing it as a `Control`
  is the a-priori-determinism trap; as a `Report` it costs adaptation nothing
  and says something true — that the manifest has drifted from the behaviour.

#### `Substitutes`, and why it is three states

A label everybody answers to is not a seam, it is the calling convention:

```
query              24 of 102   Universal   — excludes nothing
workspace-state     8 of 102   Cohort      — all coordination/coherence agents
location_context    6 of 102   Cohort      — all spatial/creature agents
abw/genome-query/1  1 of 102   Bespoke     — genome_profiler's alone
```

`UNIVERSAL_SHARE` is a tenth, and the argument is not the number: if more than
one agent in ten answers to a label, the label describes how the platform is
**called** rather than what any of them is **for**. Pinned from both sides —
loosened to 0.50 and `query` becomes a cohort; tightened to 0.05 and
`workspace-state` stops being one. Both go red.

#### The counter-intuitive result

**Typing made `accepts` labels less shared, not more.** A namespaced request
type is per-agent by construction, so of twelve namespaced accepts labels in the
corpus exactly **one** is shared. The cohorts live in the organic untyped
vocabulary agents converged on without being told to.

`fermi/forecast-question/1` is the exception and the pattern worth copying: a
namespaced question type deliberately shared, answered by five agents. It is the
only place the typed vocabulary expresses a cohort, and it does it well.

A note on how this was nearly got wrong: partitioning by `TYPED_TIER_EXEMPT`
said the post-contract cohort was no better than the legacy one. It was the
wrong partition — 16 of those 29 agents are off the exemption list and never
typed their ports at all. The signal only appeared against the twelve
typed-only agents, and those are exactly the ones that compose.

#### The observed topology is readable now

`port_trust::OBSERVED_SEAMS_SQL` — caller, callee, hops, last seen — plus
`seam_agreement`, which compares one observed hand-off against what the two
cards declare. The const was executed against production, not just written:

```
caller            callee                       hops  last_seen
weather_oracle    weather_ensemble_forecaster     4   2026-08-28
simops_companion  supply_chain_oracle             1   2026-08-19
weather_oracle    weather_calibrator              1   2026-08-18
```

`every_observed_seam_is_undeclared` pins all three, and **that test going red
would be good news**: it asserts a defect, so the direction matters. A pair
becoming `Declared` means the port vocabulary has started describing real
hand-offs. The failure to worry about is a pair *disappearing*, which means the
seam stopped being exercised. `the_seam_comparison_can_see_a_match` stops the
whole thing being satisfied by a function that always returns `Undeclared` — the
vacuity that let the trace's 6,000-character window pass for months.

#### `xaman_ek` cannot be contracted yet, and the reason is not a missing contract

It emits **prose**. Of 18 recorded responses, 4 contain even a brace, and
`extract_json` finds no document in the rest — so `claimed` is `None`, the report
is empty, and a field contract over it would enforce nothing.

That is the same failure this rung has removed four times already: a `Derived`
field with no production caller, `grade` discarding the contract it was handed,
`simops_companion`'s inert `narrative` block, and `output_schema` recorded with
no door. Writing a contract for `xaman_ek` today would be the fifth, authored
deliberately.

**The blocker is an agent change, not a platform one:** the navigator has to
emit a document before its claims can be typed. That is a product decision about
a conversational agent — `AGENT_MODEL.md` §3.3 already argues its weight should
be distributed to surface-resident agents — and it is not one to make from
inside the grounding subsystem.

What *is* now true is that every source such a contract would name exists:
demographics and artifacts from the widened `list_agents`, substitutability from
`answerers`, chainability from `OBSERVED_SEAMS_SQL`. The order was deliberate.

#### `fermi` is the meta agent that can be contracted today

Measured over agents with recorded responses:

```
agents with responses      42
  emit document-like text  28   (290 pulses)
  prose only               14   (125 pulses)
```

And among document-emitting agents that are still uncovered, ranked by traffic:

```
fermi                    49 pulses, 34 document-like   <- the meta agent to start with
cohere_and_coordinate    10
energy_advisor            5
regulatory_scanner        4  (4 of 4 document-like)
… 19 agents, 135 pulses total
```

So the meta-agent contract starts with **`fermi`**, not `xaman_ek`: it produces
FPL programs and forecasts, two thirds of its retained responses are
document-shaped, and its claims are about the platform's own model of a question
— checkable in exactly the way §4.5's inversion describes.

#### And the coverage backlog is much smaller than "81 agents"

§4.6 counted 81 uncovered cards. The number that matters is **19 agents and 135
pulses** — those that actually emit documents, actually run, and have no
contract. The rest are either prose-only, where grounding has nothing to operate
on, or cold.

This also corrects a stale comment: `prose_has_no_floor_above_ungrounded` says
*"74 of 100 curated agents return prose only"*. Against recorded responses it is
14 of 42. The comment may be counting cards without typed fields rather than
observed output, but as written it overstates the ceiling by a wide margin — the
fourth stale claim this rung has turned up, and the least consequential.

#### Correction owed on §4.6

Coverage went 11 → 21 agents on the execute path, and **20 of those are real.**
`simops_companion` — 113 pulses, third busiest on the platform — has a contract
of exactly one block, `action`, marked `narrative`. On the card path `narrative`
gets no provenance stamp and the `NARRATIVE_LEAKS` scan is a TODO, so nothing
is checked. Nominal coverage.

Its `action` block is also the wrong kind: a structured dispatch instruction is
not prose, and `regulatory_lens_translator` — the same dispatcher shape — marks
its handler-computed output `Derived`.

### 4.6 Coverage — **the contracts existed; the execute path threw them away**

26 of 42 grounding decisions were `undetermined` — which reads as *"agents with
no contract at all"*, and that reading was wrong for ten of them.

`Pulse::grade` took an `output_contract` argument and discarded it:

```rust
let _ = output_contract;
let report = grounding_trust::enforce(agent_slug, doc);
```

Under a comment saying `enforce_from_output_contract` *"is in flight on another
working tree and is not on `main`"*. It had been on `main` for some time, and
`envelope::build` — the delegation hop — had been calling it all along.

**Third instance of the same disease in this subsystem.** The stream's
`AMENDS_LATER` exemption and the trace's "Question 3 has no gate" sentence were
the other two. All three were true when written; all three were load-bearing
after they stopped being true.

And the cost was the familiar asymmetry, one layer up from §1: a compiled
contract was enforced when another **agent** called the agent and ignored when a
**person** did. `grounding_execute_coverage`'s own opening paragraph describes
exactly this defect for the `FIELD_CONTRACTS` agents and fixes it there — ten
more were in the same state through the card path, including two of the busiest
on the platform.

#### Where coverage actually stood

| home | agents | enforced on execute, before |
|---|---|---|
| `FIELD_CONTRACTS` (per-field, Rust) | 11 | yes |
| compiled card `output_contract.grounding` (per-block) | 10 more | **no** |
| nothing | 81 | n/a |

So it was 11 of 102, not 27, and the gap was a discarded argument rather than
unwritten contracts.

#### Blast radius, measured before wiring

Across the ten card-only agents, 46 blocks:

```
sourced      24     stamp only
inferred     14     stamp only
narrative     7     no stamp at all
unavailable   1     <- the only status that nulls anything
```

The one is `species_resolver.conservation`. Nine of the ten cannot lose a value
to this change.

#### It also fixes schema validity, which was the surprise

These compiled contracts declare their `_provenance` siblings **required**, with
an `enum` drawn from the platform's own vocabulary:

```
required:           [items, items_provenance, risks, risks_provenance, summary, summary_provenance]
items_provenance:   enum [tool_verified, tool_no_match, unavailable_no_tool_source]
```

Only the stamper can legitimately write those — the platform's verdict
overwrites whatever the model emitted. So while the seam was open the document
was **necessarily invalid against its own declared schema**, three required
properties absent on every pulse, and the execute path reported exactly that to
nobody in particular. 39 required keys across nine agents were in that state.

That makes this the rare coverage change with no trade: enforcement engages
**and** schema validity improves, because the missing keys were the platform's
to supply. `additionalProperties: false` on all ten made this worth checking
before wiring rather than after — the stamps are only safe because the compiler
emits the `_provenance` properties into the schema alongside them.

#### `has_contract` moved with it, and had to

The comment on it said: *"So it tracks `enforce`. When the compiled path lands
here, this reads both, and the two lines move together."* They now do.

The lag was deliberate and correct: reading the card while `enforce` could not
apply it would have recorded `approved` for a check that never ran — a false
approval, indistinguishable from a real one, which is worse than the
three-state problem the block exists to solve. The hazard is gone in the only
way that made it safe to close, which is that the check now actually runs.

#### One contradiction this exposes, and does not fix

`species_resolver.conservation` is declared `unavailable` in the grounding map
and `{"const": "unavailable"}` in the schema — the *grounding status* leaked
into the schema as the block's required **value**. Enforcement nulls the block;
the schema then demands the literal string. The two checks cannot both pass.

It is one card of 102, it is committed on `main`, and it is already happening at
the delegation hop. Schema validation on execute **reports** rather than
refuses (there is no `Gate::OutputSchema` on `agent.execute`), so the effect is
a truthful `validation_status: "invalid"` naming the contradiction rather than a
broken response. Left as found: the fix belongs to the sketch compiler, and
special-casing it here would be a silent exemption.

#### What is still uncovered

81 agents have no contract in either home, and that is now the honest number.
The per-field trace grain does not extend to the card path either —
`graded_fields` reads `FIELD_CONTRACTS` only, so those ten agents get
enforcement and a gate verdict but no per-field rows on the artifact trace. That
is a real gap and a genuine design question (the card map is per-**block**; the
trace is built around fields), deliberately not answered here.

## 5. The guards

| property | held by |
|---|---|
| a route that grades also returns what grading produced | `execute_path_parity::every_route_that_grades_also_returns_what_grading_produced` |
| the stripped value is not in the body, and the prose is untouched | `envelope::tests::the_ungrounded_value_does_not_travel` |
| the amended span is the span that was read | `envelope::tests::the_amended_span_is_the_span_that_was_read` |
| a clean document is not reported as amended | `envelope::tests::a_clean_document_is_left_exactly_alone` |
| grounding amends on both execute routes, and neither claims to be a Control | `command_registry::tests::grounding_amends_on_both_execute_routes` |
| the discarded-verdict list may only shrink | `command_registry::tests::the_discarded_gate_verdicts_are_the_ones_we_know_about` — **3 → 2 → 1** |
| question five has a word for repair | `scripts/check_trace_probe_render.js` |
| a tool asked and empty is nobody's fault | `completeness::tests::a_tool_that_was_asked_and_had_nothing_is_nobodys_fault` |
| the same document, tool never called, is the agent's | `completeness::tests::a_tool_that_was_never_called_is_the_agents_gap` |
| `0` and `false` are answers; `[]` and `"  "` are not | `completeness::tests::zero_and_false_are_answers_and_empty_containers_are_not` |
| a contract of only excused fields is undetermined, not a pass | `completeness::tests::a_required_absence_is_excused_and_so_is_a_derived_value` |
| question three names a gate and the caveat is gone | `scripts/check_trace_probe_render.js` |
| the belt is 8 rungs, and a change must say which | `artifact_trace::tests::the_absence_token_comes_from_the_gate_registry` |
| `GATE_IDS` is derived from `GATES` | `gate_trust::tests::gate_ids_match_the_declared_gates` |
| a refusal reason names the fault and the field, and never counts them | `grounding_trust::tests::a_refusal_reason_names_the_fault_and_the_field` |
| no two violation kinds share a ledger token | `grounding_trust::tests::every_violation_kind_has_its_own_token` |
| a dirty report always says something to the ledger | `falsification_registry` — the `grounding_trust::refusal_reason` pair |
| an episode decision cannot be recorded anonymously | the compiler — `decided_for_episode` takes `subject: &str` |
| what reaches the queue names its agent and its fault | `gate_trust::tests::an_episode_decision_reaches_the_queue_naming_its_agent_and_its_fault` |
| every `Recorded` gate records what it decided **about** | `gate_trust_coverage::every_recorded_gate_names_what_it_decided_about` |
| a compiled card contract is enforced on execute, and its stamps satisfy the card's own schema | `grounding_execute_coverage::a_compiled_card_contract_is_enforced_and_satisfies_its_own_schema` |
| both HTTP execute routes report the same reliance, from the same function | `execute_path_parity::both_http_execute_routes_report_the_same_reliance` |
| the reliance vocabulary promises no refusal the platform cannot make | `reliance::tests::the_vocabulary_has_no_refusal_it_cannot_emit` |
| when several readings are true, the caller is told the worst | `reliance::tests::the_worst_available_reading_wins` |
| nothing checked is not the same as checked and wrong | `reliance::tests::nothing_checked_is_not_the_same_as_checked_and_wrong` |
| the fleet listing can answer what the navigator got wrong | `platform::tests::the_fleet_listing_answers_what_the_navigator_got_wrong` |
| a universal label is not a cohort, and a domain label is | `port_trust::tests::query_is_not_a_cohort_and_workspace_state_is` |
| one answerer is `Bespoke`, not missing | `port_trust::tests::a_label_with_one_answerer_is_bespoke_not_missing` |
| the one shared question type keeps working | `port_trust::tests::the_shared_question_type_is_the_pattern_that_works` |
| `port_trust` and `completeness` decisions are registered at all | `falsification_registry::every_decision_function_is_registered_or_exempted` |
| that scan reads multi-line calls and fires on a real one | `gate_trust_coverage::the_pairing_sees_a_multiline_call_and_an_anonymous_recorded_write` |
| question three names `completeness` and never `grounding` | `trace_verification_fold::the_question_with_no_gate_is_found_rather_than_named` |
| the `no gate` note finds its subject instead of asserting one | same test |

Every one was mutated against the pre-fix source and watched go red.

`AMENDS_LATER` is **empty**, and the guard asserts it stays so: every route that
grades delivers what grading produced. It held one entry — the streaming route,
exempted because a stream has already sent its tokens by the time the document
is gradeable — for exactly one commit. That was true of the `progress` deltas
and never true of the route, which emits a terminal `complete` frame after
grading.

### The guard whose absence was the defect

`an_episode_decision_reaches_the_queue_naming_its_agent_and_its_fault` goes
through `Pulse::grade` and then reads the `PendingDecision` that arrives. Nothing
ever did that, and that is the whole story of the 42 rows: the queue, the flush
and the table were all correct, the writer simply never passed what it knew, and
no test looked at what turned up. A unit test on `refusal_reason` alone still
passes if `grade` throws the value away.

Mutated both ways, against the exact production states:

| mutation | result |
|---|---|
| `subject` → `""` | red — *"reached the ledger without naming the agent"* |
| reason → `format!("{} violation(s)", ...)` | red — *`"2 violation(s)"`* |

### The one guard that is not a test

`decided_for_episode`'s required `subject` is held by the type system rather
than by an assertion, and that is deliberate. Every other row in the table above
can be defeated by deleting the test. This one cannot be defeated without
changing a signature and every call site at once — and the defect it prevents,
42 anonymous ledger rows, is precisely the kind that no test noticed for the
life of the table because nothing ever read the column.
