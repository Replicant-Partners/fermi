# Handoff — author `carbon_accountant`, a contract-first agent for product carbon

**Status:** design, not started. Self-contained; can be executed by a parallel
session without reading the DPP work.

**Why now:** `apps/adaptogen-lab/dpp/composition.yaml` declares

```yaml
carbon_intensity:
  mode: synthetic          # <- a person typed 0.41
  value_kg_per_kg: 0.41
  scope_3_dominant: hibiscus_infusion
```

and the DPP Studio renders it. It is the third fixture found in this App —
after the synthetic regulatory rulesets and the hardcoded BOM — and it is the
one with a compliance deadline behind it: carbon disclosure is already
mandatory in the EU Battery DPP and is the expected shape for food.

**Does an agent for this exist? No.** Checked the whole curated fleet.
`energy_advisor` is the nearest and is not it: it fills *SimOps stage* energy
defaults (`power_kwh_per_input_unit`, `labor_hours_per_input_unit`) from a
free-text stage description, declares no `output_contract`, and produces
`energy_proposal_json` — a label, not a schema identity. Different input, a
different corpus and a different consumer. Extending it would produce an agent
that is bad at both jobs.

---

## 1. The one design decision that matters

**The agent must not do the arithmetic.**

A product carbon figure is three different kinds of claim wearing one number:

| part | example | who should produce it |
|---|---|---|
| the **emission factor** | "dried hibiscus, Egypt: 2.1 kg CO2e/kg" | retrieved, with a citation |
| the **calculation** | `0.02805 kg x 2.1 = 0.0589 kg CO2e` | platform code |
| the **scope attribution** | "Scope 3, category 1 — purchased goods" | the agent's judgement |

Letting the model multiply is the failure mode that makes the whole document
worthless, and it is invisible: a confidently wrong product of two plausible
numbers looks exactly like a right one, and nobody recomputes a figure that
came back formatted. It is also the easy half — a handler can do it exactly,
in Rust, for free.

So the contract has a genuine `Derived` tier, which the regulatory agent did
not:

* `factors[]` — **`Sourced`** from `web_search`. The factor value, its unit,
  its basis (cradle-to-gate vs cradle-to-grave), its geography, its year and
  its source document.
* `line_items[].kg_co2e` — **`Derived`**. Computed by the handler from
  `BOM qty x factor`. `grounding_trust`'s `DERIVATIONS` mechanism exists for
  exactly this: the platform writes the value, overwriting whatever the model
  put there, and a disagreement is a platform bug rather than the agent's.
* `line_items[].scope` and `.scope_rationale` — **`Inferred`**. Which GHG
  Protocol scope and category a line falls in is a classification over
  retrieved facts, not a retrieved fact.
* `total_kg_co2e` — **`Derived`**, summed by the handler.
* `boundary`, `exclusions` — **`Inferred`**. What the figure does *not* cover
  is the most load-bearing prose in a carbon disclosure and the agent's own
  judgement.
* `narrative` — **`Narrative`**, scanned (see §5).

A factor the corpus does not yield must produce `factor: null` and a
`line_items[].kg_co2e: null`, and the total must then be explicitly
**partial**, never a sum over the lines that happened to resolve. A total that
silently omits three ingredients is the `tool_no_match`-as-clearance failure
in a different suit.

---

## 2. Input

Accepts `adaptogen-lab/carbon-query/1`, built by the handler from documents
the user actually wrote — not a fixture:

```json
{
  "task": "calculate_carbon",
  "basis": { "serving_ml": 330, "note": "percentages read as w/v, 1 ml ~ 1 g" },
  "line_items": [
    { "item_id": "hibiscus_infusion", "name": "Hibiscus infusion",
      "qty": 28.05, "unit": "g", "origin": "Egypt / Sudan", "role": "consumable" }
  ],
  "boundary": "cradle_to_gate",
  "region": "EU"
}
```

`buildBomItems()` in `static/adaptogen-lab/index.html` already produces this
shape from `dpp/composition.yaml`, including the `qty: null` case for
`"trace"`. Reuse it rather than writing a second BOM reader.

---

## 3. The card — mistakes already paid for

Every item here cost a debugging session on `regulatory_lens_translator`.

1. **`produces` is an array of schema identities**, e.g.
   `["adaptogen-lab/carbon_statement"]`. `AgentCard::produces` is
   `Vec<String>`. A map of `{name: description}` makes the card fail to
   deserialize, and `AgentRegistry::load_from_directory` handles that by
   printing to stderr and **skipping the agent** — which is how an
   auto-hired agent came to not exist at runtime for weeks.
2. **`accepts`, `metadata.valence` and `wallet` are required** by
   `test_all_cards_satisfy_agent_contract` and
   `test_all_cards_have_card_specific_fields`. `valence` lives at
   `metadata.valence` with `AgentValence`'s four fields
   (`primary_affect`, `arousal`, `valence`, `personality_traits`). A
   top-level `valence` block is a legacy shape nothing deserializes.
3. **Declare no tool without a dispatch arm.**
   `no_curated_card_declares_a_phantom_tool` is a shrink-only ratchet. The
   real tools here are `web_search`, `read_workspace_file`,
   `write_workspace_file`. There is **no** `list_workspace_files`.
4. **Do not put the output shape in the system prompt.** Eight phrases in
   `structured_output_trigger` (`tool_executor.rs:49`) — including the bare
   string `"ONLY"` — make `ToolAwareExecutor` bypass the tool loop entirely,
   so the agent runs single-shot with **zero tools** and answers from memory
   while appearing to have searched. Build the shape in the handler and pass
   it in the query, as `claim_evaluation.rs::output_shape` does. It also keeps
   the shape beside the parser that reads it.
5. **`min_tier` is documentation.** Nothing compares against it. Model comes
   from the DB `agents.model` column via `resolve_agent_card`.

---

## 4. Enforcement

Put the contract in `FIELD_CONTRACTS` (`src/grounding_trust.rs`), not only in
a compiled card map. `enforce_from_output_contract` gives `FIELD_CONTRACTS`
precedence, and the card-map path **skips `NARRATIVE_LEAKS` entirely**
(documented TODO at `grounding_trust.rs:3916`).

Each `Sourced` field needs a `cross_check_sql` or an entry in
`CROSS_CHECK_EXEMPTIONS` with a real reason —
`every_sourced_field_is_verifiable_or_admits_it_is_not` enforces it.

**Carbon has a real cross-check the regulatory agent did not**, and it should
be written rather than exempted: `line_items[].kg_co2e` must equal
`qty x factor` to within rounding. That is internal consistency, needs no
external corpus, and catches the single failure this design exists to prevent.
Write it as a `DERIVATIONS` entry so the platform computes it, and the check
becomes structural.

The factor *values* do need an exemption: the platform holds no copy of
Ecoinvent, DEFRA, Agribalyse or IPCC. Say so, and name the route out
(URL replay, then a factor cache keyed by `(material, geography, year)` —
which is worth building, since factors are reused across products and are the
expensive thing to retrieve).

---

## 5. Narrative leaks

`NARRATIVE_LEAKS` is now agent-scoped `(agent, block, rule)`. Add needles so
prose cannot name an authority whose evidence block came back empty:
`"ecoinvent"`, `"defra"`, `"agribalyse"`, `"ipcc"`, `"ghg protocol"`,
`"scope 3"`, and `LeakRule::Quantity("kg co2e")` so a number with a unit
cannot appear in prose when no factor was retrieved.

Add the agent to the ratchet properly — do **not** extend `UNPOLICED_PROSE`
in `narrative_leak_coverage_only_shrinks`.

---

## 6. The action endpoint

`POST /api/workspaces/:id/actions/calculate_carbon`, modelled on
`src/handlers/workspace/claim_evaluation.rs`, which is the worked example for
all of this:

* reads `dpp/composition.yaml`; **refuses** if there is no BOM rather than
  costing a fixture
* refuses with 503 if the search credential is unreachable — otherwise
  `web_search` returns its own error text as a tool *result*, the model
  answers from memory, and the output is stamped `tool_no_match`, which reads
  as "searched, found nothing". See
  `claim_evaluation.rs::resolve_search_credential`
* runs the agent via `dispatch_rabble_action` (no hire required)
* **computes every product and the total in Rust**, per §1
* runs `grounding_trust::enforce`
* writes the enforced result to `dpp/carbon/statement.yaml` with per-factor
  provenance, so it is re-readable without re-running
* returns `duration_ms`; leaves `cost.credits_charged` null and points at
  `GET /api/workspaces/:id/budget` — the charge lands in a background task
  after the response, so no honest figure exists at that moment
* add the action type to `workspace_action_log`'s constraint in a new
  migration, and register it in `run_migrations` **in the same commit**

Then add it to `apps/adaptogen-lab/regulatory-lens/manifest.json`
`schema_json.action_types`, and `carbon_accountant` to
`workspace_template.auto_hire` — `tests/app_manifest_conformance.rs` checks
both, including that the endpoint is actually routed.

---

## 7. Replacing the fixture

Once it lands, `carbon_intensity.mode: synthetic` in `composition.yaml` must
go. Not by deleting the block — by making `mode` mean something:
`synthetic | agent_calculated | supplier_declared`, with
`agent_calculated` carrying a pointer to the statement and its provenance.
An operator must be able to tell a retrieved factor from a typed one, which is
the whole argument of this App.

The Supply Chain tab already says the right thing — *"When the Supply Chain
Oracle runs and a BOM is priced, Scope 3 can be estimated from ingredient
origins and transport distances"* — and origins are already on the BOM
(`Egypt / Sudan`, `Sri Lanka`, `Brazil / Australia`). The data is there; only
the agent is missing.

---

## 8. Acceptance

1. A carbon statement for the seeded product where **every** `kg_co2e` is
   reproducible by hand from a cited factor and the BOM quantity.
2. An ingredient with no retrievable factor yields `null`, and the total is
   marked partial and names what is missing.
3. `mode: synthetic` no longer appears for a calculated product.
4. The derived-value cross-check can go red: change a factor without changing
   the product and the gate catches it.
5. Prose naming Ecoinvent with no retrieved factor is nulled as a leak.
6. `cargo test --test app_manifest_conformance` green with the new action.

## 9. Context

- worked example: `src/handlers/workspace/claim_evaluation.rs`, its
  `FIELD_CONTRACTS` block in `src/grounding_trust.rs`, commit `b8dffc71`
- credential gap that will bite: `docs/plans/HANDOFF_TOOL_CREDENTIALS_FOR_PLATFORM_AGENTS.md`
- authoring rules: `docs/guides/AGENT_CONTRACT_AUTHORING.md`, and the
  contract-compiler guidance in `agents/curated/xaman_ek/agent_card.json`
- `cargo run --bin contract-sketch -- carbon_accountant` compiles the sketch
