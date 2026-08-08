# 30 — Agent taxonomy: reform and retrofit

**Status:** proposed · tooling shipped (`scripts/taxonomy.py`)
**Surfaces:** the Ecology lens (`/ecology`) groups its register by these ranks

---

## 1. What was there

47 of 96 curated cards carried a seven-rank Linnaean taxonomy in
`metadata.taxonomy`. No surface read it, so nothing kept it honest, and it
drifted. Measured:

| rank | values | singletons | mean group |
|---|---:|---:|---:|
| kingdom | 9 | 5 | 5.2 |
| phylum | 19 | 15 | 2.5 |
| class | 22 | 18 | 2.1 |
| order | 37 | **31** | **1.3** |
| family | 18 | 15 | 2.6 |
| genus | 35 | **30** | **1.3** |

Three distinct problems, not one:

**Ranks that don't group.** `order` and `genus` averaged 1.3 members per
value. A rank where almost every value has one member is not a
classification; it is a second name for the agent. Two of seven ranks were
doing no work.

**No naming convention.** `phylum` mixed four incompatible suffixes
(`-vora` ×24, `-ales` ×12, `-ria` ×6, `-ia` ×4). `class` mixed three.
`genus` mixed Latin (`Analyticus`) with plain English (`Oracle`, `Scout`,
`Scanner`, `Composer`, `Modeler`). Only `family` was consistent — 100%
`-idae`, which is correct zoological practice.

**Ranks that contradicted the card.** `class` tracked nothing observable:
`agent_type=research` alone spanned 11 classes, and `Processoria` appeared
under both `research` and `strategist`. The taxonomy could disagree with
the agent it described and nothing noticed.

Five cards also had `species ≠ agent_id`, e.g. `ar_avatar_renderer` with
species `ar_renderer`.

## 2. Why filling the 49 gaps was the wrong move

The obvious retrofit — classify the 49 unclassified cards — would have
added roughly 49 more singleton orders and genera, entrenching the ranks
that already grouped nothing. The scheme needed reform first. Hence
reform-then-retrofit, in that order.

## 3. The scheme

Seven ranks, split by who assigns them:

| rank | kind | basis | convention |
|---|---|---|---|
| kingdom | **editorial** | domain of competence | ends `-a` |
| phylum | **derived** | mode of operation | ends `-a` |
| class | **derived** | `agent_type` | ends `-ia` |
| order | **derived** | output modality | ends `-ales` |
| family | **editorial** | authoring lineage | ends `-idae` |
| genus | **editorial** | role archetype | ends `-us`/`-or`/`-is`/`-ix` |
| species | **derived** | `agent_id` | verbatim |

**Derived ranks are computed from the card and enforced.** If a card's
stated `class` disagrees with its `agent_type`, `taxonomy.py audit` fails.
That makes four of seven ranks free, correct by construction, and
impossible to drift.

**Editorial ranks require a human**, drawn from
`agents/taxonomy_vocab.json`. A name is a claim about kinship, and a
generator inventing them would only be guessing convincingly. The
controlled vocabulary is what stops the singleton explosion recurring: if
a proposed term would have exactly one member, reuse an existing one.

### 3.1 Derived: phylum — mode of operation

Three values, so it actually partitions:

- `Composita` — declares required dependencies; orchestrates other agents
- `Instrumenta` — reaches for external instruments (MCP servers, tools, skills)
- `Solitaria` — works from its own prompt alone

### 3.2 Derived: class — `agent_type`

One class per `agent_type`, e.g. `research → Researchia`,
`creative → Creativia`, `meta → Metaria`, `observability → Vigilia`.
Verifiable, and gives the rank ~12 well-populated values.

### 3.3 Derived: order — output modality

**Not an enumeration of `produces`.** That field is free text: 267 distinct
values across 321 declarations, 234 of them singletons. This is almost
certainly the original defect's origin — `order` inherited the singleton
pathology from the field it was derived from. Enumerating it would break on
every new card.

Instead `order` buckets outputs by *kind*, by pattern, so it survives new
vocabulary: `Prognosticales`, `Diagnosticales`, `Consiliales`,
`Imaginales`, `Narrativales`, `Operationales`, `Evidentiales`. Patterns are
tried most-specific first; `Evidentiales` is the catch-all and goes last.

Coverage on the unclassified 49 went from 12/50 (enumeration) to 46/50
(patterns), with a mean group of ~6.6. Cards matching nothing are left
unset and reported, rather than silently defaulted.

## 4. The retrofit process

```sh
scripts/taxonomy.py audit                 # conformance report; exit 1 on error
scripts/taxonomy.py propose               # -> agents/taxonomy_proposals.json
#   ... a human fills editorial_TODO from agents/taxonomy_vocab.json ...
scripts/taxonomy.py apply --from agents/taxonomy_proposals.json
scripts/taxonomy.py apply --derived       # refresh derived ranks only; idempotent
```

`apply --derived` is safe to run repeatedly and never invents an editorial
name, so it can be wired into a pre-commit hook or run after any bulk card
edit.

### 4.1 Order of operations

1. **Fix the five `species ≠ agent_id` cards.** Pure data bug.
2. **`apply --derived` across all 96.** Mechanical, verifiable, reversible.
   Corrects the 140 audited errors that stem from derived ranks
   contradicting their cards.
3. **`propose`, then review the 50 editorial gaps in batches.** Best done
   by vertical — the `simops_*`, `ar_*`, `rabble_*`, `efra_*` clusters map
   cleanly onto families, so a batch is a coherent editorial sitting rather
   than 50 unrelated naming decisions.
4. **Wire `audit` into CI** so new cards cannot land unclassified or
   inconsistent.

### 4.2 A caution learned the hard way

`json.dump` defaults to `ensure_ascii=True`, which escapes every non-ASCII
character. These cards are full of em-dashes and arrows in prompts and
domain knowledge, so the default turns them into `\u2014` and `\u2192`. One
card produced a 31-line diff of pure escaping noise; across 96 cards it
would have rewritten thousands of lines nobody asked to change and buried
the actual taxonomy edit. `apply` sets `ensure_ascii=False`, which makes
the rewrite a true round-trip.

## 5. Acceptance criteria

1. `scripts/taxonomy.py audit` exits 0 on the full card corpus.
2. Every card has all seven ranks; `species == agent_id` everywhere.
3. No rank has a mean group size below 1.6 — the threshold that flags a
   rank as decorative. `audit` reports this per rank and marks offenders.
4. Every editorial term appears in `agents/taxonomy_vocab.json`.
5. No card's derived rank contradicts its own structure.
6. CI runs `audit`, so a new card cannot reintroduce the drift.

## 6. Not in scope

**Auto-naming editorial ranks.** Deliberately excluded. Kingdom, family and
genus are claims about kinship and lineage; a generator would produce
plausible names with no basis, which is worse than a visible gap. The
Ecology register shows unclassified specimens under `Incertae sedis` — a
real naturalist's bucket for the undescribed — so the gap is honest and
visible rather than papered over.

**Taxonomy for DB-native agents.** `metadata.taxonomy` lives in the on-disk
`agent_card.json`. Agents created through the UI have no card on disk and
so cannot be classified by this tooling. If third-party agents should be
classifiable, taxonomy needs a home in the `agents` table — a schema
question this spec does not settle.
