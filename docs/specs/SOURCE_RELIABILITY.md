# Which source to trust, and for what

**Date:** 2026-08-19
**Prompted by:** a week in which every source we touched turned out to be wrong
about something, and by Hodgson et al. 2023 (PMID 36794335), which measures the
one step this platform labels a judgement.

The question that prompted this was *"I'm not sure which source or API to
trust."* That is the correct reaction to the evidence, and the answer is not a
ranking.

---

## 1. "Trust" is the wrong unit

`tool_verified` has never meant *this is true*. It means **you can go and check,
and here is where**. That is a weaker claim than it looks, and it is the only one
a platform can actually make.

The distinction matters because every failure this week was a source being
*right about one thing and read as authoritative about another*:

| What happened | The source was fine. The reading was not. |
| --- | --- |
| GBIF returned *Clastoptera querci*, a leafhopper, for `Quercus virginiana` | GBIF answered the question asked: "insects matching this text". The filter was ours. |
| `vernacularName` was `null` on every call since the tool was written | GBIF has the data under `vernacularNames`. We read a key that does not exist. |
| MycoBank answers without a MycoBank key | It falls back to GBIF and **says so** in its own `source` field. Exemplary behaviour; we simply had to pass it through. |
| `mcp-configs/` | A string in a marketing illustration. Never an API. |
| Glasses latency "50-150 ms" | A machine-generated page citing nothing. |
| Three agents rated `edibility` | Four tools between them, none returning edibility. |

Not one of those is a database lying. Every one is a claim of a *kind* the source
never made. So the discipline is not "find trustworthy sources", it is **hold each
source to the one question it answers**.

---

## 2. What each source we use is actually good for

| Source | Answers reliably | Does **not** answer | Verified how |
| --- | --- | --- | --- |
| **GBIF** `/species/search` | name → rank ladder, accepted/synonym status, usage key, vernacular names | specimen → name. Nothing about a physical thing. | Queried live 2026-08-17; keys and ladders confirmed against `/species/match` |
| **GBIF** scope filter | descendants of a given backbone key | anything, if the key is wrong — it returns confident results for the wrong clade rather than none | Measured: `Quercus virginiana` under key 216 returns 14 insects |
| **MycoBank** (via our tool) | fungal name status, current vs synonym, **and which database answered** | edibility, toxicity, lookalikes | Read the implementation; the GBIF fallback is labelled in `source` |
| **iNaturalist** occurrences | how often a taxon has been recorded near a coordinate recently | identification. **35% accurate at photo ID** | Hodgson 2023 |
| **NCBI Assembly** | genome size, assembled chromosome count, which assembly supplied them | ploidy — `assemblytype` describes the assembly, not the organism | `grounding_trust` docs; the trap is documented there |
| **A vision model** | what features are visible in a frame, and a candidate name | whether the name is right. Best measured comparable system: **49%** | Hodgson 2023 |
| **Open-Meteo / ERA5** | station observations and ensemble members | the settlement rules of a market that names a different station | `weather_oracle` exists for this |

The two rows that matter most are the last two of the biological ones, because
they are the ones a person acts on.

---

## 3. The measurement that anchors all of this

Hodgson SE, McKenzie C, May TW, Greene SL. *A comparison of the accuracy of
mushroom identification applications using digital photographs.* Clin Toxicol
(Phila). 2023 Mar;61(3):166-172. PMID 36794335.

78 specimens sent to the Victorian Poisons Information Centre and Royal Botanic
Gardens Victoria over 2020-2021, each confirmed by an expert mycologist, run
through three popular phone apps by three independent researchers.

| App | Overall | Poisonous subset | *A. phalloides* |
| --- | --- | --- | --- |
| Picture Mushroom | 49% [0-100] | 44% [0-95] | 60% |
| Mushroom Identificator | 35% [15-56] | 30% [1-58] | 67% |
| iNaturalist | 35% [0-76] | 40% [0-84] | 27% |

*A. phalloides* was **falsely identified** twice by Picture Mushroom and once by
iNaturalist. The stated motivation for the study is an observed increase in
poisonings following incorrect identification of poisonous species as edible
using these applications.

### Three things this changes

**The safety directive is now cited, not asserted.**
`FORAGE_SAFETY_DIRECTIVE` said photographs cannot exclude lethal lookalikes
because spore print and cut-flesh reaction do not survive a photo. True, and now
unnecessary as an argument: there is a number.

**We still have no accuracy figure of our own, and must not borrow theirs.**
The paper measures three specific apps, not a general vision model. 49% is not
this platform's number. What it is, is the best published figure on the
population that matters — specimens confusing enough to reach a poisons centre —
so the honest prior is *no better than this until measured*. Quoting 49% as ours
would be the fabrication this whole line of work exists to prevent, arriving as a
citation.

**Note the confidence intervals.** The best app is 49% with a CI of [0-100]. The
point estimate is nearly uninformative on its own, and it is the number a
marketing page would quote. That is the same defect as the machine-generated
latency figures, in a peer-reviewed journal, stated honestly by the authors.
A number is not evidence; a number with its interval is.

---

## 4. So what do you do

**Ask what question the source answers, then only ask it that.** GBIF resolves
names. It does not see. iNaturalist counts observations. It does not determine.
A vision model reads features. It does not know.

**Prefer a source that reports its own degradation.** MycoBank's tool falling
back to GBIF and naming the fallback in `source` is the single best-behaved thing
we integrated this week. It is more useful than a more accurate source that
answers silently, because you can tell which one replied.

**Treat an unresolvable answer as information.** `tool_no_match` on a
confident-sounding binomial usually means the epithet was invented. That is the
check working, and for a forager it is more useful than a resolution.

**Measure, or say you have not.** Every claim in the table above has a "verified
how" column, and where the answer is "not measured" that is written down rather
than left blank. The platform's own accuracy on photo identification belongs in
that column and is currently absent.

**Never let a lookup on a guess inherit the lookup's confidence.** This is
`hud_contract::conditioned` and it is the load-bearing rule: GBIF's ladder for
*Amanita phalloides* is a real retrieval about a name, and says nothing about
what is in the wearer's hand. The paper is why — at 49%, the name is the weak
link, and everything keyed on it inherits that.

---

## 5. Open

- **No accuracy measurement for our own photo path.** The obvious study is
  cheap: run `forage_identify` over a set of expert-confirmed specimens and
  report the number with its interval. Until then the prompts say no figure
  exists, which is honest and unsatisfying.
- **A curated, citable lookalike source** remains the thing that would make any
  of the foraging agents advisory rather than merely honest.
  `adaptogen_curator` proves the pattern works with `HERB_DRUG_INTERACTION` and
  `CONDITION_CONTRAINDICATION` against a real database.
- **This document is a snapshot.** Every figure in it was checked on the date at
  the top. `the_scope_table_matches_what_was_verified_against_gbif` is the model
  for keeping such things honest: a test that fails when a recorded fact drifts
  from the thing it records.
