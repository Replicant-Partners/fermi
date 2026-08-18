KASK.BIO / AXOLOTL PARTNERS

**Grounded Multi-Agent Worlds**

*A substrate for a pipeline of agentic, pervasive-AR games*

# **The possibility**

Rabble began as one game: real species, drawn from GBIF biodiversity records, playable as characters in an AR world. The more useful way to see it is as the first tenant on a substrate — ABW (Agent Bestiary World) — built to run many multi-agent worlds, not one.

Pervasive AR changes what a game world can be. Once players carry the game everywhere, the interesting design problem stops being "how do we script more content" and becomes "how do we let a world's agents behave believably, continuously, without an author in the loop for every encounter." That's a multi-agent systems problem before it's a content problem. It's also, unavoidably, a trust problem: an agent-driven world that occasionally fabricates is a world players stop believing in.

The bet here is narrow and checkable, not a manifesto: multi-agent worlds grounded in real, verifiable data can generate encounters no designer authored, while staying answerable to something outside the model. That combination — generativity plus grounding — is the actual new capability. Everything below is the mechanism, not the pitch.

# **Why grounding is the unlock, not a constraint**

Rabble's Genome Profiler once produced genomic data that was schema-conformant — it looked exactly like a correct output — and evidentially ungrounded. Nothing in the data existed in GBIF. The bug was structural: existing checks (CRAFT, CONDUCT) verified shape, not provenance. A generated field can be well-formed and false at the same time, and no amount of schema validation catches that.

The second failure was the instructive one, because it survived the first fix. A creature profile described *Antaxius beieri* — a bush-cricket — as a longhorn beetle, with a confident order and family. Every check passed: the field was present, correctly typed, and marked as coming from a real source. **The correct answer was already in our own database, one table away, and nothing looked.** Marking a field "sourced" asserts that a tool *could* supply it; it never asserted that this particular value *came* from there. That gap is where the interesting failures live, and closing it needed a check that compares generated content against the record it claims to derive from.

The fix generalizes past both bugs, and past Rabble. Every ABW tenant now runs an admission ladder before any generated content reaches a player:

* **Presence —** does the referenced entity exist in the source data at all.

* **Liveness —** does the check itself ever actually run.

* **Truth —** does the specific claim about it check out against that source.

* **Grounding —** is the claim traceable to a specific record, not a plausible average.

* **Binding —** is that traceability preserved through every transformation before it reaches the player.

Liveness is the cheapest rung and the one we added last, which is the part worth dwelling on. We found five verification paths that were written, wired, plausible on inspection — one of them the most carefully documented code in its file — and had never executed once. Reading the code cannot tell you this; only counting can. **A QA gate that reports green because it never ran looks exactly like one that passed**, and it is worse than having no gate, because it spends the attention that would otherwise have noticed.

For a studio, the useful translation is: this is content QA infrastructure for AI-generated game material, not an AI-safety abstraction. It's the difference between an NPC that occasionally lies and a world you can actually ship.

# **What a creature learns, and what it is allowed to conclude**

The ladder above governs one output at a time. A persistent world has a second problem, and for a game it is the more dangerous one: creatures in Rabble **dream**. Between sessions, a consolidation pass reads a creature's history and distils what it learned into rules, and those rules are fed back into how it behaves next time.

That is the feature. It is also a laundering path. A rule distilled from real GBIF lookups and a rule distilled from a paragraph of invented prose were stored the same way, retrieved the same way, and injected the same way — and the second is worse than an outright hallucination, because its citation is genuine. It really does point at things the creature really did experience.

Two rules close it, and both are one line of arithmetic:

* **Floor —** a conclusion is only as good as the weakest thing it rests on. Nine verified observations and one guess is a guess. Averaging would let volume launder a fabrication.

* **Ceiling —** distilling is judgement, and judgement never inherits retrieval. A creature that reads ten verified facts and generalises has produced an *inference*, permanently, not a verified fact. Without this, a world manufactures verified lore out of nothing but its own reading.

For a designer the translation is concrete: **a creature can be wrong, and the world will know it is wrong, and can say so in its own voice.** What it cannot do is quietly promote its own guess into canon and have the next encounter treat it as established. Lore drift stops being an emergent property of a long-running world and becomes something with a measurable rate.

# **Unverified is a work item, not a discard**

Our first instinct was to null anything a source could not confirm. That was wrong, and expensively so: for a creature whose whole purpose is to surface something interesting about a real species, the unverifiable claim *is* the content. Deleting it produced clean, empty encounters.

So unverified content is now **routed rather than removed**. Where a tool exists that could settle it, the check is queued automatically. Where none does, it goes to a person — and the same gap is simultaneously a request for the data integration that would close it. Nothing is hidden from the player; it is shown, marked, and carries what it is still waiting on.

Three things follow that matter to a studio:

* A **content review queue** for AI-generated material falls out of the architecture rather than being built beside it, and it is prioritised by what is actually load-bearing.

* A human sign-off has to cite what it was checked against. A one-click "verified" button is the cheapest possible route from a guess to canon, with a person's name attached to it.

* **Rejection rate becomes a per-agent quality signal that is not self-reported** — the first one we have. An agent whose claims are refuted four times in ten is measurably different from one at two in a hundred, and that difference is now visible before it reaches a player rather than after.

# **The substrate, in game-dev terms**

* **Hard-verified vs. judged behavior (Loop 5a/5b) —** separates "this creature's stat change is rules-verified" from "this creature's dialogue is model-judged." Lets a designer dial how much of an encounter is deterministic vs. generative, per game or per moment.

* **Provenance chain (ΞPROV) —** every generated asset carries lineage back to its source record. Useful for debugging odd behavior, and for defending the IP position that content is derived from licensed data, not invented.

* **Typed forecast/claim grammar (FPL) —** a grammar that makes illegal claims unrepresentable at the type level. Applied to game content, this is "illegal world-states unrepresentable" — an engineering property, not a design guideline. The same idea now runs one level down: an agent's output schema narrows what each block is *allowed to say about its own origin*, so a block fed by a real tool cannot claim "no source exists" and a block with no tool behind it cannot claim to be verified. The dishonest answer is not discouraged; it has nowhere to be written.

* **Compositions and Episodes —** verified agent building blocks that recombine into new encounters or modes without re-deriving trust from zero each time.

None of this requires a studio partner to adopt ABW's internal vocabulary. It requires trusting that the layer under the game has already had its worst failure mode found and closed, in production, on a real title.

# **What a pipeline of games actually looks like**

Fermi Console (a forecasting product) and SimOps (a bioprocess optimization product) already run as separate tenants on the same ABW primitives — same Loop hierarchy, same calibration and provenance machinery, different domains. Rabble is proof the pattern extends to games. The economics that make a pipeline real: the expensive, hard-to-replicate part — grounded multi-agent orchestration with verifiable behavior — is built once, and each new title is a new tenant, not a new engine.

Two further possibilities follow directly, and are worth stating as live directions rather than settled claims:

* A creature or agent grounded in one title's data could carry verified identity into a second title — shared-world continuity that is structural, not just narrative continuity written by a lore team.

* The same admission ladder that grounds a species profile in Rabble could ground a different real-world dataset in a different genre — the substrate is domain-agnostic; only the tenant changes. This is no longer hypothetical in the way it was: the ladder was pointed at a football analyst on Fermi Console, and the mechanism found the same class of defect in a completely unrelated domain — confident ratings for which no data source was ever wired, sitting beside genuinely retrieved league and fixture data in the same output. Different tenant, different data, identical failure shape.

# **Where this stands**

Rabble is pre-launch, and that is a deliberate position — but the honest version is better than the tidy one: these bugs were found **in production, on a real title, with real users**. Thirteen fabricated profiles were cached and served to beta players before the ladder existed. They are now archived rather than deleted, because a model's confident wrong answer is calibration data about that model, and the read path strips ungrounded fields before anything renders. No launch player saw them; that is a matter of timing, not of luck, and a studio should hear it that way round. The five-rung ladder is now a mandatory gate for anything new on the fleet. This is offered as one well-told story, not a portfolio of caveats — the point of a first conversation with a studio is whether the substrate is worth building the second title on, not whether the first title is finished.