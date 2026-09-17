# Coordination — DPP Studio: the three-panel restructure, the passport, and the white-label seam

**For:** the session doing parallel work on `carbon_accountant`, and whoever
takes the UI restructure.
**From:** the session that fixed the flush timeout (`9dd78615`) and diagnosed
the carbon-output visibility bug.
**As of:** `9dd78615`, live and verified serving (`/api/health` →
`commit: 9dd786152e83`, 09:16:50).

This is a design contract, not a task list. The operator's feedback was
specific about layout, and the reason the layout matters is that it decides
whether adding the eighth agent costs the same as adding the third. Everything
below is checked against the code; where it is not, it says so.

---

## 0. What is live, so nobody re-diagnoses it

| thing | state |
|---|---|
| flush timeout fix | live at `9dd78615`. `FLUSH_TIMEOUT_SECS = 240`, flush error captured into `failure_reason` + `tracing::warn!` |
| `carbon_accountant` baseline | 1 execution, 1 failure, 18 searches, `avg_loop_iterations 5.0`, 151,790 tokens, 157,246 ms |
| the original failure | `tool loop produced empty content (stop_reason=tool_use, iterations=5, hit_iteration_cap)` — eighteen *successful* searches discarded because the write-up turn died at the 90s per-hop client timeout |
| re-run since the fix | **not yet done.** Metrics unchanged from baseline. Nobody has seen the fix work |

### 0.1 Two findings about retrieval that change what "working" looks like

Tested directly against the open web, not inferred:

**Cane sugar is retrievable, three times over.** 0.36 kg CO2e/kg (Soil & More
Impacts, Mauritius, 2019); 5.71 kg CO2e/kg (*Sustainability*,
`10.3390/su122410380`, Nigeria irrigated + reservoir, 2020); a ProTerra/Blonk
study against Agri-footprint (Dominican Republic, 2022). Each carries all four
of value, dataset, geography and reference year.

Those first two are **16× apart** and both are correct for what they describe —
reservoir methane dominates the Nigerian figure. Two publishers more than 30%
apart is `diverging` → `needs_expert`. So a *working* carbon run on this BOM
produces a divergence flag and a request for a human, not a clean number. If
someone reads that as the agent still being broken, they will "fix" the thing
that is working.

**Dried hibiscus calyces are not retrievable.** The literature covers colorant
extraction, freeze-dried powder for wastewater coagulation, and solar drying
kinetics. There is no cradle-to-gate food-ingredient factor in kg CO2e/kg.
Hibiscus should come back `null`, and that is the correct answer.

### 0.2 The reference-flow hole — CLOSED in `ac39c763`

**Status: the multiplication is now refused rather than corrected.**
`inventory.items[].reference_flow` is `Sourced`, `factor_unit` is enforced
(it was already in the output shape and nothing ever read it), and
`carbon_basis_refusal` in `grounding_trust.rs` makes any factor that is not
mass-per-mass leave its line **unpriced** — a state the document already
expresses — with `basis_refusal` recording which of the two refusals applied.
An absent `factor_unit` is refused, not presumed to be per kg. `t CO2e/t` and
`g CO2e/g` are accepted because the ratio is the same number. Three tests, each
verified to go red with the check defeated; 1115 lib + 234 bin green.

**What is still open, and it is narrower:** a factor correctly labelled
`kg CO2e/kg` whose denominator is a different *substance* than the BOM line —
extract against calyces at the same unit. That needs a material match, which is
the same check `inventory.items[].geography` is already waiting on (see its
exemption), and should land with it as a mismatch report beside
`needs_expert` rather than as a refusal.

The original analysis is kept below because it is the argument for why the
field had to exist at all.

### 0.2.1 The original analysis

The top hibiscus result reports **"5 kg CO2-eq to obtain 1 g of colorant
extract"** — 5,000 kg CO2e/kg. It would satisfy every guard currently in place:
it has a value, a dataset (*Energy Reports*, 2022), a geography (Portugal) and
a reference year. Real paper, real URL, four fields present.

And it is wrong for this BOM by ~10⁵, because the reference flow is one gram of
*extract*, not one kilogram of *calyces*.

The comparability key — material, geography, reference_year, dataset — **does
not include the functional unit or reference flow.** Today's failure mode is an
honest `null`; this one is a confidently wrong, fully-provenanced number, which
is the direction nobody audits. Proposed: add `reference_flow` (e.g.
`1 kg dried calyces`) and `functional_unit_basis` as required fields in
`FIELD_CONTRACTS` for `inventory.items[]`, include `reference_flow` in the
`carbon_emission_factors` comparison key (mig-238 is append-only, so this is an
`ALTER TABLE ADD COLUMN` plus a new key tuple, not a rewrite), and make a
mismatched reference flow a gate violation rather than a note. **Not started.
Do not assume it is done.**

---

## 1. The visibility bug — diagnosed, and much smaller than it looks

> *"I can't get to the carbon calculation that was done."*

**It is a client-side omission. Every endpoint it needs already exists.**

`static/adaptogen-lab/index.html:542` renders the string
`dpp/carbon/statement.yaml` as a **label**. Nothing fetches it. The carbon
output is rendered exclusively from the in-memory `POST
/actions/calculate_carbon` response (`index.html:2279`), so a reload discards
it permanently.

Meanwhile the read path is already built and already used:

| route | status | used by the UI today |
|---|---|---|
| `GET /api/workspaces/:id/files` (list) | exists, `api_server.rs:4366` | no |
| `GET /api/workspaces/:id/files/*path` | exists, `api_server.rs:4369` | **yes** — claims (`1139`), composition (`1377`) |
| `GET /api/workspaces/:id/files-raw/*path` | exists, `api_server.rs:4377` | no |
| `GET /api/workspaces/:id/actions` | exists, `api_server.rs:4040` | yes — Activity seeding (`3114`) |

So `loadBomYaml()` reads `dpp/composition.yaml` through exactly the mechanism
the carbon panel needs and does not use.

**What survives a reload today, and what does not.** `activityLoad()` rebuilds a
one-line summary per run from `apply_result` — duration, coverage,
`total_kg_co2e`, `factors_recorded`, arithmetic disagreements (`3146`–`3157`).
So the run's *existence* and *headline numbers* persist. But it sets
`detail: null` (`3169`), and nothing re-renders the per-line table, the
citations or the prose. The record persists; the document does not.

Two smaller defects found in the same read:

- `activityLoadPulse()` fetches `/api/agents/${EVALUATOR}/metrics`
  (`index.html:3078`) and never `ACCOUNTANT`. The carbon agent's pulse — the
  numbers that diagnosed the timeout — is invisible in the UI that ran it.
- The `KB` timeline (`3180`) is a **hardcoded array** with `s:'pend'` literals.
  It reads as live build state and is not.

### 1.1 The fix, and why the obvious version is wrong

The obvious fix is to have the carbon panel call
`GET /files/dpp/carbon/statement.yaml` on load. That is necessary but not
sufficient, because **a statement file is one document and the operator asked
for history** — and `statement.yaml` is overwritten per run. One file cannot
answer "what did the run on Tuesday say".

The durable identifier already exists: `action_id`. `calculate_carbon` writes
`apply_result` against it, `GET /actions` returns the series, and the statement
YAML already embeds `action_id` (`statement_yaml(&doc, &product_id, action_id,
&audit)` in `carbon.rs`). So history is reconstructible without a new table:

1. Centre panel lists runs from `GET /actions`, filtered by `action_type`, newest first.
2. Selecting a run renders from its `apply_result`.
3. The *current* run additionally hydrates the full document from `GET /files/dpp/carbon/statement.yaml`.
4. For superseded runs, either the statement is written to a per-action path (`dpp/carbon/statements/{action_id}.yaml`) or `apply_result` carries enough to re-render. **Decision needed — see §7.**

Do not invent a new persistence layer for this. Workspace git plus the action
log is the platform's answer and both are already wired.

---

## 2. The panel contract

The restructure the operator asked for, stated as a contract so it can be
checked rather than admired:

```
┌─────────────────┬──────────────────────────────┬──────────────────┐
│ LEFT: data      │ CENTRE: agent output         │ RIGHT: operator  │
│                 │                              │                  │
│ products        │ lens: regulatory             │ activity feed    │
│ claims          │ lens: supply chain           │ spend / pulse    │
│ BOM             │ lens: carbon                 │ strategist       │
│ ── the DPP ──   │ lens: cold chain / shelf     │  (companion)     │
│ passport view   │ ─────────────────────────    │                  │
│ barcode         │ execution history per lens   │                  │
└─────────────────┴──────────────────────────────┴──────────────────┘
     nouns                    verbs                    operator
  what the product is    what agents concluded      what it cost me
```

**The invariant that makes agents cheap to add:** *a new agent adds one centre
lens and touches neither the left nor the right panel.* Left is the product's
own record; right is the operator's own record; neither is a function of how
many agents exist. Today the panels are not separated this way, which is why
carbon needed bespoke UI rather than registration.

**Therefore the lens registry must be declarative.** Something close to:

```js
const LENSES = [
  { id:'regulatory', label:'Regulatory', agent:'regulatory_lens_translator',
    action:'evaluate_claims',  doc:'regulatory-lens/evaluations/',  spends:true },
  { id:'carbon',     label:'Carbon',     agent:'carbon_accountant',
    action:'calculate_carbon', doc:'dpp/carbon/statement.yaml',     spends:true },
  …
];
```

Each entry must give: the action to POST, the document path to read back, the
agent id for `/metrics`, and whether it spends credits. `SPENDS_CREDITS`
(`index.html:2693`) is currently a parallel array of action names — it should be
derived from the registry, not maintained beside it. `parsing.cost_classes` in
the companion manifest is a contract the client already honours; the registry
must agree with it rather than restate it.

**Naming, because it will otherwise drift:** the operator said "cold chain /
shelf life". That is a *fourth lens with no agent yet*. Register it as
`available: false` rather than omitting it, so the panel can show what the
architecture expects and does not yet have — the same honesty the carbon panel
applies to a missing factor.

---

## 3. The passport, and the barcode

> *"the DPP should be/feel like a passport … generate a barcode for each product that is scannable."*

A passport's property is not decoration, it is that **a stranger can verify it
without trusting the bearer.** That is the design constraint, and it has one
hard consequence:

**A scannable code must not resolve to an unverified claim without saying so.**
The carbon statement carries `verification_status: unverified` and the agent is
explicitly not an accredited verifier. A QR code that resolves to a clean-looking
passport page launders that. Whatever the resolver renders must carry the
verification status and the `needs_expert` flag *above* the numbers, not in a
footer. This is the same two-block provenance seam already in force elsewhere:
the verdict must never inherit its citation's strength.

Practical notes, flagged as **unverified — needs research before building**:

- The EU DPP regulation (ESPR) specifies a *data carrier* linked to a unique
  product identifier. GS1 Digital Link is the likely encoding, expressing a
  GTIN as a resolvable URL. **Nobody on this codebase has confirmed the current
  ESPR data-carrier requirements. Do not encode a guess as a standard.**
- Products here have a `part_number` (`PKH-F2-330`) and an `item_id`. Neither is
  a GTIN. A real GTIN requires a GS1 prefix the operator must own. Until then,
  encode the resolver URL for the workspace product and label it clearly as an
  internal identifier, not a GS1 code.
- Generate client-side (no new dependency, no image egress) and render as SVG so
  it stays crisp under the UI-scale control (`uiscale.rs` mirror, `4ecc7ae7`).
- The resolver must be readable **unauthenticated** to be a passport at all,
  which makes it the first public read surface in this app. That is an
  authorisation decision, not a UI one — see §5.

---

## 4. Label ingest — photo or barcode → claims

> *"we should be able to upload a label to populate claims from a picture or barcode scan."*

**This is an agent job with a provenance seam, not an upload widget.** The
temptation is to OCR the image and write the strings into `claims.yaml`. That
would be wrong for a reason this codebase has already paid for once:

- The **text on the label** is `Sourced` — it came from the artefact, and the
  artefact should be retained and addressable so the extraction is falsifiable.
- The **claim boundary** — deciding that "Low in sugar" is one regulated
  nutrition claim while "supports digestive balance as part of a healthy
  lifestyle" is a health claim plus a qualifier — is `Inferred`. It is a model
  judgement about regulatory categorisation.

Collapsing those is exactly the `tool_no_match`-as-clearance failure the design
refuses everywhere else. So: ingested claims must land in a **proposed** state
carrying the image reference and the extraction's own confidence, and require
an explicit accept before any lens evaluates them. `POST
/actions/:action_id/accept` and `/reject` already exist (`api_server.rs:4124`,
`4128`) — this is that pattern, not a new one.

A barcode scan is a different operation and should not share a code path: it is
an *identifier lookup*, which either resolves to a known product or does not.
It populates nothing by itself.

---

## 5. The white-label seam — what exists and what does not

> *"expose each as an API or MCP endpoint that a third party can white-label."*

Partly built. Be precise about which half.

**Already isomorphic.** `api_server.rs:4038` states the intent: the action
protocol is "isomorphic across companion action blocks, abw CLI, and MCP
tools/call". Actions are uniform HTTP: `POST
/api/workspaces/:id/actions/{action_type}`, logged to `workspace_action_log`,
listable via `GET /actions`, with `accept`/`reject`. A third party integrating
over HTTP has a coherent surface today.

**Not built: the DPP actions are not MCP tools.** `handlers/mcp.rs` exposes
workspace-scoped tools — `read_workspace_file`, `list_workspace_agents`
(`mcp.rs:29`, `253`) — and resolves `workspace_id` from arguments or context.
`calculate_carbon`, `evaluate_claims` and `price_bom` are **not** among them.
So "expose each as an MCP endpoint" is real work: register each action as an MCP
tool whose input schema is the action's request body.

Two things to get right, neither cosmetic:

1. **Cost disclosure at the tool boundary.** These actions spend the workspace
   wallet. The browser client confirms before a `spends_credits` action using
   `parsing.cost_classes`. An MCP caller has no such affordance, so the tool
   description must declare the charge and the tool must refuse rather than
   silently spend. A third party discovering a 6-credit, 160-second tool by
   invoking it is not an acceptable first experience.
2. **`simops_companion` declares six action names as tools that do not exist in
   the registry** (known debt). Do not repeat that shape here: register the
   tool or do not declare it.

---

## 6. Shared-file protocol — the actual hazard

`static/adaptogen-lab/index.html` is the contended file. It has been corrupted
twice by shell-escaped `'\n'.join` inside heredocs writing literal `\n`, and
another session's uncommitted work has twice nearly been committed.

Rules, learned the expensive way:

- **Large edits go via a real temp file, then `os.replace`.** Never
  `open(path,'w')` before the new content exists — that truncates on the way to
  throwing, which destroyed an uncommitted `rabble_workspace.rs`.
- **Before committing a shared file, grep the staged diff for the other
  session's markers.** `git add <file>` stages *their* hunks too.
- **To commit only your hunks:** split `git diff <file>` by `@@` header, drop
  theirs, `git apply` yours onto a clean `HEAD` worktree, verify it compiles
  there, then `git hash-object -w` + `git update-index --cacheinfo`. This is how
  `9dd78615` was landed while another session held edits at lines ~118 and ~1514
  of `tool_executor.rs`. It works; it takes about ten minutes including the
  compile.
- **Verify the deployed artefact, not the dashboard.** `/api/health` returns
  `commit`. Railway silently served an 8-commit-stale image for ~90 minutes;
  the tell was a `last-modified` header matching a specific commit's timestamp
  to the second. A `DEPLOY_TRIGGER.md` commit is the repo's convention for
  nudging it.

---

## 7. Decisions — answered by the operator 2026-09-17

1. **Statement history storage. DECIDED: per-action path,** with
   `statement.yaml` kept as a pointer to the current one.

   **This is now the one blocking server-side task, and it belongs to whoever
   owns `carbon.rs`.** The client half shipped in `28b00836` and is honest
   about the gap: history lists every run from `GET /actions`, and only the
   newest run that produced a statement can open the committed document,
   because `statement.yaml` is overwritten per run.

   The implementation is not a one-liner, which is why it was not bundled:
   the response `json!({…})` in `calculate_carbon_handler` is assembled
   **after** `git.commit_files_as`, so the artefact the panel wants does not
   exist at commit time. Building the payload into a `let` above the commit and
   pushing it onto `files` as `dpp/carbon/statements/{action_id}.json` — the
   exact POST body, which `carbonPanelHTML()` already renders with no parsing —
   makes every historical run re-openable at full fidelity and costs the client
   one `JSON.parse`. **Do not write it as YAML.** The panel consumes deep
   structure (`statement.inventory.items[]`, `response.model_arithmetic`,
   `response.grounding_summary`) and a browser-side YAML parser for that shape
   is the fragile path; the human-readable YAML already exists beside it.

   **Write-set notice — read before you start, `carbon.rs` is no longer
   untouched.** An earlier version of this section said `carbon.rs` belonged
   wholly to the carbon session. That is now imprecise: `ac39c763` (the
   reference-flow guard, §0.2) put three hunks in it. They are in different
   regions from this task and no conflict is expected, but here they are so it
   can be checked rather than assumed:

   | hunk | function | what |
   |---|---|---|
   | ~`281` | `output_shape` | `reference_flow` added, `factor_unit` told to be verbatim |
   | ~`494` | `build_query` | the rule explaining why the basis matters |
   | ~`1887` | `the_statement_is_reproducible_by_hand` | fixture now states `factor_unit` |

   This task lives in `calculate_carbon_handler` — around
   `git.commit_files_as` (~`1447`) and the response `json!` (~`1483`) — so the
   two sets do not overlap. Rebase on `main` first regardless.

   **Three things the implementation has to get right**, each of which is a
   decision already made elsewhere in this handler and would be silently
   undone by a naive version:

   1. **Write the artefact from the ENFORCED document, not the reply.** The
      existing ledger append is explicit about this: "a factor the gate
      stripped is not evidence". An artefact built from the raw reply would
      preserve exactly the values the gate removed, and it would be the copy a
      reader opens.
   2. **A failed run must write no artefact.** The `parse_failure` branch
      returns before the commit and leaves the composition alone — that is
      `c12349a8`'s whole point. A per-action file written on that path would
      reintroduce the "eighteen searches published as the datasets had
      nothing" failure in a new location, and the history panel already renders
      those runs correctly from `apply_result` without one.
   3. **`statement.yaml` stays.** It is the human-readable current pointer and
      `rewrite_carbon_intensity` writes a `statement_ref` at it. The
      per-action JSON is additive, not a replacement.

   **LANDED, both halves, in `5d009dad`.** The handler builds the response
   body and the artefact from one construction (`response_payload`) rather than
   two `json!` blocks, and pushes `dpp/carbon/statements/{action_id}.json` onto
   `files` before the commit. All three constraints above are honoured; the
   enforced-document one is the reason `response_payload` takes `doc` and not
   the reply. `carbonPanelHTML` now takes its run as an argument
   (`carbonPanelHTML(run = carbonRun)`) so a reopened artefact goes through the
   renderer that drew it live, and `carbonRunRowHTML`'s "superseded" note is a
   button calling `carbonOpenRun(action_id, isCurrent)`, which falls back to
   that note on 404 for runs predating the artefact.

   Two things the implementation added that the section did not ask for, both
   because this change fails silently and its own fallback is what hides the
   failure:

   - `the_client_opens_the_path_this_handler_writes` derives the client's path
     from the page and compares it to `run_artefact_path()`. If they ever
     disagree the fetch 404s and the panel reports "no per-run artefact" —
     indistinguishable from the true case it exists to cover. Nothing would go
     red and the message on screen would be a plausible one. The path is now a
     const with a builder so a test can derive it.
   - Nothing syntax-checked `static/adaptogen-lab/index.html`.
     `inline_js_syntax.rs` walked `templates/` and `static/js/`, and neither
     reaches a single-file app holding 171,000 characters of inline JavaScript
     in one `<script>` — the largest concentration of template literals in the
     repo, and so the likeliest home for the defect that suite exists for.
     `every_inline_script_in_every_standalone_page_parses` closes it. Third
     time a scan here has been only as good as the list it scanned.

2. **Barcode standard. DECIDED: no GS1 prefix, so do not pretend to one.**

   Encode a **QR containing the resolver URL** for the workspace product, and
   label the identifier as internal (`part_number`, e.g. `PKH-F2-330`) rather
   than as a GTIN. Any phone camera opens a URL, which is the whole
   requirement; retail scanners are not the use case. If a GS1 prefix is leased
   later, the same QR becomes a GS1 Digital Link by substituting the identifier
   in the URL path — the scanning and resolver work does not change. Someone
   must still confirm the current ESPR data-carrier requirement before any of
   this is described to a customer as compliant.
3. **Public resolver authorisation.** A scannable passport implies an
   unauthenticated read surface. Which fields are public? A carbon total and a
   regulatory verdict are commercially sensitive in a way a claims list is not.
4. **Iteration budget. No longer a prediction — measured, and it is the
   blocker.** Five iterations is not enough for six BOM lines each wanting a
   second publisher, so runs reach the flush degraded. Raising `MAX_ITERATIONS`
   globally makes the flush's input larger and is the wrong lever; per-agent
   budgets are the right shape.

   The production evidence, read out of Neon on 2026-09-17. **One**
   `calculate_carbon` run exists, `138632a9`, 2026-09-16 15:50:

   | field | value |
   |---|---|
   | episode `execution_status` | `failure` |
   | `response_text` length | **0 characters** |
   | `tokens_used` | 151,790 (150,068 in / 1,722 out) |
   | `execution_time_ms` | 157,246 |
   | action `applied` | `true` |
   | `coverage` | `none`, all six lines unpriced |
   | `violations` | 0 |
   | `factors_recorded` | 0 |
   | `inventory_provenance` | `tool_no_match` |
   | `written_paths` | 2 |

   So the agent has never produced usable output, and the shape of the failure
   is exactly `MAX_ITERATIONS = 5` against twelve wanted searches: 150k input
   tokens accumulated across five tool turns, then a flush that returned
   nothing. `carbon_accountant` is **not** the `energy_advisor` problem — its
   prompt matches none of `STRUCTURED_OUTPUT_PATTERNS`, checked field by field
   against the DB copy, so it does receive tools. `energy_advisor` and
   `sidestream_miner` both still match and receive none.

   Two things that are NOT blockers, so nobody re-diagnoses them: mig-238 is
   deployed (`carbon_emission_factors` exists, 0 rows, so the ledger check is
   INERT for want of a successful run rather than for want of a table), and the
   agent card is in sync with the DB — `agent_card_drift.py carbon_accountant`
   reports all 17 seeded fields matching, so **no reseed is needed**.

5. **`tool_no_match` is a proxy, and on an empty statement it is an unearned
   claim. OPEN — needs a signature change in someone else's file.**

   `enforce_from_grounding_map` stamps a `sourced` block with no content
   `tool_no_match`, and says in its own doc comment that this is a *proxy for
   "tool was asked"*. The vocabulary asserts more than that: `hud_contract.rs`
   and `loops.rs` both define it as "the tool answered and had nothing", and
   `grounding_trust::floor` goes out of its way never to substitute one verdict
   for another because misattributing mechanism "is the specific error this
   module exists to prevent".

   Run `138632a9` is that error, shipped. It published `tool_no_match` over six
   materials having never reported back at all. `c12349a8` stopped it recurring
   by writing `outcome: no_reply` on the failure path, but forward-only: that
   row has no `outcome`, and — the part that matters — **a future run which
   searched everything and genuinely found nothing would record identical
   fields.** No rule over `apply_result` can separate the two, so no client-side
   heuristic should try. One was attempted and rejected for exactly the
   "fires on correct behaviour" reason this session has hit three times.

   What shipped instead (`index.html`, mine, in the follow-up commit): a row
   with `coverage: none` and a null total renders as **"nothing concluded"**
   rather than in the shape of a measured run, and both the row and
   `carbonPanelHTML` say that the stamp is a proxy rather than a receipt. That
   sentence is true whichever of the two worlds produced the row, which is why
   it is not a verdict. Pinned by
   `a_run_that_concluded_nothing_is_not_rendered_as_a_calculation`.

   **The real fix is not client-side and is not in my write scope.** Earning the
   stamp needs the tool-invocation count where the statement is built, and
   `dispatch_rabble_action` (`src/handlers/rabble_workspace.rs:199`) returns
   `Result<String, String>` — the reply text and nothing else. The executor
   already collects `tool_invocations`; the count has to survive the dispatch
   hop. `rabble_workspace.rs` is held uncommitted by another session, so this is
   handed over rather than attempted: **widen the dispatch return to carry the
   tool-invocation count, and refuse to publish `tool_no_match` on a statement
   where that count is zero.** `unavailable_no_tool_source` is not the
   substitute either — a tool existed. An empty inventory from zero searches is
   a failed run, which is a shape `c12349a8` already has a branch for.
6. **Restructure ordering. DECIDED: proceed.** The operator is content for the
   UX work to go ahead in parallel with the carbon agent work, on the grounds
   that it is UX and therefore separable. Note the practical constraint this
   creates: `static/adaptogen-lab/index.html` is the contended file and
   `carbon.rs` is not the UX session's to edit. Keep the write sets disjoint
   and follow §6.

---

## 8. Recommended order

Smallest verifiable thing first, because the carbon path has never once been
seen working end to end:

1. **Re-run `calculate_carbon`.** Confirm the flush fix. Expect partial
   coverage, a divergence flag on sugar, `null` on hibiscus, `needs_expert:
   true`. Read `/api/agents/carbon_accountant/metrics` against the §0 baseline.
2. ~~**Hydrate the carbon panel from `GET /files/dpp/carbon/statement.yaml`.**~~
   **DONE — `28b00836`.** Run history from `GET /actions` (survives reloads,
   reports each run's recorded coverage/total/duration/violations/factors), an
   "open committed statement" read from workspace git, failed runs rendered as
   failed rather than as six confident `no factor` rows, and the pulse panel
   widened to both credit-spending agents. Full per-run documents await §7.1.
3. ~~**Add `reference_flow`**~~ **DONE — `ac39c763`.** See §0.2. The residual
   same-unit-different-substance case rides with the `geography` material
   match.
4. **Centre-panel run history** from `GET /actions`, plus the §7.1 decision.
5. **Lens registry** (§2) — refactor the three existing lenses into it *without*
   adding a fourth, so the abstraction is proven against known cases.
6. **Passport view + barcode** behind the §7.2/§7.3 answers.
7. **Label ingest** (§4) — largest, and it needs a new agent card.
8. **MCP registration** (§5) — do last; it freezes the action surface into a
   published contract, and it should freeze a settled one.
