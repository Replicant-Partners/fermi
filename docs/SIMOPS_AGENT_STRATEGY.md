# SimOps Agent Strategy: Met Agent Pattern & A2A Contracts

**Status:** Design Draft  
**Date:** 2026-08-29  
**Author:** Ivan Labra  
**Covers:** `kask_simops` fleet, companion-as-met-agent, oracle v2, first typed A2A contract

---

## 1. Overview

This document defines the agent strategy for `kask_simops` (SimOps on kask.bio), focusing on:

- The **met agent** pattern: `simops_companion` as the sole user-facing orchestrator
- **A2A contract mechanics**: how the companion delegates to specialists via typed contracts
- **SCRO grounding**: ontological typing for supply chain data (IOF/SCRO 1.0 Beta)
- The **first typed contract**: `simops_companion → supply_chain_oracle`
- The **evolution path**: subsequent contracts and full-fleet typing

The immediate goal is to make the companion→oracle delegation *verifiable*: the query the companion sends and the response the oracle returns must both conform to a typed schema that the delegation hop can validate.

---

## 2. The SimOps Fleet (Current State)

| Agent | Role | Type | Called By |
|-------|------|------|-----------|
| `simops_companion` | User-facing orchestrator (met agent) | Strategist | User (direct) |
| `simops_cascade` | Mass/energy balance engine | Research | Companion |
| `simops_predictor` | OLS yield forecasting | Research | Companion |
| `simops_optimizer` | What-if solver | Research | Companion |
| `simops_narrator` | Plain-language pipeline interpreter | Research | Companion |
| `simops_advisor` | Process design interview | Research | Companion |
| `simops_dynamics_runner` | ODE time-series + viscosity | Research | Companion |
| `sidestream_miner` | Value-recovery per stage | Research | Companion |
| `comparator` | Experiment results narrator | Research | Companion |
| `supply_chain_oracle` | BoM pricing + risk assessment | Research | Companion |
| `sensor_advisor` | SOSA sensor design | Research | Companion |
| `energy_advisor` | Energy default proposals | Research | Companion |

**Current problem:** Every A2A call is untyped. The companion emits `invoke_agent` action blocks with an ad-hoc `query` field, and each specialist returns an ad-hoc JSON object. There is no schema validation at delegation hops, and the query format in the companion prompt does not match what the oracle expects.

---

## 3. Problems to Solve

### 3.1 Untyped and mismatched invocation

The companion's system prompt shows:
```json
{
  "type": "invoke_agent",
  "agent": "supply_chain_oracle",
  "query": { "bom": [...] }
}
```

The oracle's system prompt and sample queries expect:
```json
{
  "task": "resolve_bom_prices",
  "bom_items": [{"name": "...", "qty": 0.05, "unit": "kg"}]
}
```

Two different field names (`bom` vs `bom_items`), two different task strings, neither validated. This produces silent failures in production.

A second mismatch: the companion uses `"agent"` where the app schema's `invoke_member` type says `"agent_id"`.

### 3.2 Oracle domain too narrow

The oracle's system prompt explicitly covers only _"adaptogenic herbs and roots, medicinal mushrooms, microalgae biomass, botanical extracts, fermentation substrates, skincare actives and emulsifiers."_ SimOps models arbitrary physical processes — chemical synthesis, food manufacturing, energy conversion, pharmaceutical production. The oracle must cover all process supply chains.

### 3.3 No output typing on the oracle

The oracle has no `output_contract`. Its `produces` array is free-text labels (`"unit_costs"`, `"cost_ranges"`, etc.). The delegation hop cannot validate the response.

### 3.4 No formal A2A contract

There is no artifact that declares "companion calls oracle with X, oracle responds with Y." The implicit contract lives across two system prompts with no enforcement point.

---

## 4. The Met Agent Pattern

`simops_companion` is the **met agent** — the single point of contact between the user and the SimOps fleet. In this pattern:

- **Only the met agent talks to the user.** No specialist is reached directly by the user.
- **The met agent orchestrates.** It holds the full action grammar and decides which specialists to invoke and in what order.
- **The met agent owns workspace state.** It reads the context bundle and writes to it via typed actions.
- **Specialists are behind typed contracts.** Every delegation from the met agent is a typed invocation with a validated response.

SimOps is a **domain-constrained MoE** in ABW terms:

| MoE Property | SimOps Value |
|---|---|
| Output contract | `kask_simops/action_block` — six typed action types |
| Input decomposer | User message → companion's routing logic |
| Calibration signal | `sosa_observation` — predicted values vs real measurements |
| Synthesis protocol | `pipeline` — edit → fork → compare → annotate flow |

The `weather_oracle` composition is the closest existing analogy: one coordinator
(`weather_oracle`) + three specialists (`weather_ensemble_forecaster`, `weather_calibrator`,
`weather_market_analyst`), each with typed contracts. The `tests/weather_composition.rs` test
asserts coordinator blocks lift only fields that members actually declare.
SimOps should mirror this pattern.

### Companion's role declaration

The companion agent card adds:
```json
"role": "met_agent"
```
in `metadata`, and a new top-level `contracts` section:
```json
"contracts": {
  "provides": [],
  "consumes": ["simops_companion__supply_chain_oracle"]
}
```

---

## 5. A2A Contract Mechanics

### 5.1 How the invocation flows (current)

The companion emits an `invoke_agent` action block (SimOps alias for `invoke_member`):

```
__ACTION__
{
  "type": "invoke_agent",
  "agent_id": "supply_chain_oracle",
  "query": { <BomQuery> },
  "render_as": "bom_table"
}
__END_ACTION__
```

The kask client:
1. Parses the action block (using the `ACTION_RE` delimiter)
2. POSTs to `POST /api/workspaces/:id/actions/invoke_member` — audit record
3. Executes the agent via MCP `execute_agent` or workspace message
4. Renders the response via `render_as: "bom_table"`

### 5.2 What "typed" means

A typed A2A invocation has:
1. A **request schema** — the shape the companion MUST send as `query`
2. A **response schema** — the shape the specialist MUST return
3. A **contract document** — the artifact declaring both schemas plus ontological grounding

The platform validates at the delegation hop (`envelope::build` → `schema_validate::validate`): if the specialist's response does not conform to its `output_contract`, the hop is flagged `invalid`.

> **Note on enforcement gap:** As of 2026-08-29, `handlers::execution::execute_agent_handler` does not call `envelope::build`, so contracts are declared but not enforced over HTTP (Gap #4 in `docs/DESIGN_typed_output_contracts.md`). Shipping the contracts now and enforcing later is the recommended approach — a declared contract with a known enforcement gap beats an undeclared one.

### 5.3 Contract document format

A2A contracts live in `ontologies/contracts/` as JSON documents:

```json
{
  "$schema": "abw/a2a-contract/1",
  "contract_id": "<caller>__<callee>",
  "version": "...",
  "caller": "simops_companion",
  "callee": "supply_chain_oracle",
  "ontology": { "ref": "SCRO-1-beta", "uri": "..." },
  "invocation": {
    "action_type": "invoke_member",
    "simops_alias": "invoke_agent",
    "render_as": "bom_table",
    "agent_id": "supply_chain_oracle"
  },
  "request_schema": { /* JSON Schema — what companion sends as query */ },
  "response_schema": { /* JSON Schema — must match callee output_contract */ }
}
```

The `response_schema` is derived from the callee's `output_contract.schema`. They stay in sync — a future test will assert the two agree (analogous to `tests/weather_composition.rs`).

---

## 6. SCRO Ontological Grounding

The Supply Chain Reference Ontology (IOF/SCRO 1.0 Beta) provides the type vocabulary for supply chain data. Grounding the oracle's output in SCRO enables interoperability with other supply chain systems and makes the typing semantically meaningful rather than structural.

**Key SCRO concepts used:**

| SCRO Class / Property | Oracle Usage |
|---|---|
| `SupplyChainItem` | Each BOM item — has an identifier, unit, and supply chain relationship |
| `Supplier` | Source of an item; has geographic disposition |
| `SupplyChainProcess` | The process context (`process_context.process_name` in BomQuery) |
| `SupplyChainRelationship` | Tier relationship between the process and its suppliers |
| `ShipmentFulfillment` | Grounding for `long_lead_time` risk flag |

**SCRO continuants vs occurrents in SimOps:**

- BOM items (`SupplyChainItem`) are **continuants** — they persist through processes
- Fermentation stages are **occurrents** — they happen. The `stage_id` on each BOM item links a continuant to the occurrent that consumes it.

**SCRO-based risk vocabulary:**

SCRO's handling of temporal states (Allen's Interval Algebra) is listed as an outstanding issue in SCRO 1.0 Beta. Until SCRO formalizes risk representations, the oracle's risk flags extend SCRO's disposition vocabulary:

| Flag | SCRO / Domain Grounding |
|---|---|
| `single_source` | One `Supplier` in `SupplyChainRelationship` |
| `seasonal` | Harvest-window occurrent on `SupplyChainItem` |
| `quality_variable` | Batch-variance disposition of `SupplyChainItem` |
| `supply_tight` | Constrained `ShipmentFulfillment` window |
| `substitution_risk` | `SupplyChainItem` identity uncertainty |
| `geopolitical` | `Supplier` geographic disposition (trade, conflict risk) |
| `regulatory` | `SupplyChainItem` regulatory classification |
| `long_lead_time` | `ShipmentFulfillment` temporal interval > 8 weeks |

---

## 7. The First Contract: Companion → Oracle

**Contract file:** `ontologies/contracts/simops_companion__supply_chain_oracle.json`  
**Contract ID:** `simops_companion__supply_chain_oracle`  
**Version:** `1.0.0`  
**Ontology:** SCRO 1.0 Beta (IOF)

### 7.1 Request schema (`scro/bom-query/1`)

What the companion sends as the `query` field when invoking the oracle:

```json
{
  "task": "resolve_bom",
  "process_context": {
    "process_name": "Kombucha Brewing",
    "production_scale": "small_batch"
  },
  "bom_items": [
    {
      "name": "Withania somnifera",
      "qty": 0.05,
      "unit": "kg",
      "role": "substrate",
      "stage_id": "fermentation",
      "cas_number": null,
      "inci_name": null
    }
  ],
  "currency": "EUR"
}
```

**Changes from current companion prompt:**
- `task` discriminator: `"resolve_bom"` (simplified from `"resolve_bom_prices"`)
- `process_context`: new — scale and process name help the oracle target the right pricing tier
- `bom_items` items now carry `role` (SCRO input classification) and `stage_id` (SimOps link)
- `currency`: explicit, defaults to `"EUR"`

### 7.2 Response schema (`scro/bom-response/1`)

What the oracle returns — must conform to its `output_contract` (`scro/bom_response`):

```json
{
  "items": [
    {
      "name": "Withania somnifera",
      "scro_class": "BiologicalMaterial",
      "unit_cost": 18.0,
      "unit": "kg",
      "cost_low": 14.0,
      "cost_high": 28.0,
      "currency": "EUR",
      "source_region": "India",
      "notes": "Standardised 5% withanolide extract",
      "risk_flags": ["quality_variable", "seasonal"]
    }
  ],
  "risks": [
    {
      "item": "Withania somnifera",
      "flag": "quality_variable",
      "severity": "medium",
      "description": "Significant batch-to-batch variation in withanolide content; specify minimum 5% standardised extract in procurement."
    }
  ],
  "total_bom_cost": 0.90,
  "currency": "EUR",
  "oracle_note": "Ashwagandha is the highest-risk item in this BoM — quality variation is the primary concern, not price."
}
```

**New fields vs current oracle output:**
- `scro_class`: SCRO ontology class (model inference — stamped accordingly in output_contract)
- `currency` on items and summary: explicit throughout
- `source_region`: supplier geography (maps to SCRO `Supplier` geographic disposition)

---

## 8. Oracle v2 Design

### 8.1 Domain expansion

**Before:** "adaptogenic herbs and roots, medicinal mushrooms, microalgae biomass, botanical extracts, fermentation substrates, skincare actives and emulsifiers, solvents, and process consumables"

**After:** All physical process supply chains — biological/botanical materials, industrial chemicals and reagents, fermentation substrates and nutrients, pharmaceutical/nutraceutical actives, food/beverage ingredients, agricultural commodities, industrial solvents and carriers, process equipment consumables, packaging materials, utility inputs (water, gas, electricity).

The oracle covers whatever the SimOps process model contains. Process domain is inferred from `process_context`.

### 8.2 Output contract

The oracle gets a compiled `output_contract` (`scro/bom_response`), derived from `output_contract.sketch.json` via:

```bash
cargo run --bin contract-sketch -- supply_chain_oracle
```

The sketch has three blocks:
- `items`: `sourced` from `web_search`, `coverage: partial` (some ingredients have no web data)
- `risks`: `inferred` from domain knowledge over the items
- `summary`: `inferred` — arithmetic (`total_bom_cost`) + synthesis (`oracle_note`)

### 8.3 Changes summary

| Dimension | Before (v1) | After (v2) |
|---|---|---|
| Version | `1.0.0` | `2.0.0` |
| Domain | Botanical/skincare only | All process supply chains |
| Input discriminator | `resolve_bom_prices` | `resolve_bom` |
| Input shape | `{bom_items: [{name, qty, unit}]}` | `{task, process_context, bom_items: [{name, qty, unit, role, stage_id}], currency}` |
| Output: scro_class | absent | present on each item |
| Output: currency | absent (implied EUR) | explicit on items and summary |
| Output: source_region | absent | present |
| risk_flags | 5 flags | 8 flags (+ geopolitical, regulatory, long_lead_time) |
| output_contract | none | compiled from sketch |
| accepts | free-text strings | `["scro/bom-query/1"]` |
| produces | free-text strings | `["scro/bom-response/1"]` |

---

## 9. Companion v2.1 Changes

Minimal, targeted changes to `simops_companion/agent_card.json`:

1. **Version:** `2.0.0` → `2.1.0`

2. **Fix field name mismatch in system prompt:** `"agent": "supply_chain_oracle"` → `"agent_id": "supply_chain_oracle"` (aligns with `invoke_member` schema)

3. **Fix query format in system prompt:** Replace the untyped `{ "bom": [...] }` example with the typed `BomQuery` shape from the contract

4. **Mark as met agent:** Add `"role": "met_agent"` to `metadata`

5. **Add contracts section:** New top-level field `"contracts"` declaring which contracts this agent consumes

The companion's existing `output_contract` stub (`kask_simops/action_block` → `schema_endpoint`) remains. A full compilation of the companion's output_contract (covering all 6 action types) is Phase 2 work.

---

## 10. App Schema Updates (Noted — Not This PR)

The `apps/kask_simops.json` schema needs:

1. Add `agent_contracts` to the `invoke_member` action type — a map from `agent_id` to the typed query schema for that agent
2. Add top-level `a2a_contracts` listing registered contract files

This is noted as subsequent work. The contract document and agent card updates come first; the app schema update follows once the pattern is validated in practice.

---

## 11. Evolution Path

### Phase 1 (this work)
- [x] Design this strategy document
- [x] Oracle output contract sketch (`output_contract.sketch.json`)
- [x] A2A contract document (`ontologies/contracts/simops_companion__supply_chain_oracle.json`)
- [x] Oracle agent card v2 (expanded domain, typed I/O, output_contract)
- [x] Companion v2.1 (fix query format, met_agent role, contracts ref)
- [ ] Compile oracle output_contract: `cargo run --bin contract-sketch -- supply_chain_oracle`

### Phase 2 (next sprint)
- [ ] Type the cascade contract: `simops_companion → simops_cascade`
- [ ] Type the comparator contract: `simops_companion → comparator`
- [ ] Full companion output_contract compilation (all 6 action types as coordinator blocks)
- [ ] App schema: add `agent_contracts` map to `invoke_member` action type
- [ ] Test: assert `response_schema` in contract matches `output_contract.schema` in oracle card

### Phase 3 (supply chain data bridge)
- [ ] Oracle sensor bridge: bind `scro:SupplyChainItem` to live commodity price feeds
- [ ] FPL model per strategic ingredient: price volatility distributions over SCRO items
- [ ] SCRO temporal states: track shipment fulfillment intervals for `long_lead_time` detection

### Phase 4 (full-fleet typing)
- [ ] All 12 SimOps fleet agents typed
- [ ] SimOps becomes the second fully-typed composition after the weather pipeline
- [ ] `tests/simops_composition.rs`: assert companion blocks lift only fields members declare

---

## 12. Open Questions

**Q1 — Enforcement timing:** The delegation hop validator is not wired to the public execute path (Gap #4). Ship contracts now and enforce later?  
→ Yes. A declared unverified contract still forces the system prompt to ask for the right document, which is 80% of the value.

**Q2 — Oracle web_search coverage:** `web_search` is `partial` coverage — many industrial chemicals have no mid-market spot price on the public web. Should the oracle have a secondary `"inferred"` pricing block for items where search returns nothing?  
→ Yes — the output contract sketch already uses `coverage: "partial"`, which admits `unavailable_no_tool_source` as a valid provenance verdict. The oracle already handles this correctly (set `unit_cost: null`, add note). The sketch formalizes what was already implicit.

**Q3 — Process context depth:** Should `BomQuery.process_context` include cascade results (efficiencies, output quantities) so the oracle can give scaled pricing advice?  
→ Noted for Phase 2. For now, `process_name` + `production_scale` is sufficient context.

**Q4 — Multi-currency:** Should the companion infer the user's currency from `simops/process.yaml` metadata rather than defaulting to EUR?  
→ Yes — add `currency` to the process YAML schema. The contract already has `currency` as a request field; the companion just needs to populate it from workspace metadata.

**Q5 — SCRO class resolution strategy:** The oracle assigns `scro_class` from training knowledge of SCRO. This is model inference, not retrieval. The output contract sketch stamps it as `inferred` (`scro_class: "string?"` in the items block), which is correct. No SCRO lookup tool needed in Phase 1.
