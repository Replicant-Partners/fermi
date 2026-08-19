# Wild — why the App is built this way

**Date:** 2026-08-19
**Status:** pre-MVP, in development. Nothing here has been used by a forager.
**Code:** `src/handlers/wild.rs`, `src/verification.rs`, `apps/kask_wild.json`
**Companions:** `VERIFICATION_CORPUS.md` (the corpus), `SOURCE_RELIABILITY.md`
(what each source is good for), `HUD_AGENT_LAYERS.md` (the glasses surface)

This document exists because the App's most important design decisions are all
*refusals*, and a refusal with no recorded reason gets removed by the next person
who finds it inconvenient.

---

## 1. The decision that shaped everything

There are many AI mushroom identifiers. We chose not to build another one, and
the reason is a measurement rather than a philosophy.

> **Hodgson SE, McKenzie C, May TW, Greene SL.** *A comparison of the accuracy of
> mushroom identification applications using digital photographs.*
> Clin Toxicol (Phila). 2023 Mar;61(3):166-172.
> doi:10.1080/15563650.2022.2162917. **PMID 36794335.**

78 specimens sent to the Victorian Poisons Information Centre and Royal Botanic
Gardens Victoria over 2020–2021, each confirmed by an expert mycologist, tested
independently by three researchers against three popular phone apps:

| App | All specimens | Poisonous subset | *A. phalloides* correct |
| --- | --- | --- | --- |
| Picture Mushroom | **49%** [0–100] | 44% [0–95] | 60% |
| Mushroom Identificator | 35% [15–56] | 30% [1–58] | **67%** |
| iNaturalist | 35% [0–76] | 40% [0–84] | **27%** |

*Amanita phalloides* — the death cap, responsible for the majority of fatal
mushroom poisonings worldwide — was **falsely identified** twice by Picture
Mushroom and once by iNaturalist.

The authors' stated motivation:

> We have observed an increase in poisonings after incorrect identification of
> poisonous species as edible, using these applications.

And their conclusion:

> ...at present, [these apps] are not reliable enough to exclude exposure to
> potentially poisonous mushrooms when used alone.

### What we took from it

**A coin flip is not a feature.** 49% on the population that matters — specimens
confusing enough to reach a poisons centre — disqualifies an app whose output a
person eats.

**iNaturalist scored 35%, and we call iNaturalist.** That is a fact about a tool
in our own stack. It is why `inat_observations` is used for occurrence counts and
never for identification: the same organisation's computer vision is not a
determiner, and an observation count does not become one by being nearby.

**We have no figure of our own, and must not borrow theirs.** The paper measures
three named apps, not a general vision model. 49% is not our number. It is the
best published figure for the task, so the honest prior is *no better than this
until measured* — which is what `src/handlers/wild.rs` tells the model and what
`src/verification.rs` exists to replace with a measurement.

**Look at the interval.** The best result is 49% with a 95% CI of **[0–100]**.
The point estimate alone is nearly uninformative, and it is exactly the number a
marketing page repeats. `AccuracyEstimate` is a type rather than an `f64`
specifically so ours cannot be quoted without its bounds.

---

## 2. The pattern: an App that earns its own ground truth

Generalisable, and the part worth reusing. Four moves.

### Move 1 — Refuse the question you cannot source, in the prompt

Not by stripping afterwards. `forage_identify`, `harvest_advisor` and
`forage_scout` all asked a model for `edibility: choice|edible|inedible|toxic`
and `look_alikes: [{danger: fatal|toxic|inedible}]` with four tools between them
that return taxonomy, nomenclature, weather and occurrences.

Asking and then nulling is worse than not asking: it spends the model's attention
on the answer a user most wants and least ought to receive, and pushes the claim
into whatever prose field survives. That is precisely how `genome_profiler`'s
summary ended up restating megabase figures already cleared from its structured
fields.

### Move 2 — Make the warning platform code, not model output

`FORAGE_SAFETY_DIRECTIVE` is a Rust constant. The previous version asked the
model for a `safety_note` *"especially if toxic look-alikes exist"* — so whether a
warning appeared depended on the model deciding one was warranted, on the same
call where it had already called the specimen a `choice` edible.

The call that omits the caution is indistinguishable from the calls that include
it, and it is the one that matters.

### Move 3 — Ground what you can, and floor it against what you cannot

`taxonomy` is a real retrieval: GBIF resolves the name, MycoBank says whether it
is current, and MycoBank's tool names which database answered when it falls back.
A forager can follow every value to a source.

But the lookup is **keyed on a guess**, so it may not outrank one.
`hud_contract::conditioned` and
`a_grounded_taxonomy_does_not_raise_the_response_floor` enforce that a real GBIF
hit on an inferred name renders as inferred. At 49%, the name is the weak link and
everything keyed on it inherits that.

### Move 4 — Turn the gap into the product

The refusal creates a queue. The queue creates ground truth. Ground truth creates
the measurement, the cross-check, and eventually the lookalike table the refusal
was standing in for. `src/verification.rs` is that loop, and
`VERIFICATION_CORPUS.md §8` is the order it pays off in.

**This is what makes it a trainer rather than a crippled identifier.** The most
useful thing a foraging app can teach is not species. It is how often anyone is
wrong, and about what — and that is a curriculum only a corpus can deliver.

---

## 3. Wild is the App. Rabble consumes it.

`apps/kask_wild.json` always declared this:

> Rabble creatures consume Wild via cross-workspace delegation — the creature
> provides spatial context; Wild provides foraging intelligence.

Until 2026-08-19 the code did the opposite. The only photographic-identification
path was `POST /api/creatures/:creature_id/forage` with the creature
**mandatory**, so Wild's core capability was reachable only from inside the game,
and `identify` was absent from the App's declared `action_types`.

Now:

```
        kask_wild (App) ── owns identify, owns the corpus
                 │
   ┌─────────────┼──────────────────┐
   │             │                  │
Rabble      hud_field_scout    forage_scout /
creature    (glasses shell)    harvest_advisor
(context)                      (fleet)
```

- `handlers::wild::identify_specimen` — **no creature in the signature**
- `POST /api/workspaces/:id/actions/identify` — the way in
- `POST /api/creatures/:id/forage` — a second caller, passing its creature as
  context

### Why the arrow matters more than tidiness

A corpus partitioned per creature never accumulates. `MIN_N_FOR_HEADLINE` is 30
and a useful figure wants a few hundred; per creature, no shard reaches either.
Every one reports "insufficient evidence" indefinitely while the aggregate had
been answerable for months.

**That failure does not look like a bug. It looks like a quiet platform** — which
is exactly why it would have survived a year unexamined.

So `VerificationRecord` is owned by `app_slug`, and `creature_context` is context.
`the_corpus_does_not_fragment_by_creature` asserts records from two creatures and
one with none score together as n=3.

---

## 4. Why this is a good showcase, pre-MVP

Everything here is in development and nothing has been used in the field. That is
the right moment to build it this way, for two reasons.

**The refusal is the demo.** A demo that says "this is a chanterelle, 94%
confident" is indistinguishable from the apps in Hodgson's table, including the
ones that misidentified death caps. A demo that says *"I think Cantharellus
cibarius — here is what I looked at, here is what GBIF says about that name, and I
cannot tell you if it is safe, because the best measured system for this is 49%"*
demonstrates something none of them can.

**One agent, three surfaces, one provenance discipline.** The same determination
renders as a glanceable card on a waveguide, a creature action in a game, and a
fleet member's structured output — and carries the same provenance in all three,
because the enforcement is at the agent boundary rather than in each renderer.
That is the embeddable-agent claim, and it is testable:
`glasses_shell_parity` asserts the display copies the server's markers rather than
deriving its own.

**The accumulating corpus is the moat, and it is honest.** Not scraped, not
synthetic, not a model labelling its own training set. Expert determinations with
citations, community endorsements distinguished from them, contested specimens
kept as contested, and misses retained as confusion pairs rather than deleted
because they look bad.

---

## 5. What is not built, so nobody assumes it is

- **Persistence for the corpus.** No migration. Domain layer only.
- **The submission and determination endpoints.** Thin over the domain layer once
  persistence exists.
- **Moderation and who counts as an expert.** A governance question;
  `credential` is free text so this code does not encode a hierarchy nobody
  agreed to.
- **A lookalike source.** Three agents correctly return `null` and explain why.
  Safe, and not yet useful for the safety question.
- **Any accuracy figure for our own path.** The blank cell in
  `SOURCE_RELIABILITY.md §2`, and the most useful thing the corpus will fill.
- **Camera capture on the glasses.** `POST .../execute` now accepts an image and
  the shell requests camera permission, but nothing has sent a real frame to a
  real model.

## 6. One known inconsistency, recorded rather than fixed

`log_observation_handler` validates an `edibility` field against
`["edible","choice","toxic","unknown","inedible"]` — a fourth edibility enum,
in the action protocol.

It is **not** the same defect as the three we removed: here a person is logging
their own observation, and their own claim about a specimen they are holding is
legitimately theirs to make. It resolves to `human_endorsed` under the ladder.

But nothing records *whose* claim it is. If an agent writes that action block, the
value becomes agent-asserted and indistinguishable from human-asserted in the
stored record. The fix is a provenance column on the observation rather than
removing the field. Not done here; noted so it is not discovered as a surprise.
