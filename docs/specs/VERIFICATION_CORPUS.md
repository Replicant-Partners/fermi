# Verification corpus — a foraging trainer that earns its own ground truth

**Date:** 2026-08-19
**Code:** `src/verification.rs` (domain layer, 17 tests). Nothing else is built.
**Reading:** `docs/specs/SOURCE_RELIABILITY.md` for why, Hodgson et al. 2023
(PMID 36794335) for the number.

---

## 1. The reframe

Hodgson measured three phone mushroom-identification apps against 78
expert-confirmed specimens sent to a poisons centre. Best accuracy **49%**,
iNaturalist **35%**, death caps misidentified by two of the three. The study
exists because its authors observed poisonings following incorrect
identification of poisonous species as edible using such apps.

For an app that tells people what is safe to eat, that figure is disqualifying,
and we spent three commits removing exactly those claims from
`forage_identify`, `harvest_advisor` and `forage_scout`.

**For an app that teaches how unreliable photographic identification is, the
figure is the curriculum.**

That is not a repositioning of the same product. It changes what the software
optimises for: not "give the right answer" but "make the learner calibrated
about how often anyone gets the right answer, and accumulate the evidence."

## 2. Why it is not just a nicer story

The reframe closes three gaps this repository had already written down as open —
and one of them was recorded as permanent.

| Recorded as open | Closed by an expert determination |
| --- | --- |
| *"No accuracy figure exists for this system"* (SOURCE_RELIABILITY §3) | Ground truth to measure against |
| *"A curated, citable lookalike source is what would make this advisory"* (three commits) | Confusion pairs, accumulated from real misses |
| `forage_identify.taxonomy` exemption: *"the check that would mean something is agreement between two independent determiners on the same frame — a capability decision, not a missing JOIN"* | Exactly that second determiner |

The third deserves attention. That exemption was written as a structural limit:
no platform record knows what a person photographed. An expert determination **is
such a record.** The limit was never structural. It was an absence of people, and
this is the mechanism that supplies them.

## 3. What is built

`src/verification.rs`, pure, no I/O:

- `Determiner` — `Model`, `Community`, `Expert`
- `Determination` — a taxon, the rank it reaches, an optional citation, notes,
  and `provenance()` mapping it onto the platform's existing vocabulary
- `VerificationRecord::resolve()` — majority taxon among settling
  determinations, carrying the strongest provenance among those that *agreed*
- `MatchLevel` / `compare_names` — exact, genus, wrong, undetermined
- `AccuracyEstimate` — Wilson 95% interval, and a `headline()` that cannot omit it
- `Corpus::report()` — queue depth, cited vs endorsed, contested count, exact and
  genus-or-better accuracy, and the confusion pairs

### It rides the existing provenance ladder

No new vocabulary. The verification lifecycle *is* the assertion layer's states:

```
model_inference        the model's guess, submitted
    ↓
pending_human_check    queued; nobody has looked yet
    ↓
human_endorsed         someone vouched, without citing anything
human_sourced          someone named what they checked it against
rejected               checked, and found wrong
```

That the state machine already existed is a good sign about the machinery rather
than a coincidence. `grounding_trust` had already worked out that a citation is
what separates a source from an opinion, and enforced it at the database level
precisely because *"a one-click 'verified' button is how a queue becomes a
laundering UI."*

## 4. The four rules the domain layer enforces

**Agreement is not corroboration.** Five uncited agreements resolve to
`human_endorsed` — the same strength as the model's guess. Only a citation
reaches `human_sourced`. Same doctrine `vote_strategist` applies to agents whose
shared model makes their agreement correlation rather than corroboration; it
applies harder to people, who read each other's answers.

**Standing is not a citation, and a citation is not standing.** An uncited expert
is an endorsement. A community member who names the key they used is
`human_sourced`. The ladder measures reproducibility, not credentials — and
inverting that is the deference `grounding_trust` exists to remove.

**A model can never settle its own score.** Scoring it against itself is
circular, so `Determiner::Model` cannot settle a record. An unsettled record is
`pending_human_check`, not `unavailable`: a person *can* answer this.

**Declining is not a miss.** A model that returns `null` is excluded from
accuracy rather than counted wrong. Scoring "I don't know" as a failure would
push it toward guessing, which is the opposite of the point.

## 5. The number, and its interval

`AccuracyEstimate` exists as a type rather than an `f64` for one reason: the
figure that started all of this is **49% with a 95% CI of [0–100]**. A point
estimate that is nearly uninformative alone, and exactly the number a summary
repeats.

So `headline()` always emits n and the bounds, and under `MIN_N_FOR_HEADLINE`
(30) it declines to state a percentage at all — showing raw counts instead. That
threshold is editorial, not statistical. There is no n at which an estimate
becomes true; under 30 the interval is wide enough that quoting a percentage
invites a reader to believe the percentage rather than the interval, and this
project has already caught one machine-generated page and one peer-reviewed
abstract inviting exactly that.

Wilson rather than normal-approximation because at small n and extreme
proportions the normal interval reports bounds outside [0,1] — impossible values,
stated confidently.

**Our interval on the same data would be narrower than Hodgson's**, because they
account for three independent raters per specimen and we do not. Recorded in the
test so nobody reads our tighter bound as an improvement on their statistics.

## 6. Deliberately not built

Named so the absence reads as a decision rather than an oversight:

- **Persistence.** No migration, no table. The shape is
  `verification_records` + `determinations` with a CHECK mirroring the
  `human_sourced`-requires-citation rule already enforced for assertions. Not
  written because another session is active in `migrations/` and a schema is the
  hardest thing to change later.
- **Endpoints.** `POST /submit`, `POST /:id/determine`, `GET /queue`. Thin over
  the domain layer once persistence exists.
- **Moderation and standing.** Who counts as an expert is a governance question.
  `credential` is free text precisely so this module does not encode a hierarchy
  nobody agreed to.
- **Notification, reputation, gamification.** The training loop wants all three
  and none belongs in a first cut.
- **Coordinates.** `locality` is free text, not a point. A precise location for a
  rare or over-collected species is a conservation risk, and this should not be
  the reason a patch gets stripped.
- **Synonym resolution.** `compare_names` reads *Agaricus chantarellus* and
  *Cantharellus cibarius* as `Wrong` when they are the same fungus. A known
  undercount; fixing it means routing both through GBIF's accepted-name view,
  which a pure function cannot do.

## 7. Wild is the App. Rabble consumes it. The code has this backwards.

The corpus forced this question, and it turns out the manifest already answered
it.

`apps/kask_wild.json` declares Wild as a standalone App: its own slug, its own
composition (`wild_forager`), its own schema (`kask-wild/1`), its own homepage,
auto-hiring seven agents — and, verbatim:

> Rabble creatures consume Wild via cross-workspace delegation — the creature
> provides spatial context; Wild provides foraging intelligence.

That is the right architecture and it is not what is wired.

| | |
| --- | --- |
| Wild's declared workspace actions | `log_observation`, `update_goal`, `annotate_location` |
| Where `identify` actually lives | `POST /api/creatures/:creature_id/forage`, `action: "identify"` |
| Signature | `Path(creature_id): Path<uuid::Uuid>` — **mandatory** |

So Wild's only photographic-identification capability is reachable **exclusively
through a Rabble creature.** The dependency arrow points the opposite way from the
one the manifest declares. Same defect class as everything else this week: a
declared design and a wired design that disagree, with nothing comparing them.

### Why the corpus makes this urgent rather than tidy

If submissions are creature-scoped, **the corpus fragments.**

`MIN_N_FOR_HEADLINE` is 30 and a useful figure wants a few hundred. Partitioned
per creature, no shard ever reaches either. Every one reports "insufficient
evidence" indefinitely while the aggregate had been answerable for months — and
that failure does not look like a bug. It looks like a quiet platform.

So `VerificationRecord` is owned by `app_slug` and carries `creature_context` as
**context, not ownership**. A creature may have been present; that is useful for
the game and does not remove the determination from the shared corpus.
`the_corpus_does_not_fragment_by_creature` asserts records with different creature
contexts — and none — are scored together.

### The refactor this implies, not done here

1. **Extract the identify logic** from `forage_handler` into a function taking
   `(photo_ref, locality, habitat)` and no creature.
2. **Add `identify` as a Wild workspace action** —
   `POST /api/workspaces/:id/actions/identify` — and add it to the App's declared
   `action_types`, which is where its absence is currently visible.
3. **Keep the creature route**, delegating to the same function and passing the
   creature as context. Rabble keeps working; the creature stops being required.
4. **Then** the corpus can accumulate from both, and the Rabble foraging module
   becomes what the manifest says it is: a caller.

Deliberately not done in this commit. It re-routes a live endpoint, and another
session is active in the tree; the corpus shape is the part that had to be right
first, because it is the part that would have been expensive to change after data
existed.

### What informs what, once this holds

```
              kask_wild (App) — owns the corpus
                       │
     ┌────────────────┼────────────────┐
     │                 │                │
  Rabble          hud_field_scout    forage_scout /
  creature        (glasses)          harvest_advisor
  (context)                          (fleet)
```

One corpus, three surfaces. The glasses agent, the Rabble creature module and the
Wild fleet all submit into it and all read the same accuracy figure, the same
cross-check, and eventually the same confusion pairs. Three copies of a corpus
would be three answers to one question, and the one that disagreed would be
whichever was nearest the writer.

## 8. What the corpus pays for, in order

1. **An accuracy figure of our own.** ~30 settled records makes the headline
   reportable; a few hundred makes it meaningful. This is the cell currently
   blank in SOURCE_RELIABILITY §2.
2. **A cross-check for `forage_identify.taxonomy`.** Once expert determinations
   exist for submitted frames, the exemption can be replaced by a real
   `cross_check_sql` comparing the model's determination against the settled one
   — the first falsifiable check on a photo-ID claim this platform has had.
3. **Confusion pairs.** `CorpusReport::confusions` accumulates
   (model said, expert said) for every miss. That is the seed of the curated
   lookalike source three agents currently return `null` for, built from
   observed confusions in the actual user population rather than a textbook list.
4. **Calibration for the learner.** Loop 5 applied to people: show the card,
   take their guess, reveal the settled answer, score them over time. The most
   useful thing a foraging trainer can teach is not species — it is how often you
   are wrong, and about what.

Note the ordering. (3) is the thing that would make the foraging agents genuinely
advisory, and it is third, because it depends on (1) and (2) existing first. A
lookalike table assembled before we can measure agreement would be another
plausible list nobody could falsify.
