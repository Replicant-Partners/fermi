# Reading a carbon statement from `carbon_accountant`

**Audience:** anyone integrating with the agent, rendering its output, or
deciding whether to rely on it. Not an authoring guide — for that, see
`docs/guides/AGENT_CONTRACT_AUTHORING.md`.

**Sources of truth:** `agents/curated/carbon_accountant/agent_card.json`,
`agents/curated/carbon_accountant/output_contract.sketch.json`,
`src/handlers/workspace/carbon.rs`, `src/grounding_trust.rs`,
`migrations/238_carbon_emission_factors.sql`,
`apps/adaptogen-lab/regulatory-lens/manifest.json`.

---

## 1. What it produces

One document, type `adaptogen-lab/carbon_statement`. Five blocks, four
provenance stamps:

| key | what it holds | stamp |
|---|---|---|
| `inventory` | retrieved emission factors + the platform's arithmetic over them | `inventory_provenance`: `tool_verified` \| `tool_no_match` \| `unavailable_no_tool_source` |
| `attribution` | GHG Protocol scope and category per BOM line | `attribution_provenance`: `model_inference` (const) |
| `boundary` | what the figure covers, what it excludes, under which standard | `boundary_provenance`: `model_inference` (const) |
| `assurance` | what the statement is fit to be relied on for | `assurance_provenance`: `model_inference` (const) |
| `explanation` | the prose an operator reads | **none** |

`explanation` carries no stamp on purpose: a retrieval verdict about a sentence
is a category error. The schema (`additionalProperties: false`) declares exactly
these nine keys and requires all nine.

The persisted file differs in shape from the API response, and integrators
should not assume one from the other:

| | API response (`statement`) | `dpp/carbon/statement.yaml` |
|---|---|---|
| provenance | flat `<block>_provenance` keys | nested under `provenance:` |
| arithmetic stamp | absent | `provenance.arithmetic: platform_derived` |
| extras | — | `product_id`, `calculated_by`, `calculated_at`, `action_id`, `model_arithmetic`, `endorsement: null` |

### Worked example — the seeded product, `partial` coverage

The seeded BOM is `precision_kombucha_hibiscus_f2`, six lines, serving basis
330 ml. Percentages are read as w/v against that serving with 1 ml taken as 1 g,
so hibiscus at 8.5% resolves to 0.02805 kg. `starter_scoby` is declared `trace`
and resolves to no quantity at all. The `black_tea` line below spells out every
key; the others are abbreviated, and the keys they omit are present and `null`.

**The factor values below are illustrative of the document's shape. They are not
retrieved factors and must not be reused as if they were** — which is the entire
point of an agent whose output carries a `source_url` per line.

```json
{
  "inventory": {
    "queries_run": [
      "ecoinvent dried hibiscus calyces Egypt emission factor",
      "Agribalyse black tea dried cradle to gate kg CO2e",
      "cane sugar Brazil emission factor ecoinvent 3.9",
      "citric acid E330 emission factor",
      "process water treated emission factor kg CO2e per kg"
    ],
    "items": [
      {
        "item_id": "water",
        "bom_name": "Water (process grade)",
        "material": "treated process water",
        "activity_qty_kg": 0.29139,
        "activity_basis": "88.3% w/v of a 330 ml serving, 1 ml taken as 1 g",
        "factor_kg_co2e_per_kg": null,
        "dataset": null,
        "source_url": null,
        "corroboration": "absent",
        "kg_co2e": null,
        "arithmetic": null
      },
      {
        "item_id": "black_tea",
        "bom_name": "Black tea (Camellia sinensis)",
        "bom_origin": "Sri Lanka",
        "material": "black tea, dried, at farm",
        "activity_qty_kg": 0.00264,
        "activity_basis": "0.8% w/v of a 330 ml serving, 1 ml taken as 1 g",
        "factor_kg_co2e_per_kg": 12.5,
        "factor_unit": "kg CO2e/kg",
        "lca_basis": "cradle_to_gate",
        "geography": "LK",
        "reference_year": 2021,
        "dataset": "ecoinvent 3.9.1",
        "source_url": "https://…",
        "source_title": "…",
        "source_quote": "…",
        "corroborating_value_kg_co2e_per_kg": 11.2,
        "corroborating_dataset": "Agribalyse 3.1",
        "corroborating_source_url": "https://…",
        "corroboration": "agreeing",
        "kg_co2e": 0.033,
        "arithmetic": "0.00264 kg x 12.5 kg CO2e/kg = 0.033 kg CO2e"
      },
      {
        "item_id": "cane_sugar_residual",
        "bom_origin": "Brazil / Australia",
        "activity_qty_kg": 0.00693,
        "factor_kg_co2e_per_kg": 0.6,
        "geography": "BR",
        "reference_year": 2021,
        "dataset": "ecoinvent 3.9.1",
        "corroborating_value_kg_co2e_per_kg": 1.2,
        "corroborating_dataset": "Agribalyse 3.1",
        "corroboration": "diverging",
        "kg_co2e": 0.004158,
        "arithmetic": "0.00693 kg x 0.6 kg CO2e/kg = 0.004158 kg CO2e"
      },
      {
        "item_id": "starter_scoby",
        "bom_name": "SCOBY starter culture",
        "activity_qty_kg": null,
        "activity_basis": null,
        "factor_kg_co2e_per_kg": null,
        "corroboration": "absent",
        "kg_co2e": null,
        "arithmetic": null
      },
      {
        "item_id": "hibiscus_infusion",
        "bom_origin": "Egypt / Sudan",
        "material": "dried hibiscus calyces",
        "activity_qty_kg": 0.02805,
        "activity_basis": "8.5% w/v of a 330 ml serving, 1 ml taken as 1 g",
        "factor_kg_co2e_per_kg": 2.1,
        "lca_basis": "cradle_to_gate",
        "geography": "EG",
        "reference_year": 2021,
        "dataset": "ecoinvent 3.9.1",
        "corroborating_value_kg_co2e_per_kg": null,
        "corroborating_dataset": null,
        "corroboration": "single_source",
        "kg_co2e": 0.058905,
        "arithmetic": "0.02805 kg x 2.1 kg CO2e/kg = 0.058905 kg CO2e"
      },
      {
        "item_id": "citric_acid",
        "activity_qty_kg": 0.00099,
        "factor_kg_co2e_per_kg": 1.6,
        "geography": "RER",
        "reference_year": 2019,
        "dataset": "ecoinvent 3.9.1",
        "corroborating_value_kg_co2e_per_kg": 1.45,
        "corroborating_dataset": "supplier EPD",
        "corroboration": "agreeing",
        "kg_co2e": 0.001584,
        "arithmetic": "0.00099 kg x 1.6 kg CO2e/kg = 0.001584 kg CO2e"
      }
    ],
    "total_kg_co2e": 0.097647,
    "coverage": "partial",
    "unpriced_items": ["water", "starter_scoby"]
  },
  "inventory_provenance": "tool_verified",

  "attribution": {
    "items": [
      { "item_id": "hibiscus_infusion", "scope": "scope_3",
        "ghg_protocol_category": 1,
        "rationale": "Purchased botanical input, bought as a processed infusion; upstream of the reporting entity's gate." },
      { "item_id": "starter_scoby", "scope": "scope_3",
        "ghg_protocol_category": 1,
        "rationale": "Process aid, purchased. In scope as a purchased good even though no factor resolved and it carries no quantity." }
    ],
    "scope_boundary_note": "Organisational boundary assumed at the beverage producer's own F2 facility; the infusion is bought in rather than extracted on site."
  },
  "attribution_provenance": "model_inference",

  "boundary": {
    "declared": "cradle_to_gate",
    "included": ["raw material acquisition", "upstream processing", "inbound transport where bundled into the dataset's reference flow"],
    "excluded": ["packaging", "outbound distribution", "retail refrigeration", "use phase", "end-of-life"],
    "exclusion_rationale": "Distribution, use and end-of-life are outside a cradle-to-gate boundary by the standard. Packaging is excluded for want of data: it is not on the bill of materials at all, so nothing here can speak to it. The two exclusions are not equivalent and should not be read as one caveat.",
    "standard_followed": "GHG Protocol Product Standard"
  },
  "boundary_provenance": "model_inference",

  "assurance": {
    "needs_expert": true,
    "verification_status": "unverified",
    "regulatory_fitness": [
      { "regime": "CSRD/ESRS E1", "fit": "screening_only",
        "why": "Four of six lines priced from database averages, two geographies proxied, one line diverging between publishers. Disclosable with the method and the coverage stated; not presentable as a measured figure." },
      { "regime": "CBAM", "fit": "screening_only",
        "why": "No installation-level actual data for any line, and the product is not a CBAM good in any case; the figure could not support an embedded-emissions report." },
      { "regime": "EU Battery DPP", "fit": "inadmissible",
        "why": "Requires a declaration to a prescribed method with third-party verification. verification_status is unverified, so the factor quality is not the binding constraint." }
    ],
    "blocking_gaps": [
      "Supplier-specific factor for hibiscus_infusion from the Egyptian supplier, replacing the EG dataset average.",
      "Resolve the cane_sugar_residual divergence: 0.6 (ecoinvent, BR) against 1.2 (Agribalyse) is a question about which reference flow is being described, not a pair to average.",
      "A factor or an explicit exclusion for process water; today it is 75% of the mass and 0% of the total.",
      "Packaging mass and material, which the bill of materials does not carry."
    ]
  },
  "assurance_provenance": "model_inference",

  "explanation": "Four of six lines resolved. The total is a subset: process water and the starter culture carry no factor, and water is the largest single mass in the product. Of what did resolve, the hibiscus dominates. Two lines rest on a geography other than the BOM's stated origin. This figure is cradle-to-gate and says nothing about packaging, distribution, chilled retail or disposal — those are absent, not zero."
}
```

`coverage: partial` with named `unpriced_items` is the **normal** case for a
real product, not an exception. Process-grade water, a starter culture and a
trace acidulant frequently have no published factor at the grain a product
statement needs. A `complete` statement over a six-line food BOM should be
read with more suspicion than a partial one, not less.

---

## 2. The one thing to understand: three claims, three different checks

A product carbon figure is three claims wearing one number. They have different
best-available checks, and the document keeps them apart so that no surface can
read one as the other.

| part | example | tier | how it is checked |
|---|---|---|---|
| emission factor | `dried hibiscus, EG: 2.1 kg CO2e/kg` | `Sourced` | **falsifiable only** — cross-run ledger + a within-run second publisher |
| arithmetic | `0.02805 × 2.1 = 0.058905` | `Derived` | **verified to correctness** — the platform computes it |
| scope / boundary / fitness | `Scope 3 cat 1`; `use phase excluded` | `Inferred` | **not mechanically checkable** — routes to a human |

The asymmetry follows from one question: *does the platform hold the referent?*

- **Arithmetic is the strong tier because the platform holds the referent
  entirely.** It does the multiplication itself, in Rust, inside
  `grounding_trust::enforce`, overwriting whatever the reply carried
  (`DERIVATIONS` registers `inventory.items`, `inventory.total_kg_co2e`,
  `inventory.coverage`, `inventory.unpriced_items`). `activity_qty_kg` is
  echoed from the committed BOM, not taken from the reply, so both inputs are
  the platform's or are cited. After that, a disagreement between the document
  and `qty × factor` is a platform bug rather than the agent's. The reason this
  half is owned rather than trusted: a confidently wrong product of two
  plausible numbers looks exactly like a right one, and nobody recomputes a
  figure that came back formatted.
- **The factor can only be falsified, because the referent is licensed and
  absent.** The platform holds no copy of ecoinvent, Agribalyse, the World Food
  LCA Database, the DEFRA/BEIS factors or the IPCC GWP tables. Nothing here can
  confirm `2.1`; the checks that exist can only catch it being inconsistent
  with a second reading (§5).
- **Scope, boundary and fitness have no referent at all.** No dataset row states
  a GHG Protocol category. Ecoinvent gives a factor for a material; the decision
  that a purchased botanical infusion is upstream category 1 while the
  electricity for the same plant's F2 tank is Scope 2 is a classification over
  retrieved facts, made against an organisational boundary the dataset knows
  nothing about. Stamped `model_inference` so a reviewer endorsing that block
  knows they are endorsing the *reasoning*, not the retrieval.

The arithmetic lives inside `inventory` rather than in a `computation` block of
its own, and that is forced rather than aesthetic: `enforce` stamps a block from
the strongest thing in it, so an all-`Derived` block would be stamped
`platform_derived` — a value `card_contract::GROUNDING_STATUSES` has no
authoring token for. The card would then have to declare a stamp the runtime
never writes. Folding the derivation in beside the factors it is computed from
keeps the block `sourced` and the stamp true.

The model's own arithmetic is **measured before it is discarded**, and reported
as `model_arithmetic: {lines_the_model_priced, disagreements,
worst_relative_error}`. A derivation makes every stored row correct by
construction and would otherwise conceal exactly the behaviour the design
forbids.

---

## 3. How to call it

```
POST /api/workspaces/:id/actions/calculate_carbon
```

| field | type | default | meaning |
|---|---|---|---|
| `boundary` | `cradle_to_gate` \| `cradle_to_grave` \| `gate_to_gate` | `cradle_to_gate` | The boundary to *attempt*. `boundary.declared` reports what the retrieved factors can actually defend, which may be `indeterminate`. |
| `region` | string | `EU` | Reporting region, used to bias factor geography. |
| `force` | bool | `false` | Recalculate even though a statement exists. |
| `source_message_id` | UUID string | — | Optional link to the workspace message that triggered the run. |

Response keys: `action_id`, `action_type`, `product_id`, `statement`,
`statement_path`, `written_paths`, `composition_updated`, `boundary_requested`,
`region`, `grounding_summary` (`{is_clean, violation_count, provenance[]}`),
`model_arithmetic`, `factor_ledger` (`{recorded, not_comparable, hints_offered,
note}`), `duration_ms`, `cost` (`credits_charged` is always `null` — credits are
charged asynchronously *after* the response is built, and quoting an estimate
beside a real duration would read as though both were measured).

### It reads the committed document, not a posted BOM

There is no BOM in the request body. The handler reads `dpp/composition.yaml`
from the workspace git repository **at HEAD**, so uncommitted working-tree
edits are not included in the statement. If that path does not exist at HEAD it
falls back to the platform copy shipped at
`apps/adaptogen-lab/dpp/composition.yaml` — so a workspace that was never
seeded produces a statement about the *fixture* product. Check `product_id` in
the response against the product you meant.

### Refusals, and why refusing beats degrading

| status | condition |
|---|---|
| `503` | `carbon_accountant` is not installed / its card does not resolve |
| `503` | no `brave_search` credential is reachable |
| `404` | no `dpp/composition.yaml` anywhere in the chain |
| `422` | the composition does not parse as YAML, or carries no `consists_of` entries |
| `504` | the run exceeded `RUN_TIMEOUT_SECS` = 300 |
| `502` | the dispatch to the agent failed |

The search-credential refusal is the one worth understanding, because the
alternative is not "a worse statement" but "an unfalsifiable one". `web_search`
reads its key at call time and, when it is absent, returns the string
`BRAVE_SEARCH_API_KEY environment variable not set` **as its tool result**
rather than raising. The model is handed that string and does the only thing it
can: answers from training data. The run then *succeeds*. Every factor arrives
uncited, and because a sourced field did come back populated, the block is
stamped `tool_no_match` — "the datasets were asked and had nothing" — which is
indistinguishable from a real empty search. A missing key is an operator problem
with a one-line fix; a workspace of carbon statements that quietly came from
model memory is not fixable at all, because nothing separates them from the real
ones.

### The default is a cache read

With `force` absent or false, an existing `dpp/carbon/statement.yaml`
short-circuits the whole run:

```json
{
  "action_type": "calculate_carbon",
  "skipped": true,
  "reason": "already_calculated",
  "statement_path": "dpp/carbon/statement.yaml",
  "duration_ms": 3,
  "note": "A statement already exists. …Pass `force: true` to recalculate."
}
```

A client must branch on `skipped` before reading `statement`. Emission factors
are expensive to retrieve and do not change between runs; a run is minutes and
real credits.

---

## 4. Reading the output honestly

This is the section a consumer cannot skip.

**`inventory_provenance: tool_verified` means a tool answered. Nothing more.**
Precisely: at least one `Sourced` field on at least one line came back
populated. It does **not** mean the total is complete — that is
`inventory.coverage` — and it does not mean the factor is right. A single
resolved line out of six yields `tool_verified`.

**`coverage: partial` means `total_kg_co2e` is a SUBSET.** It is derived by the
platform from the ratio of priced lines to BOM lines (`complete` when every
echoed line resolved, `none` when none did, `partial` otherwise), and
`unpriced_items` names the missing `item_id`s in BOM order. Named rather than
counted, so the gap is a work item rather than a caveat. **A subset rendered as
a footprint is the failure these two fields exist to prevent.** If your surface
can show `total_kg_co2e` without also showing `coverage` and `unpriced_items`,
your surface has a bug.

**`total_kg_co2e: null` when nothing resolved — never `0`.** Zero is a footprint
claim, and the strongest one in the document. It is also the value an empty sum
produces by accident, which is why the derivation returns `null` explicitly. Do
not coalesce it to zero anywhere: not in a chart axis, not in a sum across
products, not in a CSV export.

**`corroboration` is a deliberately weak verdict.** Computed by the platform,
never by the agent, on a 30% band (`CORROBORATION_BAND`):

| value | meaning |
|---|---|
| `agreeing` | a second publisher is within 30%. Read as: *probably the right material, right order of magnitude*. **Not** that the number is accurate. |
| `diverging` | more than 30% apart. At least one of the two does not describe what you think it does. A question for a person — **never a pair to average**. |
| `single_source` | only one inventory carries the material. Honest and common. Not a failure. |
| `absent` | no primary factor resolved at all. |

The band is 30% and not the ledger's 2% because the two answer different
questions: two readings of the *same* dataset row must be the same number, while
two *different* publishers legitimately differ by 20–30% through different system
models, allocation rules and reference flows. A tight band here would fire on
correct behaviour, and a check that fires on correct behaviour gets switched off.
Pushing an agent toward a second citation it cannot honestly find would be
strictly worse than `single_source`, because a fabricated corroboration converts
a known weakness into a false assurance.

**`verification_status` is always `unverified` as produced.** The agent is not an
accredited verifier and no run of it can be. The stronger values —
`internal_review`, `third_party_limited`, `third_party_reasonable` — exist so a
human endorsement lands on the same field a consumer already reads rather than
in a comment beside it. The persisted YAML carries `endorsement: null` for the
same reason: nobody having signed off is different from a rejection.

**There is no confidence score, and you should not add one.** A self-rated number
on a document that may be read against ESRS E1 is a shading where a decision is
needed. `0.7` is not actionable; `unverified` is. The decisions are:

- `assurance.needs_expert` — true whenever coverage is incomplete, geographies
  or years are mismatched against the BOM origins, the bases are mixed, or any
  line came back `diverging`. Under-flagging is the expensive direction, because
  the reader of a carbon statement is usually not the person who could tell that
  a 2019 European average was standing in for a named Egyptian supplier.
- `assurance.verification_status` — in the vocabulary an auditor uses.
- `assurance.regulatory_fitness[].fit` — `adequate` \| `screening_only` \|
  `inadmissible`, per regime.

**`explanation: null` means the grounding gate nulled the prose.** The narrative
is scanned against the `inventory` block, and the scan only fires when no
sourced field in `inventory` came back. In that state, naming a factor database
(`ecoinvent`, `agribalyse`, `defra`, `exiobase`, `world food lca`, `ipcc`),
saying `emission factor`, `scope 3`, `carbon neutral` or `climate neutral`, or
stating any digit-led quantity in CO2e units (`kg co2e`, `kgco2e`, `g co2e`,
`t co2e`, `tco2e`, `kg co₂e`) nulls the whole paragraph. Nulled rather than
flagged, because a gate cannot rewrite a sentence into honesty, and because
`parse_evidence_text` lifts this string into the episode digest — so an
unchecked sentence travels further than the document does. Render a null
`explanation` as "the summary was withheld", never as an empty paragraph.

Methodology words are deliberately **not** needles: `ghg protocol`,
`iso 14067`, `scope 1`, `scope 2` and `cradle-to-gate` name a method, and an
agent that retrieved nothing can still honestly say which standard it read the
absent factors against.

---

## 5. Why agreement is weak evidence

The factor cross-check compares two runs that resolved the same
`(material, geography, reference_year, dataset)` key in the
`carbon_emission_factors` ledger, with a 2% transcription band. It is worth
being exact about what that buys, because a cross-check whose reach is
overstated is worse than none.

**Disagreement is sound.** If two readings of one published figure differ by
more than transcription slack, at least one is wrong. That is a real finding
every time, and the check *only ever fires on disagreement* — so it is sound in
the direction in which it makes claims.

**Agreement proves close to nothing**, because the two observations are not
independent:

1. **Shared source.** Both runs share the corpus, the ranking and the query
   shape, so they tend to land on the same page. If that page misquotes the
   dataset, both runs are wrong identically. Note the perverse direction: the
   *more* reliable retrieval is, the *more* likely two runs converge on one
   document, so improving the search strengthens the correlation rather than
   the evidence.
2. **Correlated prior.** It is the same model with the same weights. Its errors
   are correlated *through* those weights. Two samples from one posterior are
   not two observations of the world.
3. **The join key is itself model-reported.** `material`, `geography`,
   `reference_year` and `dataset` all come from the agent. A dataset mislabelled
   the same way twice yields two rows joined on a key that describes neither,
   and they agree.

The design attacks (1) and (2) where it can. The **hint mechanism** exists for
this: when an earlier run has resolved a line, the query passes forward the
material, dataset, geography, year and URL and **withholds the value**. The
agent is spared the expensive part — working out which dataset row applies —
and must still read the figure itself. Handing back the cached number would
make the second run echo the first, the two ledger rows would agree by
construction, and agreement built from a number the platform supplied is
evidence of nothing. The ledger's `retrieval = 'search'` predicate on both sides
of the self-join is the same guard, written into the query rather than left in
somebody's memory.

Routes to stronger evidence, and what blocks each:

| route | what it would establish | blocked on |
|---|---|---|
| **tool-result join** | that each `source_url` actually appeared in a `web_search` result in the same episode. Strictly the strongest available check: a real ecoinvent page the agent never opened passes a replay and fails this. | `web_search` results are not persisted per episode, so the query would join against nothing. |
| **URL replay** | that the URL exists and the quoted factor is still on the page. | egress; licence walls (most LCA dataset pages); PDF-only sources; link rot, which would be indistinguishable from fabrication. |
| **licensed corpus** | the factor itself. **The only thing that would make agreement strong evidence rather than weak.** | procurement. ecoinvent, Agribalyse and the WFLDB are licensed, not merely absent. |
| **decorrelated retrieval** | that the primary factor is the right material and order of magnitude, independent of run-to-run correlation. | **done** — this is the second-publisher `corroborating_*` mechanism, and it is the one part of the document that attacks correlation rather than error. |

The independence property the second-publisher mechanism depends on is itself
checked rather than hoped for: `inventory.items[].corroborating_dataset` carries
a live cross-check counting lines where the two dataset names normalise to the
same string. That is not fabrication — it is a corroboration that decorrelated
nothing, and the distinction is worth having a number for.

---

## 6. What it refuses to do

| refusal | the failure it prevents |
|---|---|
| **No arithmetic by the model.** `kg_co2e`, `arithmetic`, `total_kg_co2e`, `coverage`, `unpriced_items` and `activity_qty_kg` are written by the platform over whatever the reply carried. | A wrong product of two plausible numbers is invisible and nobody recomputes a formatted figure. |
| **No proxy substitution without labelling.** A line with no retrievable factor carries `factor_kg_co2e_per_kg: null`. A proxy material may be used only when the entry says it is a proxy. | A European average silently standing in for a named Egyptian supplier is defensible when declared and indefensible when not — and invisible once the number is formatted. |
| **No zero for a missing factor.** Null at the line, null at the total. | Zero is the one value that makes the total wrong in the direction nobody checks. |
| **No carbon-neutral / climate-neutral / offset claim.** Forbidden in the prompt and a `NARRATIVE_LEAKS` needle. | EU Directive 2024/825 restricts generic environmental claims and those resting on offsetting. A field shaped to hold such a conclusion invites it, so no field exists. |
| **No writing of the statement file by the agent.** The platform persists the *enforced* document after `enforce` has run. | A direct `write_workspace_file` would keep the fields enforcement was about to strip, and every later read would serve them as though they had passed. |
| **No emission factors in the handler.** There is no fallback table and no "typical beverage" default in `carbon.rs`. | A hardcoded factor would be `mode: synthetic` one layer down and harder to see — worse than the typed `0.41`, because it would arrive wearing a provenance stamp. |

---

## 7. Regulatory fitness

`assurance.regulatory_fitness` is an array of `{regime, fit, why}`. `fit` is
`adequate` \| `screening_only` \| `inadmissible`. **An empty array means no
regime was assessed, which is different from a regime being satisfied** — do not
render an empty array as a pass.

The same number differs across regimes because the regimes have different
evidentiary standards, not because the number changes. The agent's prompt names
these, and the descriptions below are the ones it reasons with; they are
orientation for reading its verdict, **not legal advice**, and a qualified
reviewer decides:

| regime | why the same figure lands differently |
|---|---|
| **CSRD / ESRS E1** | Figures are *reported* and subject to assurance. A screening estimate built from database averages is disclosable **with its method and coverage stated** — but it is not presentable as a measured figure. |
| **CBAM** | The definitive regime starts in 2026, with importers surrendering certificates against embedded emissions. Installation-level actual data is what the regime expects; default values are a fallback. A database average is therefore `screening_only` at best. (Also worth stating when true: most food and drink products are not CBAM goods at all.) |
| **EU Battery DPP** | A carbon footprint declaration to a prescribed method, third-party verified. An unverified statement is `inadmissible` **however good the factors are** — the binding constraint is `verification_status`, not factor quality. |
| **EU ETS** | Covers installations, aviation and maritime. A product footprint is not an ETS figure, and saying so is more useful than a fitness verdict. |
| **Green Claims / Directive 2024/825** | A consumer-facing environmental claim needs substantiation; generic environmental claims and claims resting on offsetting are restricted. Relevant to what you may *print*, which is a different question from what you may disclose. |
| **ISO 14067** | A methodology rather than a filing regime. Its relevance here is whether `boundary.standard_followed` can honestly name it. |

`blocking_gaps` is the actionable half: what would have to be obtained to move
up a tier, written as work items — a supplier-specific factor for a named line,
a transport leg, a verified electricity mix. A gap nobody can action is a
disclaimer, and this field is what the document has instead of disclaimers.

---

## 8. Where it sits next to the SimOps fleet — the seam that matters

`energy_advisor` also produces carbon numbers, and a consumer must not sum or
compare them naively. The distinction is one of **scope** and of **evidence
tier**, and only the first is a difference in job.

### Different scope, and the seam is already acknowledged

| | `carbon_accountant` | `energy_advisor` |
|---|---|---|
| unit of analysis | per **product**, from BOM materials | per **process stage** |
| output | `adaptogen-lab/carbon_statement` | `kask_simops/energy_proposal` (declared in `capabilities.simops_contract`) |
| carbon field | `inventory.items[].kg_co2e`, `inventory.total_kg_co2e` | `carbon_intensity` — kg CO₂eq per unit of **output** — plus `carbon_unit`, `carbon_rationale`, `carbon_confidence`, `carbon_typical_range` |
| fleet tag | `dpp-orchestra` | `simops-orchestra`, `simops_contract.extension_task: "energy"` |

These are complementary: purchased materials against stage energy. The seam is
already acknowledged in `energy_advisor`'s own prompt, which puts the sugar
feedstock at ~0.6 kg CO₂eq/kg and says in as many words that it *"is a BoM
concern, not a stage concern"*. That is exactly right, and exactly the line
between the two agents.

### Very different evidence tier

This is not criticism of `energy_advisor`. Its stated job — "give the operator a
starting point they can correct, not a form to fill" — is a legitimate and
useful job, and its prompt is honest about being that. But the two documents do
not carry the same weight, and that has to be visible to anyone consuming both.

From the live specimen (`GET /api/specimen/energy_advisor`) and the card:

- `declares_contract: false`, `typed: false`. The declaration ladder reads **1 of
  4 rungs** — ports only: no `output_type`, no `output_schema`, no
  `field_contract`. It is `grandfathered: true` and listed on
  `workflows::agent_contract::TYPED_TIER_EXEMPT`.
- No field contract means **no `Grounding::Sourced` fields, no provenance
  stamps, no cross-checks, and nothing that can say whether a value was
  fabricated**. `sourced_fields: 0`.
- `prompt_check.gets_tools: false`, `trigger: "Return a valid JSON"`. The prompt
  demands a JSON-only reply, which makes `ToolAwareExecutor` hand the request
  straight to the model: **it receives no tools at runtime**, even though its
  card declares `web_search`. Its carbon numbers therefore come from parametric
  memory.
- Its own worked example cites *"ecoinvent 3.8 + Defra 2024 grid factor"* as the
  source of a factor it did not retrieve, and performs the multiplication itself
  in the rationale text (`0.005 kWh × 0.4 kg CO₂/kWh ≈ 0.002 kg`). It emits
  `carbon_confidence: 0.7` and `carbon_typical_range: [0.02, 0.08]`.

Read against §2, that is precisely the inverse split: `energy_advisor` supplies
a recalled factor, its own arithmetic and a self-rated confidence;
`carbon_accountant` supplies a cited factor, the platform's arithmetic and a
decision instead of a score.

### What a consumer must therefore not do

1. **Do not add the two numbers** without first deciding a boundary and writing
   it down. Stage energy and purchased materials can both belong inside a
   cradle-to-gate figure, but `energy_advisor`'s is per unit of *stage output*
   and `carbon_accountant`'s is per *serving of finished product* — different
   denominators, and the double-counting risk runs both ways (grid electricity
   may already sit inside a cradle-to-gate material factor).
2. **Do not treat them as interchangeable**, and do not let a UI render them in
   one column. One is a defensible starting point to correct; the other is a
   disclosure-grade statement with citations and platform arithmetic.
3. **Where they conflict, carry the cited one into a disclosure.** Not because
   `energy_advisor` is careless, but because a disclosure has to be traceable to
   a document, and only one of the two produces a `source_url`.

---

## 9. Integration notes

**Fleet coordination.** `carbon_accountant` carries the `dpp-orchestra` tag,
shared with `dpp_companion`, `regulatory_lens_translator` and
`supply_chain_oracle`. `dpp_companion` discovers its fleet by that tag at
session start and routes by role rather than by agent id, so a workspace may
substitute a carbon specialist and the routing follows.

**Ports.** `produces: ["adaptogen-lab/carbon_statement"]`. `accepts:
["adaptogen-lab/carbon-query/1", "bom:Item", "bill_of_materials",
"product_composition", "reporting_boundary"]`. `dependencies.required` and
`.optional` are both empty; the one hard runtime dependency — a reachable
`brave_search` credential — is enforced by the handler's preflight rather than
declared as a dependency.

**Files the action writes.** One commit, up to two paths, reported in
`written_paths`:

1. `dpp/carbon/statement.yaml` — the enforced document, with a header comment
   stating that the arithmetic is the platform's and reproducible by hand, and
   that the factors are cited but not independently verified.
2. `dpp/composition.yaml` — the `carbon_intensity:` block is rewritten in place,
   line by line rather than through a `serde_yaml` round-trip, so every comment
   in the file survives. `composition_updated: false` means the composition
   carried no top-level `carbon_intensity:` key, which is a real state and not
   an error.

The rewritten block:

```yaml
carbon_intensity:
  # mode vocabulary:
  #   synthetic           a person typed the number. Not evidence.
  #   agent_calculated    retrieved factors x BOM quantities, the
  #                       arithmetic performed by the platform. See
  #                       statement_ref for the factors and their URLs.
  #   supplier_declared   a supplier's own figure, ideally an EPD.
  mode: agent_calculated
  value_kg_per_kg: 0.2959
  coverage: partial
  # coverage is not complete: this intensity is a SUBSET of the product's
  # footprint. statement_ref names which lines are missing.
  scope_3_dominant: hibiscus_infusion
  statement_ref: dpp/carbon/statement.yaml
```

The `mode` vocabulary is the point of the block, not decoration: **an operator
must be able to tell a retrieved number from a typed one and from a supplier's
declaration.** `value_kg_per_kg` is `total_kg_co2e ÷ serving mass` — here
0.097647 ÷ 0.33 kg, on the same 1 ml = 1 g assumption as the numerator, so the
denominator matches. It is `null` (never zero) when no line resolved or no
serving mass exists to divide by. `scope_3_dominant` is the `item_id` with the
largest `kg_co2e`.

**Audit anchors.** The run is logged to `workspace_action_log` with
`action_type: "calculate_carbon"` before the agent runs, so the persisted
statement can reference its `action_id`; the outcome is written back to
`apply_result`. The transcript message is dispatched as `message_type:
"agent_action"` rather than `"calculate_carbon"`, because
`workspace_messages.message_type` carries a CHECK constraint admitting a closed
list — a fresh token fails it silently and the reply never reaches the
transcript. The action's own identity lives in `workspace_action_log.action_type`.
Both the log insert and the ledger insert soft-fail: the statement is the
product, and a missing migration must not lose work that has already cost real
searches. The consequence is named in the response
(`factor_ledger.not_comparable`, `written_paths`), because an `action_id` in no
table is an audit anchor pointing at nothing.

---

## 10. Known limits

Stated completely, because a limit nobody wrote down is a limit somebody
discovers in a filing.

- **The factors are cited but not independently verified.** Seven `Sourced`
  fields on this agent sit in `grounding_trust::CROSS_CHECK_EXEMPTIONS`:
  `source_url`, `dataset`, `reference_year`, `geography`, `lca_basis`,
  `corroborating_value_kg_co2e_per_kg`, `corroborating_source_url`. Each entry
  names its own route out. Only three cross-checks are live:
  `factor_kg_co2e_per_kg` (the ledger self-join), `corroborating_dataset` (the
  two publishers actually differ) and `inventory.items` (the model's own
  arithmetic against `qty × factor`).
- **The ledger cross-check reports INERT, not clean, until two runs resolve the
  same key.** `carbon_emission_factors` is empty on every fresh deployment, and
  an empty table yields zero mismatches — which would read as a verified claim
  about emission factors on the strength of having compared nothing.
  `CROSS_CHECK_COVERAGE` supplies the denominator so the harness says INERT
  instead, and `tests/grounding_contract.rs` prints *"INERT is not a pass"*.
- **The corroborating reading is not yet recorded in the ledger.** It would
  need its own geography and reference year to be placed on the right key, and
  inventing those from the primary's would fabricate the very metadata the key
  depends on. So the second publisher is visible in the document and invisible
  to the cross-check.
- **`reference_year` staleness is unchecked.** A 2014 grid factor in a 2026
  statement is wrong by a wide margin and renders identically to a current one.
  The ledger cannot catch it, because the year is part of the comparison *key*:
  two rows with different years are two keys that never meet, not a
  disagreement. Checking it needs either the dataset's own documentation or a
  staleness bound — and "how old is too old" is a policy question, not a
  verification one.
- **Geography proxying is reported, not adjudicated.** The platform holds the
  BOM's `origin` and the factor's `geography` side by side, and a mismatch is a
  legitimate, declared choice. It belongs beside `needs_expert`, so read that
  flag and the per-line `geography` rather than assuming they match.
- **Cradle-to-gate only, in practice.** Nothing in a bill of materials describes
  distribution, use or end-of-life, and packaging is not on the BOM at all. A
  `cradle_to_grave` request will be attempted, but `boundary.declared` reports
  what the retrieved factors can defend, and `indeterminate` is the honest
  verdict for a mixed set — which is the normal case when factors come from
  three databases.
- **Mixed `lca_basis` makes a total internally inconsistent** while looking
  entirely ordinary. Within-statement consistency is checkable today and is the
  agent's job to report in `boundary.declared`; whether each individual basis
  label is correct is not checkable without the dataset's documentation.
- ~~**The ledger SQL has not been exercised against a live database.**~~
  **Executed.** `bash scripts/carbon_sql_probe.sh` spins a throwaway Postgres
  cluster, applies mig-238 and runs every cross-check this agent declares —
  reading the SQL out of `src/grounding_trust.rs` rather than restating it, so
  the probe cannot drift from the contract. Results: the migration applies
  (2 CHECK constraints, both of which refuse a negative factor and an
  unrecognised `retrieval` mode); an honest reply scores 0 on all four queries;
  a reply carrying one bad product and one same-dataset "corroboration" scores
  1 on the arithmetic check and 1 on the independence check, while leaving the
  ledger untouched; and the ledger itself moves 0 → 0 → 2 mismatches over 0 → 1
  → 3 comparable pairs as a third reading disagrees with the first two. So the
  queries are sound and each fires on the rows it claims to.

  What that does **not** establish is anything about production. Whether real
  data disagrees with itself is a fact about the deployed database and nothing
  else — `scripts/grounding_contract_live.sh` is the only thing that can say,
  it needs `DATABASE_URL`, and `carbon_emission_factors` does not exist there
  until mig-238 deploys. Until two real runs resolve the same key the ledger
  check reports INERT, which is the honest reading and not a pass.
- **A parse failure is stamped `tool_no_match`.** If the reply cannot be read as
  a document, the response carries `statement.parse_failure` with the first 400
  characters, `inventory.items` is rebuilt from the BOM with every line empty,
  `coverage` is `none` and `total_kg_co2e` is `null` — but
  `inventory_provenance` reads `tool_no_match`, which normally means "asked and
  found nothing". Branch on `parse_failure` before reading the stamp.
  `parse_failure` is returned **beside** the statement rather than inside it,
  so the document still satisfies its own schema — an earlier version put it in
  the document, where `additionalProperties: false` made every parse failure
  also a schema violation, the second fault caused by the error handling rather
  than by the agent.
- **The agent's prompt is more conservative than the gate.** It warns against the
  words *divergence* and *vulnerable* on the grounds that the narrative scan is
  shared across agents. As of the current `enforce`, `NARRATIVE_LEAKS` rules are
  filtered by `agent_id`, so `regulatory_lens_translator`'s needles cannot null
  this agent's prose. The advice costs nothing and misdescribes the mechanism;
  do not build a consumer-side check on it.
