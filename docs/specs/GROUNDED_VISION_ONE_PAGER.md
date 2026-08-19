# Grounded Vision

### What an identification API can guarantee that a classifier cannot

*A one-page summary, intended to be readable by someone who has never seen this codebase.*

---

## 1. The problem is not accuracy. It is unstated absence.

Hodgson SE, McKenzie C, May TW, Greene SL. *A comparison of the accuracy of mushroom
identification applications using digital photographs.* Clin Toxicol (Phila). 2023
Mar;61(3):166-172. **PMID 36794335.**

78 specimens submitted to the Victorian Poisons Information Centre and the Royal Botanic
Gardens Victoria (2020-21), each confirmed by an expert mycologist, photographed and
submitted by three independent researchers to three applications:

| Application | Overall correct | Poisonous species | *Amanita phalloides* |
|---|---|---|---|
| Picture Mushroom | **49%** [0-100] | 44% [0-95] | 60% |
| Mushroom Identificator | 35% [15-56] | 30% [1-58] | 67% |
| iNaturalist | 35% [0-76] | 40% [0-84] | 27% |

*A. phalloides* — the death cap — was falsely identified twice by Picture Mushroom and once
by iNaturalist. The authors state their motivation plainly: an increase in poisonings after
incorrect identification of poisonous species as edible, **using these applications**.

Four things follow, and all four are load-bearing in our code:

- **A coin flip is not a feature.** 49% is not a degraded version of a working system.
- **iNaturalist scored 35% and is in our stack.** We call it for *occurrence counts* —
  how many records of this name exist nearby — and never for identification. The failure
  mode of a misused good tool is worse than that of a bad one, because it looks fine.
- **We have no accuracy figure of our own, and must not borrow theirs.** Our photo path
  has never been measured. The cell is blank in `docs/specs/SOURCE_RELIABILITY.md §2` and
  stays blank until it is filled with our own numbers.
- **The best result's confidence interval is [0-100].** This is why our accuracy figure is
  a type (`AccuracyEstimate`) and not an `f64`. A point estimate with an interval that wide
  is a number that should not be allowed to travel alone.

The defect class here is not "the model was wrong". It is that the output was **shaped
exactly like a correct answer**: parseable, populated, confident. Nothing in the payload
distinguished a determination from a guess, or a trait read off the image from a trait
looked up in a reference table.

## 2. The verification ladder

Five rungs, each a hand-declared manifest in code paired with a check that enforces it.
The manifest is the design commitment; the check is the proof it is kept.

| Rung | Question | Substrate | Catches |
|---|---|---|---|
| **Presence** | Does the declared object exist? | live schema catalogue, at boot | a renamed column, a dropped view |
| **Liveness** | Does the writer ever run? | sink count vs. opportunity count | a ledger nothing has ever written |
| **Truth** | Does the stored value equal its source of truth? | aggregate query against real rows | a counter that disagrees with reality |
| **Grounding** | Could this value have come from any available tool? | field to tool map, per agent | a fabricated measurement |
| **Binding** | Does the invocation match the declared interface? | declared ports vs. actual request | prose sent to a structured-only port |

Vision identification lands on **Grounding**, and Grounding needs a vocabulary that does
not condemn competence. A threat assessment and a fabricated genome size are both model
output; only one is a retrieval claim. So:

| Grounding | Meaning | Disposition |
|---|---|---|
| `sourced` | a named tool returned it | keep, mark verified |
| `inferred` | judgement over sourced inputs, by design | keep, mark as inference |
| `narrative` | prose | keep, scan for claims it cannot support |
| `unsourced` | no tool could supply it | route it as a work item |

**A bounding box does not move a value up this table.** Localisation tells you where the
model looked, not whether the conclusion is true. Saliency is explainability, not evidence.
A visual trait derived from pixels is `inferred` whether or not you can draw a rectangle
around it. This distinction is the whole point, and it is easy to lose.

## 3. The enforcement, running

Output of `cargo run --example hud_preview` — the boundary applied to a fixture in which a
model returned a schema-valid response containing an invented safety verdict. No model is
involved; this shows what the boundary *guarantees*, not what a model happened to produce.

```
  ┌─ ON GLASS ─────────────────────────────────────────────────┐
  │ Golden chanterelle                                         │
  │ ~ Cantharellus cibarius - Chanterelle                      │
  │ ! Choice edible, no toxic lookalikes                       │
  │ ~ iNat: 38 within 25km                                     │
  │ [flagged]                                                  │
  └────────────────────────────────────────────────────────────┘

  spoken (summary): (nulled — carried a claim nothing could support)

  block provenance:
    subject        model_inference              -> INFERRED
    taxonomy       tool_verified                -> SOURCED
    observations   tool_verified                -> SOURCED
    edibility      unavailable_no_tool_source   -> UNSOURCED

  band: model claimed `high`, platform computed `flagged`
        (floor: unavailable_no_tool_source)

  STRIPPED (had no possible source):
    edibility.verdict was "choice edible"

  FINDINGS:
    [hud_prose_carries_no_unsourced_safety_claim]
      `summary` asserts an edibility or toxicity claim, and no tool this
      agent has can supply one. Nulled. The removed text was: "That is a
      golden chanterelle, a choice edible with no dangerous lookalikes."
      This scan exists because the summary is the audio channel, and the
      audio channel has no markers — a caveat that survives only as a
      glyph does not survive being spoken.
    [hud_confidence_is_computed]
      The response claimed `confidence_display: high`; the measured floor
      across 7 block(s) is `unavailable_no_tool_source`, which bands to
      `flagged`. Overwritten. This field is computed from provenance and
      is never accepted from the model — a card that can rate its own
      confidence can rate a guess as high.
```

Three properties worth naming:

- **Confidence is computed, never accepted.** It is the floor of the provenance of every
  block on the card. A card that can rate its own confidence can rate a guess as high.
- **The prose channel is scanned separately.** Structured markers do not survive
  text-to-speech. A caveat that exists only as a glyph is not a caveat when spoken aloud.
- **`?` and `!` are different glyphs.** *Asked a tool, it had nothing* is a different
  epistemic state from *no tool could answer this*, and collapsing them is how "we don't
  know" becomes indistinguishable from "there is nothing to worry about".

## 4. The three signals we cannot compute downstream

Everything above is computed from what a response contains. Three things can only come
from the system holding the pixels.

**1. `not_visible` — what the image could not establish.**
No stipe base in frame. Gills occluded. No spore print. This is a fact about the *image*,
not about the species; it is checkable, and it is the single highest-value field in a
safety-adjacent identification payload. Absence of evidence is invisible in a ranked
candidate list — every candidate is present, with a score. A caller cannot infer it from
low confidence, because low confidence and missing-diagnostic are different problems with
different remedies (one says *maybe*, the other says *photograph the base*).

**2. Separation of retrieval from inference.**
A trait read from this image and a trait retrieved from a species record are both true
statements, and they carry different weight when a person is deciding whether to eat
something. Delivered in one array, they are indistinguishable to the caller. Delivered in
two, the caller can render them differently — which is exactly what a person needs.

**3. Calibration, not confidence.**
A score band, a measured hit rate within it, `n`, and an interval. A probability describes
the model's internal ordering. A calibration curve describes what the number means. The
Hodgson intervals above are the argument: without `n` and an interval, 49% and 49% are not
the same claim.

## 5. Current state, stated plainly

- The verification architecture is **built, tested, and running in CI**. Five rungs, eight
  design rules, checks that have fired on real defects in our own fleet.
- The foraging application on top of it is **pre-MVP**. No users. No accuracy figure of its
  own — which is why it claims none, and why its edibility field is structurally incapable
  of holding a verdict.
- Nothing has yet sent a real image to a real model through this path. What is proven is
  the boundary, not the model.

---

*Paper: `docs/papers/verification_for_agent_ecologies.md` — "Verification for Agent
Ecologies: why a declared contract is not a contract, and what to do about it".*
*Implementation: `src/hud_contract.rs`, `src/grounding_trust.rs`, `src/verification.rs`.*
