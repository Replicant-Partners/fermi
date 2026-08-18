KASK.BIO / AXOLOTL PARTNERS

**Grounded Multi-Agent Worlds**

*A substrate for a pipeline of agentic, pervasive-AR games*

# **The possibility**

Rabble began as one game: real species, drawn from GBIF biodiversity records, playable as characters in an AR world. The more useful way to see it is as the first tenant on a substrate — ABW (Agent Bestiary World) — built to run many multi-agent worlds, not one.

Pervasive AR changes what a game world can be. Once players carry the game everywhere, the interesting design problem stops being "how do we script more content" and becomes "how do we let a world's agents behave believably, continuously, without an author in the loop for every encounter." That's a multi-agent systems problem before it's a content problem. It's also, unavoidably, a trust problem: an agent-driven world that occasionally fabricates is a world players stop believing in.

The bet here is narrow and checkable, not a manifesto: multi-agent worlds grounded in real, verifiable data can generate encounters no designer authored, while staying answerable to something outside the model. That combination — generativity plus grounding — is the actual new capability. Everything below is the mechanism, not the pitch.

# **Why grounding is the unlock, not a constraint**

Rabble's Genome Profiler once produced genomic data that was perfectly well-formed and entirely invented. Nothing in it existed in GBIF. The bug was structural rather than careless: our checks verified that output had the right *shape*, and shape is not the same question as whether the content is real. A generated field can be flawless and false at once.

The second failure was the instructive one, because it survived the first fix. A creature profile described *Antaxius beieri* — a bush-cricket — as a longhorn beetle. Every check passed, including one that said the field came from a real source. **The correct answer was already in our own database, one table away, and nothing looked.** Saying a field is "sourced" claims that a source *could* supply it. It never claimed that this particular value actually *came* from there.

Closing that gap meant a different kind of check: not "is this well-formed" but "does this match the record it claims to come from." That generalised into a ladder every ABW tenant now runs before generated content reaches a player:

* **Presence —** does the thing being described exist in the source data at all.

* **Liveness —** does the check itself ever actually run.

* **Truth —** does the specific claim about it hold up against that source.

* **Grounding —** is the claim traceable to one specific record, rather than a plausible average.

* **Binding —** does that traceability survive every transformation between the source and the player.

Liveness is the cheapest rung and the one we added last, which is the part worth dwelling on. We found several checks that were written, wired, and had never once executed. **A gate that reports green because it never ran looks exactly like a gate that passed** — and it is worse than no gate, because it absorbs the attention that would otherwise have noticed.

For a studio, the useful translation is: this is content QA infrastructure for AI-generated game material, not an AI-safety abstraction. It's the difference between an NPC that occasionally lies and a world you can actually ship.

# **What a creature is allowed to conclude**

The ladder governs one piece of content at a time. A persistent world has a second problem, and for a game it is the more dangerous one: creatures in Rabble **dream**. Between sessions, they distil their own history into lessons, and those lessons shape how they behave next time.

That's the feature. It's also how a guess becomes canon. A lesson drawn from real biodiversity records and a lesson drawn from a paragraph the model made up were stored, recalled and acted on identically — and the invented one is *harder* to catch, because its citation is genuine. It really does point at things the creature really did experience.

Two rules close it:

* A conclusion is only as good as the weakest thing it rests on. Nine solid observations and one guess is a guess.

* A creature's own conclusions never become facts, however much real data went into them. Reasoning over verified material produces an interpretation, permanently — not a new verified record.

For a designer the payoff is concrete: **a creature can be wrong, the world knows it is wrong, and it can say so in its own voice.** What it cannot do is quietly promote its own guess into lore and have the next encounter treat it as established. Lore drift stops being an emergent property of a long-running world and becomes something with a rate you can look at.

# **Unverified is a work item, not a discard**

Our first instinct was to delete anything a source couldn't confirm. That was wrong, and expensively so: for a creature whose whole purpose is to surface something interesting about a real species, the unconfirmed claim often *is* the content. Deleting it produced clean, empty encounters.

So unverified content is now **routed rather than removed**. Where a data source exists that could settle it, the check runs automatically. Where none does, it goes to a person — and that same gap doubles as a prioritised request for the data integration that would close it. Nothing is hidden from the player; it's shown, marked, and carries what it's still waiting on.

Three consequences worth a studio's attention:

* A **content review queue** for AI-generated material falls out of the architecture instead of being built beside it, ordered by what actually matters.

* A human sign-off has to record what it was checked against. A one-click "verified" button is the cheapest possible route from a guess to canon, with someone's name attached.

* **How often an agent's claims get rejected becomes a quality measure it can't self-report.** An agent refuted four times in ten is measurably different from one refuted twice in a hundred, and that shows up before players see it rather than after.

# **What the substrate gives a game team**

Described in game terms; the internal names are incidental and no partner needs to learn them.

* **A dial between deterministic and generative, per moment.** "This creature's stat change is rules-verified" and "this creature's dialogue is model-judged" are handled as different things, so a designer chooses how much of an encounter is authored physics and how much is improvisation.

* **Every generated asset carries its lineage back to a source record.** Useful for debugging strange behaviour, and useful for the IP position: content is demonstrably derived from licensed data rather than invented.

* **Content cannot misdescribe where it came from.** Each piece of generated material has to state its own origin, and the format only offers it truthful options — material backed by a real source cannot claim there was none, and material with nothing behind it cannot claim to be verified. The dishonest answer isn't discouraged; there's nowhere to write it down.

* **Verified building blocks that recombine.** Agents and encounters that have already earned trust can be composed into new modes without re-establishing it from zero each time.

What a studio partner has to trust is not our vocabulary. It's that the layer under the game has already had its worst failure mode found and closed, in production, on a real title.

# **What a pipeline of games actually looks like**

Fermi Console (forecasting) and SimOps (bioprocess optimisation) already run as separate tenants on the same ABW foundations — same verification machinery, different domains. Rabble is proof the pattern extends to games. The economics that make a pipeline real: the expensive, hard-to-replicate part — grounded multi-agent orchestration with verifiable behaviour — is built once, and each new title is a new tenant, not a new engine.

Two further directions follow, offered as live rather than settled:

* A creature grounded in one title's data could carry verified identity into a second — shared-world continuity that is structural, not just lore written to match.

* The same ladder that grounds a species profile could ground an entirely different dataset in an entirely different genre. That is less hypothetical than it was: we pointed it at a sports-analysis agent in Fermi Console and it found the same class of defect immediately — confident ratings with no data source behind them, sitting beside genuinely retrieved data in the same output. Different product, different domain, identical failure shape.

# **Where this stands**

Rabble is pre-launch, and that is deliberate — but the honest version is better than the tidy one: these bugs were found **in production, on a real title, with real users**. Thirteen fabricated profiles were cached and served to beta players before the ladder existed. They're archived rather than deleted, because a model's confident wrong answer is useful evidence about that model, and the read path now removes ungrounded fields before anything renders. No launch player saw them — that's timing, not luck, and a studio should hear it that way round.

The ladder is now a mandatory gate for anything new across the fleet. This is offered as one well-told story rather than a portfolio of caveats: the question in a first conversation is whether the substrate is worth building the second title on, not whether the first one is finished.

---

*The engineering account — the five contracts, why each is invisible to the one below it, and the design rules that came out of getting them wrong — is written up separately in `verification_for_agent_ecologies.md`.*
