# Handoff — rendering `field_state::Observed` on the artifact trace

**Status:** `src/field_state.rs` is built, guarded and consumed by two surfaces.
`Observed` is built and **nothing renders it**. This is that task.

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

### 4.3 Retire the prose

Once the token renders, the sentences it replaces should go, or there are two
answers again. In `templates/trace.html`:

* `"This artifact was repaired before it left: N unsourced claim(s) were
  removed"` (~line 674) — `stripped` says this per field.
* `"grounding graded N claim(s) and strips any the..."` (~line 702).
* Anywhere `refused` reaches a reader as a field-level verdict.

Keep the aggregate sentence if it earns its place, but it must use `stripped`,
not `unsourced`.

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
3. **Absent must look different from bad.**
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
  The compiler exists: `run_publish_checks` returns named checks with severity
  and **is what `Gate::Admission` refuses on**. The create page calls neither it
  nor `validate_agent_card`. Replacing the steps with a live verdict strip is
  mostly wiring.
* **Prompt-time fleet-digest injection** — in flight in the parallel session.
* **`error` carries two senses on the specimen page** — a field state and a
  severity for non-field rows. Pre-existing; worth fixing when that page's row
  grammar is next touched.
