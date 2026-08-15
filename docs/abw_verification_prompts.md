# ABW Verification Remediation — Coding Agent Prompts

Context: two related but distinct failures surfaced while cataloguing the agent ecosystem —
(1) the Genome Profiler agent fabricated ungrounded field values (genome size, karyotype,
divergence time, conservation status) with no tool to source them, and (2) the agent
ecology's I/O contracts are declared as labeled ports without actual typed/enforced schemas
("No typed schema declared — the ports above are labels, so composability with them is
asserted, not verified"). Both are the same underlying pattern: a spec asked for a real
constraint, the implementation produced something spec-*shaped* but not spec-*enforcing*,
and nothing caught it because existing checks validate structure/labels, not grounding or
enforcement.

Given the operating thesis — zero-trust, agent-speed generation, not human-speed review —
verification needs to be automated and adversarial, not discovered by chance.

Four prompts below, meant to be run roughly in this order:

1. Immediate fix for the Genome Profiler agent specifically
2. Follow-up: wire in real data sources for the fields that were fabricated
3. System-wide audit: find every other agent with the same ungrounded-field problem
4. System-wide audit: find every interface/contract that's declared but not enforced

---

## 1. Fix: Genome Profiler provenance/fabrication bug

**Task: Fix provenance/fabrication bug in the Genome Profiler agent**

**Problem:** The agent's system prompt asks for a JSON schema (`taxonomy`, `genome`,
`phylogeny`, `conservation`, `summary`) but only two tools are available
(`gbif_species_search`, `gbif_taxonomy_tree`), both of which return taxonomy data only — no
genome size, chromosome count, divergence time, sister taxa, or conservation status. The
model is filling those ungrounded fields with confident-sounding fabricated values instead
of indicating they're unsourced. This has shipped to 56+ episodes with no anomaly flagged.

**Required changes:**

**1. Rewrite the system prompt** to split fields by provenance tier explicitly. Replace the
current instruction block with:

- `taxonomy` block: must be populated ONLY from `gbif_taxonomy_tree`/`gbif_species_search`
  results. If GBIF returns no match, all fields = `null` and summary states no match found.
- `genome`, `phylogeny.sister_taxa`, `phylogeny.divergence_mya`, `conservation`: these have
  NO backing tool currently. The prompt must explicitly instruct the model to return `null`
  for each of these fields UNLESS a tool result actually supplied that data. Remove the "you
  have deep knowledge of insect taxonomy and phylogenomics" framing that invites the model
  to substitute parametric knowledge for retrieval — replace with an explicit instruction:
  "You do not have tools to verify genome, phylogeny, or conservation data for this specific
  species. Do not estimate, infer, or generate specific values for these fields under any
  circumstance. Return null and set the corresponding `_provenance` field to 'unavailable —
  no tool source'."
- Add a `_provenance` sibling key to each top-level block (`taxonomy_provenance`,
  `genome_provenance`, etc.) with allowed values: `"gbif_verified"`,
  `"unavailable_no_tool_source"`. No other values permitted.

**2. Add a schema validator** (post-generation, before returning to the app) that:
- Rejects/flags any response where a `genome`, `phylogeny`, or `conservation` field is
  non-null but its `_provenance` isn't `gbif_verified` (impossible today since no tool
  exists — so in practice this means the validator should currently force these fields to
  null and flag any attempt by the model to populate them as a CRAFT anomaly).
- Logs a CONDUCT anomaly event any time the model attempts to populate an unsupported
  field, so this becomes visible in observability instead of silently passing.

**3. Update the app UI layer** for the Genome Profile card to:
- Only render `genome`/`phylogeny`/`conservation` sections if
  `_provenance == "gbif_verified"`.
- If unavailable, either hide those sections entirely or show a clear "Not yet available —
  real genomic data coming soon" state instead of blank/omitted fields that could be
  mistaken for a loading error.

**4. Backfill:** flag all 56 existing episodes' genome/phylogeny/conservation fields as
fabricated-unverified in the data store (don't delete — tag for reprocessing once a real
tool source, e.g. NCBI or BOLD Systems API, is integrated).

**Do not** add a genome/conservation data tool as part of this fix — that's a separate
integration task. This fix's scope is: stop fabrication, make the gap visible, don't
regress the taxonomy path that already works correctly.

---

## 2. Follow-up: Integrate real genomic/conservation data sources

**Task: Integrate real genomic and conservation data sources for Genome Profiler**

**Goal:** Replace the null-provenance genome/phylogeny/conservation fields (see prior fix)
with actually-sourced data, so the Genome Profile card can show real content instead of
"not yet available."

**Candidate data sources, by field:**

- **`genome.estimated_size_mb`, `genome.chromosome_count`, `genome.ploidy`**
  → NCBI Genome / Assembly database (E-utilities API, `esearch`/`esummary` against the
  `genome` and `assembly` DBs) keyed by species binomial or GBIF taxon key crosswalk.
  Coverage caveat: only a minority of insect species have sequenced genomes — expect
  frequent no-match, especially for rare/regional species. Chromosome counts are patchier
  still; may need a secondary source (e.g. Animal Genome Size Database or published
  karyotype literature) and this may not be programmatically queryable — flag as
  manual-curation candidate if no clean API exists.

- **`phylogeny.sister_taxa`, `phylogeny.divergence_mya`**
  → GBIF's sibling-taxa data (already partially returned by `gbif_taxonomy_tree`) can
  supply sister_taxa at genus/family rank without a new integration. `divergence_mya` is
  harder — real values require a dated phylogeny (e.g. TimeTree.org has an API for
  divergence time between two taxa). Treat this as its own optional sub-feature; TimeTree
  coverage is decent at order/family level, sparse at genus/species level, so expect
  frequent null even once wired in.

- **`conservation.iucn_status`, `conservation.population_trend`**
  → IUCN Red List API (requires a free API token, rate-limited). Query by scientific name.
  Good coverage for vertebrates, notably sparse for insects — most insect species,
  especially micromoths, are simply "Not Evaluated," which is itself a valid, honest value
  to surface (distinct from null/unavailable — "Not Evaluated" is real IUCN data, not a
  gap).

- **`conservation.genetic_diversity_notes`**
  → No clean structured API for this at species level. Likely stays null/unavailable
  indefinitely unless there's a literature-mining step later. Deprioritize.

**Implementation notes:**
- Build as additional MCP tools (`ncbi_genome_search`, `timetree_divergence`,
  `iucn_red_list_lookup`) parallel to the existing `gbif_species_search`/
  `gbif_taxonomy_tree`, following the same pattern.
- Update the `_provenance` tags from the prior fix to distinguish sources:
  `"ncbi_verified"`, `"gbif_verified"`, `"iucn_verified"`, `"not_evaluated_iucn"` (a real
  value, not a gap), vs `"unavailable_no_tool_source"`.
- Given sparse coverage is the expected norm for most non-model insect species, design the
  UI card to treat partial data as the default case, not the exception — e.g. a card that
  shows taxonomy + whatever subset of genome/conservation is actually available, rather
  than an all-or-nothing render.
- Reprocess the 56 backfilled/flagged episodes once each tool is live; expect most to still
  resolve to null on genome/divergence data given real-world coverage gaps — that's correct
  behavior, not a bug.

**Sequencing suggestion:** IUCN and GBIF sibling-taxa are the cheapest wins (existing API,
decent-to-good coverage). NCBI genome and TimeTree are worth doing but budget for low
hit-rates on rare species. Treat "genetic diversity notes" as out of scope until there's a
clear source.

---

## 3. System-wide audit: ungrounded/unsourceable output fields

**Task: Audit all agents for ungrounded/unsourceable output fields**

**Context:** We found that the Genome Profiler agent's system prompt requested output
fields (genome size, chromosome count, divergence time, conservation status) that no
available tool could actually supply — the model silently filled them with plausible-
sounding fabricated values instead of returning null or flagging the gap. This passed
schema validation and anomaly detection because it was syntactically well-formed.

**Your job:** Audit every agent in the system for this same pattern. Do NOT summarize or
assess "does this seem fine" — produce a literal, traceable mapping.

**For each agent, produce:**

1. **List every field in the agent's declared output schema** (JSON schema, dataclass,
   whatever the response contract is).
2. **List every tool available to that agent** and, for each tool, what fields/data it
   actually returns (read the tool's implementation or API docs, not just its
   name/description).
3. **For each output field, mark one of:**
   - `SOURCED` — a specific tool call's return value maps directly to this field (cite
     which tool, and the exact response field).
   - `DERIVED` — computed/transformed from a `SOURCED` field (show the transformation).
   - `UNSOURCED` — no available tool returns data that maps to this field. The model can
     only populate it from parametric/training knowledge.
   - `UNCLEAR` — you can't determine from the code/prompt alone; needs human review.
4. **For every field marked `UNSOURCED`, quote the exact system prompt language that
   invites the model to fill it anyway** (e.g. "you have deep knowledge of X," or a schema
   example showing a plausible value, or just an unconditional required field with no
   fallback instruction).

**Output format:** one table per agent, columns: `field | status | source_tool_or_reasoning
| prompt_language_enabling_fabrication`. No prose summary, no "overall this agent looks
solid" — table only, per agent. If you can't complete a row with confidence, mark it
`UNCLEAR` rather than guessing.

**Do not fix anything yet.** This is audit-only. Flag findings; don't touch code.

**Trust check:** once the table is returned, spot-check 3-4 fields marked `SOURCED` at
random by verifying yourself that the cited tool actually returns that data. If those hold
up, the rest of the table is probably reliable; if even one is wrong, treat the whole audit
as `UNCLEAR` for that agent and have it redone with more care.

---

## 4. System-wide audit: declared vs. enforced contracts

**Task: Build a spec-enforcement verifier — prove contracts hold, don't trust that they're
declared**

**Context:** We found that when asked for WSDL-style typed I/O contracts on agent
interfaces, the implementation produced labeled port names (`species_data`,
`genome_summary`, etc.) without actual type schemas or runtime enforcement — the
observatory page itself flags this: "No typed schema declared — the ports above are
labels, so composability with them is asserted, not verified." This mirrors the provenance
bug above: a spec asked for a real constraint, the implementation produced something
spec-shaped but not spec-enforcing, and nothing caught it because verification checked for
the label/shape, not for actual behavior under violation.

**Operating principle for this build:** We run zero-trust, agent-speed generation — no
human reads the implementation as the trust mechanism. Verification must therefore be
automated, adversarial, and run on every agent/interface at generation or deploy time, not
discovered by chance during unrelated browsing.

**Your job:** Build a verifier that, for any agent or interface claiming a typed contract
(I/O ports, schema, WSDL-style stub, or similar), does NOT check "is the contract
declared" — it checks "does the system actually reject inputs/outputs that violate the
declared contract." Specifically:

1. **Enumerate every agent/interface in the system** that has a declared I/O contract,
   typed schema, or port specification (source: agent definitions, the observability
   catalog, system prompts, or wherever these are declared).

2. **For each declared input/output type, generate an adversarial test case** that should
   fail if the type is genuinely enforced: wrong type, missing required field, malformed
   shape, out-of-range value, or (for the provenance case) a field populated without a
   valid source tag.

3. **Run each adversarial case against the actual agent/interface** and record: did it
   reject the bad input/output, or did it silently accept it (or worse, produce a
   plausible-looking but wrong result)?

4. **Classify every declared contract as one of:**
   - `ENFORCED` — adversarial cases were correctly rejected.
   - `COSMETIC` — the contract is declared (labeled, documented, present in a
     schema-looking structure) but adversarial cases pass through unrejected.
   - `PARTIAL` — some adversarial cases caught, others not; list which.
   - `UNTESTABLE` — could not construct an adversarial case (explain why — usually means
     the contract is too vague/label-only to even attempt violation).

5. **Output format:** one row per agent/interface/port, same style as the provenance audit
   — table only, no summary prose, columns: `agent | port_or_field | declared_contract |
   adversarial_test_used | result | classification`.

6. **Do not fix anything in this pass.** This is detection-only. The goal is a complete,
   current map of which contracts in the system are real versus decorative, so we can
   prioritize which `COSMETIC` results get load-bearing fixes first.

**Follow-up (separate task, don't do yet):** once we have the map, build this adversarial
check into the CI/generation pipeline itself, so any new agent or interface is
automatically tested for `ENFORCED` vs `COSMETIC` before it's considered done — not audited
after the fact by chance.

**Trust check:** same caveat as the provenance audit — spot-check 2-3 `ENFORCED` results by
hand before trusting the rest of the table, since the tool doing the checking is subject to
the same "looks done" trap as the thing it's checking.
