# HUD agent — the four layers, and which one is built

**Status:** layer 3 implemented. Layers 1, 2 and 4-as-transport are not, and
this document exists mainly to say why the second one is not, since guessing at
it is the expensive mistake available here.

**Date:** 2026-08-17
**Code:** `src/hud_contract.rs`, `agents/curated/hud_field_scout/agent_card.json`,
`tests/hud_contract.rs`, plus `hud_field_scout` entries in
`src/grounding_trust.rs`.

---

## 1. The layers

| # | Layer | Runs on | State |
| --- | --- | --- | --- |
| 1 | Capture — voice + frame, triggered by temple shortcut or wake phrase | glasses | **not built** |
| 2 | Relay — bridges glasses I/O to the agent | phone | **not built, deliberately** |
| 3 | Agent — reasoning, typed I/O, provenance enforcement | ABW / MCP | **built** |
| 4 | Grounding | the layer-3 boundary | **built, as part of 3** |

Layer 4 is not a separate stage. Enforcement happens inside
`hud_contract::enforce` at the agent boundary, before a card is returned, and it
cannot be skipped by a caller because there is no path that returns a card
without going through it. A post-hoc grounding check is a check that ships
disabled the first time it is inconvenient.

---

## 2. Hardware: confirmed vs. researched vs. unknown

Kept as three columns rather than two, because the middle one is where the
temptation to round up lives.

### 2.1 Confirmed (vendor spec sheet, as supplied in the brief)

- 4K camera, up to 10 min video, 3 aspect ratios
- 4-mic directional array, speakers
- Waveguide display, titanium frame, 49 g
- Temple shortcuts: short press = photo, long press = video
- Battery: ~6 h music / 5 h calls / 2 h real-time translation / 45 min video
- Ships with a bundled assistant (ChatGPT/Gemini) as the default AI layer

### 2.2 Researched this session — documented, with URLs

Findings below come from a web research pass on 2026-08-17. They are **secondary
to the spec sheet and should be re-verified before any of layer 2 is written**,
because this is a fast-moving product line and one of the traps found was a
machine-generated page confidently supplying exactly the latency numbers we
wanted.

The three questions the brief flagged as unverified, answered:

**Q1 — Is there a third-party assistant/agent SDK?**
Partly, and more than the brief assumed — but at the *reasoning* layer, not the
*trigger* layer.

- Rokid documents a **Custom Agent** path via its Rizon / 灵珠 console
  (`rizon.rokid.com`, also `agent-develop.rokid.com`). A developer registers an
  SSE endpoint; Rokid POSTs to it and streams the reply back. **Input type is
  selectable between text and image**, the latter wired to a glasses-side photo
  capture. Announced 2026-02-11.
- Confidence: **high** that this path exists and accepts text + image.
  That is what makes layer 3's input contract (text + optional image) the right
  shape rather than a guess.
- Region caveat: **China-only at announcement** (Rokid AI App, not Hi Rokid). An
  international path (`aiui-global.rokid.com`) and a US Agent Store appeared
  later, 2026-08-07. Whether Custom-Agent SSE works on international units today
  is **unresolved**.
- The wake phrase and mic pipeline stay Rokid's. Confidence **high** that no
  documented API lets a third party own the temple-button trigger on this
  device. The nearest thing is `abortBroadcast()` on an ordered Android
  broadcast, which is a race, not an extension point. **Do not design around
  it.**
- A separate on-device framework (AIUI / Ink / JSAR, `js.rokid.com`,
  `github.com/jsar-project/AIUI`) does expose `Recorder`, `Camera`,
  `SpeechRecognition` to an agent *while it is running*, dispatched to by the
  assistant's intent router. So mic access exists inside your own session; a
  passive tap on the assistant's audio does not.

**Q2 — Can a third party render to the HUD?**
Yes. Confidence **high**, four documented paths: a declarative view-tree JSON
pushed from the phone (`customViewOpen`/`customViewUpdate`), a full native APK on
the glasses, the AIUI/Ink runtime, and bare-metal Android. There is no
companion-card-only restriction.

**This answer changed a layer-3 design decision.** The AIUI design system ships
`design/monochrome/design-system-green.md`, describing the target hardware as
reproducing "one luminous green channel over pure black", with full-colour
tokens marked planned and unauthored; a field report notes that assets not
pre-filtered to green render black. So **provenance cannot be signalled by
colour.** `hud_contract::Treatment` uses leading ASCII glyphs instead, and
`markers_are_ascii` is a test rather than a comment. A confidence signal encoded
as colour on a monochrome panel is a confidence signal that does not exist.

**Q3 — Glasses↔phone link latency?**
**Unknown, and the reason layer 2 is not being written.**

- Transport topology *is* documented and independently corroborated by several
  community projects: BLE GATT + classic RFCOMM as the control plane, Wi-Fi
  Direct as the bulk plane, both required before the link is considered ready.
  USB-C works for ADB but needs a separate 5-pin dev cable.
- **No latency or bandwidth figures are published.** The only derivable number
  is the documented audio stream format — 16 kHz mono 16-bit PCM = 256 kbit/s.
- ⚠️ A CSDN page offers "bandwidth 5–20 Mbps, latency 50–150 ms" and "Type-C
  <10 ms". It is **explicitly labelled AIGC — machine-generated — and cites
  nothing.** Recorded here specifically so nobody re-finds it and treats it as
  sourced. It is the exact failure mode this whole codebase is built against: a
  plausible number is indistinguishable from a measured one once it is in a
  document.

### 2.3 Still unknown — needs Rokid DevRel or a device in hand

1. Latency and throughput: BLE control round-trip, sustained Wi-Fi Direct
   throughput, `startAudioStream` first-byte latency, `customViewUpdate` render
   latency. **All four need measuring, not estimating.**
2. Whether the AI trigger is genuinely un-ownable on this device, or whether the
   assistant listens out-of-band on the co-processor.
3. The exact Rizon SSE contract: schema, real timeout (community reports
   ~30–60 s), image encoding, and whether multi-turn context is server- or
   client-held. Only reverse-engineered today.
4. Region parity for Custom Agents on international units.
5. Whether the phone-side SDK needs a commercial licence — its
   `connectBluetooth()` signature takes a licence blob, which suggests yes.

---

## 3. Why layer 2 is not scaffolded

The brief said to scaffold it last or behind a swappable interface. It is not
scaffolded at all, which is the stronger version of the same instruction, and
the reason is item 3 above.

An interface is only swappable if its *shape* is right. The unresolved questions
are not "which function name" — they are shape questions:

- If the Rizon SSE path is the transport, layer 2 is **not a phone app at all**.
  Rokid's cloud calls an HTTPS endpoint, and the "relay" is a web service that
  adapts SSE to MCP. Nothing runs on the phone that we write.
- If the on-glasses AIUI/native path is the transport, layer 2 **is** on-device
  code, and the phone may not be in the loop.
- Those two designs share almost no code. An interface abstracting over both
  would be a guess wearing an abstraction, and the abstraction would make the
  guess harder to remove later, not easier.

So the honest scaffold is this document plus a boundary (layer 3) that is
transport-agnostic because it only ever sees `(text, Option<image>)` — which is
what both candidate transports can supply.

**To unblock:** get an answer on the Rizon SSE contract and region availability,
or put a device on a desk and measure §2.3 item 1. Either resolves the shape
question; neither takes long. Writing the relay before then risks the whole
layer, not one function.

---

## 4. Provenance: how the requested vocabulary maps onto the platform's

The brief asked for `SOURCED | DERIVED | UNCLEAR | UNSOURCED`. The platform
already has a closed five-value set in `grounding_trust::PROVENANCE_VALUES`,
asserted by tests over both the Rust constants and every card's declared enums.
Two vocabularies would mean two provenance channels, and the one nothing checks
is the one a fabrication moves into — which is the documented reason
`grounding_trust` scans narrative prose at all.

So the wire vocabulary is the platform's, and the requested four are a display
alias (`hud_contract::spec_word`):

| Requested | Platform | Band | Marker |
| --- | --- | --- | --- |
| `SOURCED` | `tool_verified` | high | *(none)* |
| `DERIVED` | `platform_derived` | high | *(none)* |
| — | `model_inference` → **`INFERRED`** | medium | `~` |
| `UNCLEAR` | `tool_no_match` | low | `?` |
| `UNSOURCED` | `unavailable_no_tool_source` | flagged | `!` |

**The requested set has four values and the platform emits five.** This is the
one place the implementation declines to follow the brief as written, stated
here rather than reconciled quietly:

- Folding `model_inference` into `DERIVED` erases the distinction
  `grounding_trust` was extended to preserve — a derivation is reproducible and
  auditable, a model judgement is neither.
- Folding it into `UNSOURCED` nulls the output of every agent whose product *is*
  a judgement, which is the failure mode that would get the contract switched
  off.

Neither is acceptable, so `model_inference` keeps its own word. Note the brief's
own instruction points the same way: it says a camera's object recognition is
"DERIVED at best". Under the platform's stricter reading it is not derived
either, because two runs over one frame can disagree. It is `model_inference`,
and the fifth value is what lets the system say so.

### 4.1 Three rules the display layer adds

1. **Subject conditioning** (`hud_contract::conditioned`). Every lookup is keyed
   on a guessed subject, so none may outrank it. A real GBIF hit on a guessed
   name renders `~`, not unmarked. This is the `Antaxius beieri` defect — a
   bush-cricket profiled as a beetle with every check green — reappearing where
   no cross-check is possible, because no platform record knows what a wearer is
   pointing at.
2. **Computed confidence.** `card.confidence_display` is written from the
   measured floor and never accepted from the model, and a disagreement is
   recorded as a finding rather than silently corrected.
3. **Sticky correction** (`hud_contract::REVIEW_MARKER`). A card that had to be
   corrected stays `flagged` on every later pass. Without this, re-enforcing a
   cached card rates it *higher* than the first pass did, because the fabricated
   value is now null and indistinguishable from an honestly empty field. Same
   trap `PRE_CONTRACT_MARKER` exists for, arriving as a confidence band instead
   of as a value.

### 4.2 What does *not* count against the band

A block that is permanently unsourceable and honestly reports itself empty does
not lower the confidence band, though it always shows its `!` marker.
`edibility` is unsourceable by design for `hud_field_scout`, so counting it
would make every card `flagged` — and a band that never varies carries no
information, while making `flagged` normal would make a genuinely alarming card
indistinguishable from a routine one. See `is_declared_gap`.

---

## 5. Open finding: the card contract cannot say `derived`

`grounding_trust::Grounding` has five variants including `Derived`, and the
runtime emits `platform_derived`. `card_contract::GROUNDING_STATUSES` has four —
`sourced`, `inferred`, `narrative`, `unavailable` — and **no `derived`**. So a
card cannot declare a platform-computed field truthfully today.

`hud_field_scout` has six such fields (`capture`, `card`, the four
`*_provenance` stamps and `_hud_review`). They are declared `inferred`, with each
`why` saying plainly that the honest status is derived and that the vocabulary
lacks the word. `inferred` is the safe direction — it understates a reproducible
value rather than overstating a guess — but it is a workaround, and six
`inferred` entries that are really derived is a smell.

**This is a decision for the maintainers, not one to take unilaterally**, which
is why the workaround shipped instead of a patch. Adding `"derived"` to
`GROUNDING_STATUSES` looks purely additive: no existing card uses the string, the
set stays closed, and `the_status_vocabulary_is_closed` keeps passing since it
asserts that an *invalid* value is rejected. It would also close a real drift —
this is the same class of split-vocabulary bug that
`no_card_declares_a_provenance_value_the_runtime_cannot_emit` was written to
catch, one level up, and nothing currently checks the status vocabulary the same
way. A sibling test asserting `GROUNDING_STATUSES` covers every `Grounding`
variant would make it impossible to reintroduce.

---

## 5a. The Insecta filter, and why it was worse than a missing result

`gbif_species_search` hard-coded `highertaxonKey=216` (Insecta) into its name
search. It was written for the Rabble insect ecosystem and is shared by six
agents that all live there (`naturalist`, `species_resolver`, `swarm_host`,
`enemy_sensor`, `genome_profiler`, `prey_locator`).

The first guess was that this made the tool return nothing for a plant, so the
card would honestly read `? GBIF: no match`. **That was wrong, and the truth is
worse.** Measured against the live API on 2026-08-17:

```
q=Quercus virginiana&rank=SPECIES&highertaxonKey=216   -> 14 results
  Glyptotus cribratus LeConte, 1858        (Insecta)
  Clastoptera querci Thompson et al. 2020  (Insecta)
  Catocala delilah Strecker, 1874          (Insecta)

same query with no filter                              -> 117 results
```

The filter works. Searching for an oak inside Insecta returns **insects whose
text matches** — *Clastoptera querci* is a real leafhopper named after oaks. So
the tool returns a real, populated, correct-for-what-it-is taxonomic ladder
**about a completely different organism**, and `grounding_trust` stamps the block
`tool_verified`, because a declared tool genuinely did return content.

This is `Antaxius beieri` again — the bush-cricket profiled as a cerambycid
beetle with every check green. Subject conditioning caps the card at `~` because
the subject was guessed, which limits the damage, but it does not detect that
the retrieval is about the wrong subject. **An empty result would have been
safer than a plausible one.**

### The fix

`scope` (named) and `higher_taxon_key` (raw) are now optional arguments.
**The default is unchanged at 216**, so all six Rabble agents keep byte-identical
behaviour — asserted by
`omitting_the_scope_keeps_the_historical_insect_filter`. `hud_field_scout` passes
`scope` explicitly and its prompt refuses to search when it cannot tell which
kingdom it is looking at.

An unrecognised scope is an **error, not a fallback**. A silent fallback would
turn `scope: "plantea"` into a zero-result insect search, which the caller
records as `tool_no_match` — "GBIF has no record of this" — when the truth is
"you asked the wrong question". That is a false claim about the world
manufactured by a typo, in exactly the vocabulary this system exists to keep
precise.

Scope keys were verified against `GET /v1/species/match?name=<name>`
(`matchType: EXACT` for all but Animalia, confirmed separately via
`/v1/species/1`) and the verification is re-asserted by
`the_scope_table_matches_what_was_verified_against_gbif`, so a key edited from
memory later fails a test rather than silently returning the wrong clade.

### Considered and deliberately NOT implemented: a subject/match agreement check

The obvious defence-in-depth is to compare `subject.scientific_name` against
`taxonomy.matched_name` and downgrade the block when they disagree. It is not
implemented, because a naive version fires on correct output:

**GBIF legitimately changes the genus when it resolves a synonym.** A search for
`Agaricus chantarellus` correctly returns `Cantharellus cibarius` — different
genus, and the *right* answer. A genus-equality check would flag that as a
mismatch, and `Agaricus` appears nowhere in the returned ladder either, so
"subject genus must appear somewhere in the result" fails the same way.

A check that fires on correct output gets switched off, and the switching-off
looks like cleanup. Doing this properly needs the tool to surface
`taxonomicStatus` and `acceptedKey` so a synonym resolution is distinguishable
from a text-match coincidence — which is a tool change first, then a check.
Recorded here so the gap is known rather than assumed closed.

## 5b. `vernacularName` was reading a key that does not exist

Found while surfacing common names on the card. The extraction read:

```rust
"vernacularName": s.get("vernacularName"),   // always null
```

The `/species/search` response has **no `vernacularName` field**. It has
`vernacularNames` — plural, an array of `{vernacularName, language}`. So the tool
emitted `null` on every call since it was written, while its own description
promised "common names".

The consequence reaches further than this agent. `species_resolver`'s prompt asks
for `common_name` in its species card and says GBIF supplies it; it never did, so
that field has been filled from model memory and written to
`creatures.common_name` for every minted creature.

**Worth being precise about the harm:** those names are probably mostly right —
*Vanessa atalanta* really is the Red Admiral. The defect is that they are
unverifiable and unlabelled, presented as retrieved when they were recalled.
That is the `genome_profiler` shape, not a wrong-answer bug, and it is why
"is it accurate?" is the wrong question to ask about it.

### Selection rule, and why it is not `[0]`

Measured on the live API, 2026-08-17:

| species | first in array | frequency-ranked |
| --- | --- | --- |
| *Danaus plexippus* | `Milkweed` | **`Monarch`** (13 sources vs 4) |
| *Quercus virginiana* | `Southern Live Oak` | `Southern Live Oak` (3) |
| *Cantharellus cibarius* | `Chanterelle` | `Chanterelle` (6) |
| *Amanita phalloides* | `Death Cap` | `Death Cap` (6) |
| *Bombus terrestris* | `Buff Tailed Bumblebee` | `Buff-tailed Bumblebee` (6) |
| *Clastoptera querci* | — | `null` (GBIF lists none) |

First-in-array calls a monarch a "Milkweed". Counting how many independent
checklists list each name picked the expected answer in all five populated
cases. GBIF's `preferred` flag would be the principled route and was `None` on
every record inspected, so it is unused.

The selection is deterministic — lowercased counting key, earliest variant wins
a tie, original casing returned — which is what allows the result to be labelled
sourced rather than chosen. `null` means GBIF listed none and is an ordinary
outcome for obscure taxa, never an empty string that would render as a blank
line.

`taxonomicStatus` needed no fix; that key was correct all along and is now
surfaced as `taxonomy.taxonomic_status`. A `SYNONYM` verdict is among the more
actionable things this agent can tell a wearer, since it explains why their field
guide and the database disagree.

Both new fields are still floored against `subject`, so they render `~`. That is
correct: GBIF knows what *Quercus virginiana* is called, not that the wearer is
looking at one. The gain is that the name is no longer invented.

### Still open

The six Rabble agents are unaffected but remain **unscoped by intent rather than
by declaration** — they inherit the default. If Rabble expands beyond insects
(flowers and pollinators being the obvious direction, which would need `plantae`
alongside `insecta`), each should start passing `scope` explicitly so the default
stops carrying the meaning. `hymenoptera`, `lepidoptera` and `magnoliopsida` are
already in the table for that reason.

## 6. Follow-ups

- **`NARRATIVE_LEAKS` wants agent scoping.** It is keyed by block name alone, so
  a needle filed under `edibility` fires on every contracted agent lacking that
  block — including `prey_locator`, whose predation prose could legitimately say
  "edible". `hud_contract::SAFETY_LEAKS` is therefore module-local, duplicating a
  mechanism. Adding an optional `agent_id` to the shared table and moving the
  needles there is the right fix; it touches a table four agents depend on, so it
  is not bundled here.
- **`species_resolver` should be re-read now that `vernacularName` works.** Its
  prompt was written against a field that returned null, so whatever it currently
  does with `common_name` was shaped by the absence. It will now receive a real
  value, which is an improvement it does not know about; its species-card schema
  and any downstream `creatures.common_name` expectations deserve a look. Not
  bundled here because it is a Rabble behaviour change and wants its own review.
- **Backfilling `creatures.common_name` is now possible but not obviously
  right.** The names are there to be fetched per `gbif_key`, but overwriting rows
  a user has seen is a product decision, and the honest interim state is that
  those values have unknown provenance rather than bad values.
- **A lookalike source would change the safety design.** `edibility` is null
  because nothing supplies it, not because the field is unwelcome.
  `adaptogen_curator` already holds the nearest schema (`HERB_DRUG_INTERACTION`,
  `CONDITION_CONTRAINDICATION`). Wiring one flips `edibility_provenance` from a
  `const` to an `enum`, which is deliberately a visible schema diff.
- **A second recogniser would make the subject cross-checkable.** Today
  `hud_field_scout.taxonomy` is exempt from cross-checking because no platform
  record knows what the wearer saw. Agreement between two independent
  determiners on one frame is the check that would actually bite. That is a
  capability decision, not a missing query.
- **`card_provenance` is `platform_derived` even on a corrected card.** The
  card is genuinely computed either way; the correction shows up in the band and
  in `_hud_review`. Defensible but worth a second opinion.
- **Nothing here has run against a live model.** The harness tests the boundary
  with hand-written fixtures on purpose — a boundary test that needs a model is
  a boundary test nobody runs — but the prompt's *own* compliance (does the model
  actually leave `edibility` null?) is unmeasured. That wants eval cases, and
  the failure mode it would catch is different from the one the boundary
  catches: the boundary guarantees a fabrication is stripped, not that the model
  stops producing them.
