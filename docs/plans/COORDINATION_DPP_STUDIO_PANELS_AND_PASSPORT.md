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

### 0.2 An unfixed hole, specified here because it is worse than the bug I fixed

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

## 7. Open decisions — these need the operator or a judgement call

1. **Statement history storage.** Per-action path
   (`dpp/carbon/statements/{action_id}.yaml`) versus fattening `apply_result`.
   Per-action files keep workspace git as the record and make the passport
   diffable; they also grow the repo per run. *Recommendation: per-action path,
   with `statement.yaml` kept as a pointer to the current one.*
2. **Barcode standard.** Cannot proceed past a placeholder without knowing
   whether the operator holds a GS1 prefix, and without someone confirming the
   current ESPR data-carrier requirement.
3. **Public resolver authorisation.** A scannable passport implies an
   unauthenticated read surface. Which fields are public? A carbon total and a
   regulatory verdict are commercially sensitive in a way a claims list is not.
4. **Iteration budget.** Five iterations is not enough for six BOM lines each
   wanting a second publisher, so runs reach the flush degraded and coverage is
   partial by construction. Raising `MAX_ITERATIONS` globally makes the flush's
   input larger and is the wrong lever. Per-agent budgets are the right shape.
   Judge it after a successful run, with the factor ledger populated.
5. **Whether the operator wants the restructure before or after a verified
   carbon run.** The panels are a larger change than everything else here
   combined, and the carbon path is one re-run away from being demonstrable.

---

## 8. Recommended order

Smallest verifiable thing first, because the carbon path has never once been
seen working end to end:

1. **Re-run `calculate_carbon`.** Confirm the flush fix. Expect partial
   coverage, a divergence flag on sugar, `null` on hibiscus, `needs_expert:
   true`. Read `/api/agents/carbon_accountant/metrics` against the §0 baseline.
2. **Hydrate the carbon panel from `GET /files/dpp/carbon/statement.yaml`.** One
   fetch. Fixes the reported bug for the current run.
3. **Add `reference_flow`** (§0.2) before anyone trusts a number, because the
   failure it prevents is silent.
4. **Centre-panel run history** from `GET /actions`, plus the §7.1 decision.
5. **Lens registry** (§2) — refactor the three existing lenses into it *without*
   adding a fourth, so the abstraction is proven against known cases.
6. **Passport view + barcode** behind the §7.2/§7.3 answers.
7. **Label ingest** (§4) — largest, and it needs a new agent card.
8. **MCP registration** (§5) — do last; it freezes the action surface into a
   published contract, and it should freeze a settled one.
