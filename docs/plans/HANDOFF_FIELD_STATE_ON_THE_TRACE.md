# Handoff — rendering `field_state::Observed` on the artifact trace

**Status:** `src/field_state.rs` is built, guarded and consumed by two surfaces.
`Observed` is built and **nothing renders it**. This is that task.

**Amended** after a second reading of the same trace, with the user's sign-off on
three decisions (§2.1). The plumbing in §4.1 and §4.2 is unchanged; §2.1, §4.2b,
§4.4 and §4.5 are new and are about **what the tokens should say**. Do §4.1
first regardless.

**Repo:** `/home/ilabra/fermi`, branch `main`, deployed to `agent-bestiary.world`
(the remote *is* the dev environment; deploying to iterate is normal).

---

## 1. The ambition this serves

The user's model, and it is the frame for everything below:

* **The contract shapes the pulse.** Prospective — declared before the run.
* **The gates verify the pulse against the contract.** Retrospective — after
  the model writes. Plus a distinct class of **preconditions** (`credit`,
  `rate_limit`, `attachment`) that gate the *run*, not the artifact.
  `gate_trust`'s `decides_before_the_artifact` is the platform's own name for
  that split.
* **Composition moves pulses between agents**, coordinated by the strategist.
  Ports say two agents *can* connect; contract states say whether what flows is
  worth consuming.

## 2. Why this task exists

A user opened one `football_analyst` trace and found four vocabularies over one
set of facts. Reconciled, they agree completely:

| | 13 fields, three partitions |
|---|---|
| contract (specimen) | 6 resolved · 4 pending · 1 derived · 1 inferred · 1 narrative |
| this run (trace top) | 1 from a tool · 1 from judgement · 11 at zero |
| the 11 | 5 tool asked & empty · 4 correctly absent · 1 owed · 1 derived |
| violations (trace) | 1 field carried a value the contract forbids → nulled |

Nothing contradicted. It read as though everything did, because:

* **`unsourced` meant two opposite things on adjacent panels** — a declared kind
  on the specimen ("a standing request, not a defect") and a violation on the
  trace ("1 unsourced claim was removed").
* **`refused` did not mean refused.** `Decision::Refused` is the ledger's word
  for *this gate acted*. Nothing was declined; one field was nulled and the
  answer was delivered whole. Asked *"what exactly was refused?"*, the page had
  no answer.

### 2.1 The second reading — three reds, four meanings

After `b3fa7e0a` the user read the same `football_analyst` trace again and
objected to what it *means*, not how it is built:

> "1 field empty ok but why is this a red flag?"
>
> "In practical terms this feels like a bunch of rejection."
>
> "Some of the things the agent was trying to do were incomplete, and 11 things
> couldn't be checked but were **omissions not hallucinations**. The error is
> **agent completeness vs aspiration**, not failure in the sense of producing
> untrusted or unsourced data. This kind of agent **compiles and should** — but
> is also not up to the standard one would want as the agent designer."

Verified against the code. The page renders four findings in one red, and they
are four different kinds of thing:

| where | keyed on | what it actually is |
|---|---|---|
| header badge `VIOLATIONS` | `violations > 0` — `artifact_trace::reading()`, `src/artifact_trace.rs:537` | **fault** — the model asserted what it could not know |
| Q3 `1 of 8 owed`, tone hardcoded `bad` | `e.empty > 0` — `templates/trace.html:926` | **shortfall** — commissioned work not delivered |
| Q4 `11 ◌` | `weakSourced > 0 ? "bad"` — `templates/trace.html:958` | mostly **capability gaps** and **compliance** |
| Q5 `records only` | `enforcement !== "control"` — `templates/trace.html:791` | a **ceiling**, not a finding at all |

Three conclusions, and they are the amendment.

**a. The badge is already right, and reads as wrong.** `reading()`'s first
branch returns `Reading::Fault` on `violations > 0` and nothing else. It is
reporting the single fabrication-shaped event on the page. But the badge says
`1` and Q3 says `1`, they are different `1`s, nothing distinguishes them, and Q4
puts `11` in the same colour family. *The one event that should alarm a reader
is the smallest number on the page and is drowned by two larger numbers that
mean "didn't do everything."*

**b. Rule 3 holds *within* completeness and not *across* the questions.**
`owed` / `no_data` / `excused` are carefully separated inside Q3's prose. Then
Q3's tone line reads `e.empty === 0 ? "ok" : "bad"` and paints a shortfall with
the fault colour anyway.

**c. The page answers one question and the reader has two.**

1. **Legality** — does it compile, is the artifact deliverable? On this pulse,
   **yes**.
2. **Ambition** — is the agent doing what its designer hoped? **No**, and that
   is not a fault.

They are interleaved in one strip with one colour ramp, so a designer-grade
disappointment reads as a caller-grade defect. That is the "bunch of rejection".

**The specimen page already solved this.** `templates/specimen.html:645`:

> Green means zero **errors**. The 4 pending field(s) are declared gaps with no
> source yet — each one a standing request for an integration, and **pruning
> them to reach green would delete the ambition the contract exists to record.**

Same sentence, same repo, one surface away. The trace lacks the frame, not the
facts.

#### Decisions taken, with the user, before any of this was coded

| # | question | decision |
|---|---|---|
| 1 | is "how bad" a separate axis from `whose`? | **Separate.** `Owed` and `Stripped` are both the agent's and only one is damaging; conflating attribution with severity is what produced the problem. → §4.2b |
| 2 | what replaces `records only`? | **Not a better adjective.** The word makes the platform sound impotent when the fact is "somebody left a thread dangling" — and the reason is already on the wire and discarded. → §4.4 |
| 3 | split legality from ambition? | **Split.** *"Ideally those things have no divergence but they are orthogonal and in conscious flux as agent versions change."* → §4.5 |
| 3b | drop the header badge once Row A carries the same finding? | **Keep and label** as Row A's summary, for now. Revisit once the split has been read in anger. |

## 3. What already exists — read this before writing anything

`src/field_state.rs`. One vocabulary, two clocks, **deliberately disjoint**:

```
Declared   resolved · error · pending · derived · inferred · narrative
Observed   filled · absent_by_contract · tool_empty · owed · stripped
```

* `Declared::of(&Grounding, |tool| dispatchable)` — the contract clock.
* `Observed::of(&GradedField, &Report, Option<&Assessment>)` — the run clock.
* `.token()`, `.why()` on both; `.whose()` on `Observed` (nobody's / the
  world's / the agent's).
* `FieldRow { path, declared, observed: Option<Observed> }` — `None` on a
  surface with no pulse in hand.

**`stripped` is the word `refused` was standing in for.**

Guards that will hold you to it:

| guard | what it stops |
|---|---|
| `the_two_vocabularies_share_no_token` | one word meaning a declared kind and a run state — the `unsourced` collision, made a build failure |
| `refused_is_not_a_field_state` | `refused` reaching a surface again |
| `no_surface_maps_grounding_to_a_state_itself` | a fourth inline copy of the five-way match |
| `every_token_is_lowercase_and_explained` | a state a reader cannot look up |

Consumers today: `handlers/specimen.rs` and `handlers/workspace/core.rs`, both
reading `Declared`. **Nothing reads `Observed`.**

## 4. The work

### 4.1 Fix the enforcement drift FIRST

**This is a real bug and it is mine.** The trace re-runs the contract over the
retained bytes at `src/handlers/loops.rs:770`:

```rust
let report = match enforced.as_mut() {
    Some(doc) => fermi::grounding_trust::enforce(&agent_name, doc),
    None => fermi::grounding_trust::Report::default(),
};
```

That is `enforce` — the `FIELD_CONTRACTS`-only path. The execute path was moved
to `enforce_from_output_contract` in commit `02041d5b` (§4.6 of
`WHAT_THE_PLATFORM_CAN_REFUSE.md`), so **the trace and the execute path now
enforce differently**. For the ten card-only agents — `simops_companion`,
`supply_chain_oracle`, `equity_analyst`, `species_resolver`, the four
`weather_*`, `macro_data_agent`, `moe_router_strategist`, `pipeline_strategist`
— the execute path strips and stamps and the trace shows nothing.

Deriving `Observed` from that report would be wrong for those ten, so fix it
first.

The fix is small: `output_contract` is **already selected** by the trace's query
(`loops.rs:745`) and already read at line 841. Move that read above line 770 and
swap in `enforce_from_output_contract(&agent_name, output_contract.as_ref(), doc)`.

Add a guard. `tests/execute_path_parity.rs` already owns "every route that
grades delivers what grading produced" and is the natural home for "every
surface that grades uses the same enforcement entry point."

### 4.2 Carry `Observed` to the page

Everything `Observed::of` needs is **already in scope** at the call site, which
is why this is a small change:

```
loops.rs:770  report        (after 4.1, the right one)
loops.rs:774  graded        Vec<GradedField>
loops.rs:827  completeness  fermi::completeness::assess(&graded, &tools_called)
loops.rs:829  artifact_trace::fields(&agent_name, &graded)   ← the seam
```

1. Add `observed: &'static str` (and probably `whose`) to
   `artifact_trace::Field` (`src/artifact_trace.rs:375`).
2. Widen `artifact_trace::fields()` to take `&Report` and `Option<&Assessment>`
   and call `Observed::of` per field. **Do not recompute anything** — that is
   the whole point of the module.
3. Render the token in `templates/trace.html`'s per-field rows, beside the
   existing contract grade, with `why()` in one legend keyed by the token.

`artifact_trace::fields` has few call sites; check them before widening.

### 4.2b `Finding` — the axis `whose` cannot carry

`Observed::whose()` answers **attribution** and buckets `Owed | Stripped` as
`"the agent's"`. Both are the agent's; only one is damaging. Attribution
therefore cannot carry the colour, and a second axis is needed:

```rust
pub enum Finding { Delivered, Compliance, CapabilityGap, Shortfall, Fault }

impl Observed {
    pub fn finding(self) -> Finding {
        match self {
            Self::Filled           => Finding::Delivered,
            Self::AbsentByContract => Finding::Compliance,
            Self::ToolEmpty        => Finding::CapabilityGap,
            Self::Owed             => Finding::Shortfall,
            Self::Stripped         => Finding::Fault,
        }
    }
}
```

`Finding::tone()` is then **the one producer of the colour** (house rule 8), and
the standing rule it encodes is:

> **Red is reserved for faults.** Shortfall is amber. Capability gap and
> compliance are neutral. Delivered is neutral.

Consequences to apply with it:

* `templates/trace.html:926` — Q3 stops hardcoding `bad` for `empty > 0` and
  takes its tone from the worst `Finding` among its fields.
* `templates/trace.html:957` — the pip distribution stops carrying a tone at
  all. `weakSourced > 0 ? "bad"` is a verdict smuggled into a description; the
  verdict moves to a row that can attribute it (§4.5, row 5).

Carry `finding()` on `artifact_trace::Field` alongside `observed`, in the same
widening as §4.2. Guard it the way the module guards everything else: every
`Observed` maps to exactly one `Finding`, and `Owed` and `Stripped` must not map
to the same one — mutate it and watch it go red.

### 4.3 Retire the prose

Once the token renders, the sentences it replaces should go, or there are two
answers again. In `templates/trace.html`:

* `"This artifact was repaired before it left: N unsourced claim(s) were
  removed"` (~line 674) — `stripped` says this per field.
* `"grounding graded N claim(s) and strips any the..."` (~line 702).
* Anywhere `refused` reaches a reader as a field-level verdict.

Keep the aggregate sentence if it earns its place, but it must use `stripped`,
not `unsourced`. Under §4.5 the surviving aggregate becomes Row A's caption.

### 4.4 The enforcement word — the reason is already on the wire

`records only` collapses **four** states into two. `command_registry` declares:

| `Enforcement` | what it does |
|---|---|
| `Control` | refuses the run |
| `Amend` | alters the artifact before it leaves |
| `Report` | verdict reaches the caller, unacted |
| `Metric` | verdict computed and **thrown away** |

`templates/trace.html` tests `b.enforcement === "control"` (lines 791, 831, 966)
and prints `records only` for the other three. So `Amend` — the mode that *does*
remove the bad part — reads as impotent, and `Report` (you were told) is
indistinguishable from `Metric` (discarded).

Worse: `command_registry::GateApplication::why_not_control` is **mandatory** for
every non-`Control` application. `a_gate_that_cannot_refuse_explains_itself`
panics without it, on the stated grounds that *"a gate demoted to a metric is a
decision somebody made, and the reason is what tells a later reader whether it
was deliberate or drift."* `artifact_trace::Rung` already carries it to the
client (`src/artifact_trace.rs:96`, filled at line 257).

**The page receives the reason and throws it away, then prints a word that
sounds like the platform shrugging.** There are two situations under that one
word and they are distinguishable in data today:

* *a limit that is real* — grounding on `/execute`: nothing can know a field is
  ungrounded until the model has written it. That is the honest ceiling for a
  check that happens afterwards.
* *a promotion nobody has done yet* — the dangling thread. The ratchet
  `the_discarded_gate_verdicts_are_the_ones_we_know_about` already says the set
  *"may only shrink. Promoting one to Control is the fix."*

Render consequence, not bookkeeping:

| | chip | full row |
|---|---|---|
| `Control` | `can stop it` | refuses the run |
| `Amend` | `removes the bad part` | (today's `strips and records` already reads well) |
| `Report` | `tells the caller` | + `why_not_control` |
| `Metric` | `counted, not acted` | + `why_not_control` |

Only `Metric` is a dangling thread and only `Metric` should carry a tone.

And: **the legend comes out of `<details class="expert">`** (`trace.html:1023`).
Today the token is unfolded and its gloss is folded, which honours house rule 2
in the letter and breaks it in effect. A token whose gloss is folded has no
gloss.

### 4.5 The strip splits — two standards, named

Five questions become two rows plus one description.

**Row A — "Fit to send"** · the caller's standard. Can be green today, and on
this pulse is.

1. allowed to run · `credit`, `rate_limit`
2. got its inputs · `attachment`, `input_binding`
3. **clean of unsourced claims** · `grounding` violations

Row 3 is what the header badge says. The badge **stays and gains a label**
naming it as Row A's summary (decision 3b); it is not dropped until the split
has been read in anger.

**Row B — "As designed"** · the designer's standard. Green here is an
aspiration, and divergence is information rather than failure.

4. did the work · `completeness` owed
5. sources reachable · fields naming a tool that was never called or cannot run
   (this is where `weakSourced` goes, per §4.2b)
6. checks that can act · route enforcement, with `why_not_control` (§4.4)

**Where the numbers came from** · the pip distribution, full width, **no tone**.
It is a description; it should not render a verdict.

Row B takes the computed caption — the specimen's sentence, ported and derived
from the `Finding` counts rather than written:

> **Deliverable, below standard.** No unsourced claim left this answer. What is
> missing is missing: 1 value the agent owed, 5 the world could not supply, 4
> the contract requires be empty. Pruning the contract to reach green would
> delete the ambition it exists to record.

The distance between the two rows is the designer's worklist. This is the same
idea as replacing the create-page wizard (§10) with a live verdict strip:
**legality is a gate, ambition is a gradient**, and the platform currently only
has vocabulary for the first.

**Sequencing.** §4.5 lands *after* §4.2 and §4.2b, not with them. It needs
`Finding` to exist before it can colour anything, and it is the only part of
this handoff that restructures a panel rather than adding to one.

## 5. The nuance that must not be lost

**The trace re-runs the contract; it does not read stored violations.** The
comment at `loops.rs:761` says so and it is deliberate — `response_text` has been
retained since migration 199 precisely so a historical episode can be re-graded.

So `stripped` on a trace means *"the contract as it stands today would remove
this"*, not *"it was removed when this ran"*. For a pulse older than a contract
change those differ. Say so on the page or in `Observed::Stripped`'s `why()` —
do not let a reader assume a historical fact.

## 6. Verification protocol — non-negotiable, all of these bit

1. **`cargo` in the main working tree tells you nothing about `main`.** Another
   session works in the same clone and its tree is frequently mid-flight and
   uncompilable. Always:
   ```
   git worktree add /tmp/fv-x --detach HEAD
   cp <only your files> /tmp/fv-x/...
   cp .env /tmp/fv-x/.env
   touch /tmp/fv-x/src/lib.rs /tmp/fv-x/agent-bestiary/memory/src/*.rs
   CARGO_TARGET_DIR=/home/ilabra/fermi/target/vX cargo test --lib --manifest-path /tmp/fv-x/Cargo.toml
   ```
   The `touch` is mandatory — a stale `memory` artifact produces errors that
   exist in neither tree.

2. **Two worktrees must not share a `CARGO_TARGET_DIR`.** They will serve each
   other's test binaries. A pristine worktree once reported 8 tests in a file
   containing 7, silently, and every comparison made that way was invalid. This
   is the worst trap in the repo because it produces plausible results.

3. **Put target dirs under `target/`** (gitignored, 1.4T free). `/tmp` is a 23G
   tmpfs and two target dirs fill it.

4. **Never `git add` a shared file wholesale.** If a file has another author's
   hunks, use index surgery:
   ```
   git show HEAD:<file> > /tmp/f.rs   # apply ONLY your hunks
   sha=$(git hash-object -w /tmp/f.rs)
   git update-index --cacheinfo 100644,$sha,<path>
   ```

5. **`--no-verify` is required.** The pre-commit hook compiles the working tree,
   which is often theirs and broken. Run every check by hand instead.

6. **Commit messages: `write_file` + `git commit -F`.** Never `printf`.

7. **Rust string literals need the literal `—`, not `\u2014`.** `edit_file` will
   happily write the escape and the build fails. Same for JS.

8. **Backticks inside an HTML comment inside a JS template literal end the
   literal.** `tests/inline_js_syntax.rs::every_widget_script_parses` now covers
   `static/js/*` as well as templates. This bug has been written four times,
   most recently by me.

9. **Assert your mutation applied.** A `str.replace` that silently matches
   nothing is indistinguishable from a guard that does not fire, and only one is
   a problem. Always `assert pattern in source` in the mutation script.

10. **Every guard must be mutated and watched go red.** A check never seen to
    fail has not been shown to work. Both directions where the property is
    two-sided.

## 7. Parallel session

Same clone, same `.git`, very active. As of this handoff it is plumbing
`registry` into `ExecutionContext` for prompt-time fleet-digest injection —
roughly 20 files including `llm_executor.rs`, `api_server.rs`, `executor.rs`,
`handlers/a2a.rs`, `handlers/xaman.rs`.

`src/handlers/loops.rs`, `src/artifact_trace.rs` and `templates/trace.html`
were **not** theirs at handoff time. Check `git status` before starting.

**Known red on `main`, all theirs, do not try to fix:**

* `test_all_migrations_registered` — migration 232 unregistered
  ([issue #36](https://github.com/Replicant-Partners/fermi/issues/36))
* `no_curated_card_declares_a_phantom_tool` — `regulatory_lens_translator`
  declares `list_workspace_files` with no dispatch arm
* `the_field_contract_roster_does_not_shrink` — ratchet 10 → 11
* `typed_tier_exemptions_only_shrink` — ratchet 77 → 78, records a *loosening*

## 8. House rules that shaped all of this

1. A cell holds a value or a token, never a sentence.
2. **Explain once** — the reason belongs to the *state*, in one legend keyed by
   the token the rows print.
3. **Absent must look different from bad** — and, per §2.1, this must hold
   *across* panels and not only within one. Red is reserved for faults;
   a shortfall is amber and a capability gap is neutral.
4. `value · condition · act`, positionally fixed.
5. If the platform can name what would close a gap, **the name is the control**.
6. Green = zero errors, not zero pending.
7. **A check never seen to fail has not been shown to work.**
8. **One producer of a verdict; everything else reads it.** The trace strip once
   re-derived in JavaScript the gate it was drawing. Four surfaces each deciding
   a field's state is the same defect spread wider — and is why
   `field_state` exists.

## 9. Recent commits, for context

| sha | what |
|---|---|
| `b3fa7e0a` | `field_state` — one field, two clocks; the vocabulary the surfaces disagreed in |
| `72495f06` | the verification ladder removed from the artifact page |
| `f9c5e7c9` | the Team tab shows what each member can be trusted about |

Full argument: `docs/plans/WHAT_THE_PLATFORM_CAN_REFUSE.md` (read §4 first) and
`docs/architecture/META_AGENT_FLEET_AWARENESS.md`.

## 10. Also open, not this task

* **The wizard.** `templates/agent_create.html` has `wizard-panel`,
  `step-indicator`, four steps — against "no wizards, the agent should compile".
  **Directly related to §4.5:** the legality/ambition split is the same idea,
  and the create page needs it more than the trace does.
  The compiler exists: `run_publish_checks` returns named checks with severity
  and **is what `Gate::Admission` refuses on**. The create page calls neither it
  nor `validate_agent_card`. Replacing the steps with a live verdict strip is
  mostly wiring.
* **Prompt-time fleet-digest injection** — in flight in the parallel session.
* **`error` carries two senses on the specimen page** — a field state and a
  severity for non-field rows. Pre-existing; worth fixing when that page's row
  grammar is next touched.
