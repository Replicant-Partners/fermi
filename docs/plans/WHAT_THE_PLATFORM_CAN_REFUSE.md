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
| **amend** — strip and deliver the rest | after | delegation hop ✅ · execute ✅ *(this change)* · stream ✗ |
| **refuse** — deliver nothing | after | nobody, and see §4 |

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

### 4.1 A refusal contract for callers — *before* any refusal

If the platform refuses, the caller receives **what**? The delegation hop has an
envelope — `payload_status`, `violations`, the partial payload. HTTP has none.

Without it, refusal converts a degraded answer into no answer, callers route
around it, and a control people route around gets switched off. This has to
exist before anything is promoted, not after.

### 4.2 A measured false-positive rate

**7 refusals, 0 reviews.** You cannot justify promoting anything to `Control` on
a control nobody has ever checked. `gate_decision_reviews` exists (migration
216) and is empty. The number that unlocks promotion is *overturned / decided*,
per gate.

### 4.3 A gate for question three

Nothing asks *"did the agent fill the fields it was asked for."* Question three
is computed on the trace, from the values, precisely because no checkpoint
computes it — and it is the only cell on that page with no gate behind it.

It is computable at the boundary today: `graded.fields` carries `produced` and
`kind`, and the tool-call record is in scope. `Gate::Completeness`, `Metric`
first. The distinction it must make is the one the row grammar already makes and
the strip does not:

* the tool was asked and had nothing → **the world's gap**, nobody's fault
* the tool was never asked, or answered and the value was dropped → **the
  agent's**

### 4.4 `input_binding` is free prevention, unused

Declared a Metric because *"`is_mismatch()` guards a warning and control flow is
identical either way."* It is the one place genuine **prevention** is available:
a malformed input can be refused before a single credit is spent. Cheaper than
everything above, and it protects the payer rather than the reader.

### 4.5 The ceiling

Grounding can only refuse what it can **name**. It says *"no tool could have
supplied this."* It cannot say *"this is wrong."* `ContradictsCanonical` exists
and only three agents carry a `cross_check_sql`.

Refusal on **correctness** needs the assertion queue to receive verdicts, which
is Loop 2, which is starved because `anomaly_events` is its only input and most
episodes raise nothing. That is the real ceiling on all of this and it is a much
larger piece of work.

### 4.6 Coverage

26 of 42 grounding decisions are `undetermined` — agents with no contract at
all. Enforcement on 15 typed agents while 100+ run ungoverned is a control with
a very small blast radius. Coverage is a prerequisite for enforcement, not a
consequence of it.

## 5. The guards

| property | held by |
|---|---|
| a route that grades also returns what grading produced | `execute_path_parity::every_route_that_grades_also_returns_what_grading_produced` |
| the stripped value is not in the body, and the prose is untouched | `envelope::tests::the_ungrounded_value_does_not_travel` |
| the amended span is the span that was read | `envelope::tests::the_amended_span_is_the_span_that_was_read` |
| a clean document is not reported as amended | `envelope::tests::a_clean_document_is_left_exactly_alone` |
| grounding amends on execute and does not claim to be a Control | `command_registry::tests::grounding_amends_on_execute_and_still_does_not_on_the_stream` |
| the discarded-verdict list may only shrink | `command_registry::tests::the_discarded_gate_verdicts_are_the_ones_we_know_about` — **3 → 2** |
| question five has a word for repair | `scripts/check_trace_probe_render.js` |

Every one was mutated against the pre-fix source and watched go red.

The exemption list is `AMENDS_LATER`, and it holds one entry: the streaming
route, which has already sent its tokens by the time the document is gradeable.
Amending it needs a terminal frame carrying the enforced document and a consumer
that prefers it over the concatenated deltas. When that lands, the entry comes
off and the guard starts holding it too.
