//! What must an agent declare before this substrate can say anything about it?
//!
//! # The finding this module exists for
//!
//! Every trust surface built so far reports `unknown` far more often than it
//! reports anything else, and the reason had never been separated from the other
//! reasons. Measured over production:
//!
//! | | agents that have produced an episode | ports | output type | checkable schema | field contract |
//! |---|---|---|---|---|---|
//! | real | **96** | 93 | 10 | **2** | **7** |
//! | `test_agent_*` | **110** | 0 | 0 | 0 | 0 |
//!
//! So `unknown` is overwhelmingly **not** a stalled loop, a cold counter, or a
//! contract nobody has written. It is *the subject declaring no structure to
//! check against*. 3,571 of 3,576 episodes carry no grounding stamp because 89 of
//! 96 real agents have no field contract, and 110 further agents are test rows
//! that declare nothing and never will.
//!
//! # Why that distinction has to be in the type system
//!
//! `panel_absence::Resolver` already has five ways to explain an absence:
//! `Liveness`, `LoopStage`, `Gate`, `GateLedger`, and `Unresolved { why }`. None
//! of them is *the subject declared nothing*, and `Unresolved` is the one it
//! collapses into by default — which is wrong in a specific and expensive way:
//!
//! > `Unresolved` is a work item for **us**. `Undeclared` is a work item for the
//! > **agent's author**.
//!
//! Rendering an undeclared agent as `Unresolved` blames the platform for the
//! agent's silence, and it makes the retrofit worklist invisible — the platform
//! looks like it has 89 missing contracts to write when what it has is 89 agents
//! that have not declared themselves. Those have different owners, different
//! costs, and different exit conditions.
//!
//! # And why the two worklists are separated
//!
//! 110 of 206 is cruft. Pruning a `test_agent_<uuid>` row is a delete behind a
//! safety gate; retrofitting `weather_oracle` is authoring work with a domain
//! expert. Reporting them as one number makes the retrofit look twice its actual
//! size, and the actual size is the thing that decides whether it is worth doing.
//! [`Disposition`] keeps them apart.
//!
//! # No target, and no ratchet on coverage
//!
//! Coverage is **reported, never asserted against a figure.** The house rule is
//! that a threshold must be a measurement or a two-way ratchet and never a
//! target, and here even a ratchet would be wrong: new agents arrive undeclared
//! by definition, so a ratchet on the count would fire on entirely correct
//! behaviour, and §5.2 says what happens to a check that cries wolf.
//!
//! The one thing that *is* safely ratcheted is [`crate::grounding_trust
//! ::FIELD_CONTRACTS`], because removing a contract is unambiguously a
//! regression and the list is a hand-maintained const. That is pinned in this
//! module's tests, not here.
//!
//! # Two measurements of typed output, and they are not duplicates
//!
//! `workflows::agent_contract::TYPED_TIER_EXEMPT` is a shrink-only ratchet over
//! **curated agents at publish time** — how many are still grandfathered out of
//! needing a typed contract. The `output_schema` rung here measures **agents that
//! have produced an episode, at trace time** — how many of the outputs anyone has
//! actually received could have been checked.
//!
//! Different populations (101 curated against 96 real producing), different
//! moments, and the pair is worth having: theirs is the *supply* of typed
//! contracts and this is the *coverage* observed in the fleet. They will
//! disagree, and the gap between them is the interesting number — an agent that
//! is exempt and never runs costs nothing, and one that runs constantly and is
//! exempt is where the missing checks actually bite.
//!
//! Stated here because two lists measuring adjacent things is exactly how a
//! third gets invented. If a third arrives, it should be because someone argued
//! for it against this paragraph.

use crate::grounding_trust;

/// Is this row test cruft rather than a real agent?
///
/// The canonical definition. Lived in `handlers::mod` until the ladder needed it
/// — the handlers are binary code and this is library code, so the choice was one
/// definition here or two definitions. `handlers::is_test_cruft` re-exports this.
///
/// Integration tests have been inserting `test_agent_<uuid>` rows into the shared
/// database for a long time (v0.10.20's audit found 565; 110 of them have
/// produced episodes). Deliberately a prefix match and not a regex: the rows are
/// named by our own test harness, so the shape is known rather than guessed, and
/// a looser pattern would eventually hide a real agent someone named badly.
pub fn is_test_cruft(agent_name: &str) -> bool {
    agent_name.starts_with("test_agent_")
}

// ── the ladder ───────────────────────────────────────────────────────────

/// One thing an agent may declare, and what declaring it makes possible.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Declaration {
    /// Stable token. The key a worklist groups by.
    pub rung: &'static str,
    /// What the agent is declaring, in a sentence an author would recognise.
    pub declares: &'static str,
    /// Where the declaration lives, so a retrofit knows what file to open.
    pub owner: &'static str,
    /// **Which substrate capability this unlocks.**
    ///
    /// Required, and it is the field that makes the ladder an argument rather
    /// than a checklist. A rung that unlocks nothing should not be on the ladder,
    /// and `fermi_contract` is the case that tested this: 15 of 96 agents carry
    /// one, which would have made coverage look twice as good, and it is domain
    /// configuration for forecast agents (`finding_labels`, `multiplier_range`)
    /// that no trust surface can read. Counting it would have inflated the number
    /// with something the trace cannot use.
    pub unlocks: &'static str,
    /// **What reads `unknown` without it, and why that is the agent's silence.**
    ///
    /// Required. This is the sentence a surface shows in place of a blank, and it
    /// has to name the consequence rather than the absence — "no field contract"
    /// is a fact about a const table; "nothing can say whether this agent
    /// fabricated the value" is the finding.
    pub without_it: &'static str,
}

/// Everything an agent may declare, weakest rung first.
///
/// Ordered by what it costs an author, not by importance, because that is the
/// order a retrofit will actually proceed in: ports are a line in a card, a
/// checkable schema is a design session, and a field contract needs someone who
/// knows which tool could have supplied each field.
pub const LADDER: &[Declaration] = &[
    Declaration {
        rung: "ports",
        declares: "What this agent accepts and what it produces, as labels.",
        owner: "agents.accepts / agents.produces, from the card's `accepts` / `produces`",
        unlocks: "`port_trust::bind_input` at every execute boundary, and the seam \
                  census — which of the platform's port labels could ever connect \
                  to another agent's. Measured: 289 distinct `produces` labels, \
                  236 `accepts`, and 13 that appear on both.",
        without_it: "The input-binding gate returns `undetermined` for every call, \
                     so nobody can tell free text sent to a structured port from a \
                     correct invocation. The agent also cannot appear on either \
                     end of a seam, because there is no label to match.",
    },
    Declaration {
        rung: "output_type",
        declares: "The name of the type it produces (`output_contract.produces_schema`).",
        owner: "agents.output_contract.produces_schema",
        unlocks: "`agent_backend::envelope::declared_type`, so an agent receiving \
                  a delegated result knows what it was handed rather than \
                  inferring it from the shape.",
        without_it: "A delegated consumer receives an untyped blob. It cannot \
                     refuse a payload of the wrong kind, so a substitution at the \
                     hop is undetectable — the failure `port_trust` exists for, on \
                     the agent-to-agent path instead of the human one.",
    },
    Declaration {
        rung: "output_schema",
        declares: "A checkable JSON Schema for that type (`output_contract.schema`).",
        owner: "agents.output_contract.schema — authored via `contract_sketch`, \
                see docs/DESIGN_typed_output_contracts.md",
        unlocks: "`schema_validate::validate` at the delegation hop, which is what \
                  makes a checkpoint's `verified` mean *a schema resolved on both sides* \
                  rather than *the labels matched*. Also the wrapped output type \
                  `{value, provenance, verified}`, once declarable.",
        without_it: "The envelope reports `unverified` — and that is explicitly \
                     NOT a pass. A document that contradicts its own declared type \
                     and a document nobody checked produce the same word \
                     everywhere downstream. Measured: 2 of 96 real producing \
                     agents clear this rung. This is the rung with the cheapest \
                     remedy and the worst coverage, which is not a coincidence — \
                     `contract_sketch` exists because the contract was never \
                     disputed, only unaffordable: six authored decisions expand to \
                     thirty-five artefacts, and an author who writes that six \
                     times copies the nearest neighbour.",
    },
    Declaration {
        rung: "field_contract",
        declares: "For each output field, which tool could have supplied it — or \
                   that none could, or that it is a judgement the agent is asked \
                   to make.",
        owner: "grounding_trust::FIELD_CONTRACTS (a Rust const, so third parties \
                cannot add one — see the note below)",
        unlocks: "The grounding rung on the artifact trace, `episodes \
                  .assertions[].basis`, the per-field provenance grade, the \
                  weakest-link floor over an output, and the verification queue in \
                  `assertion_verifications` — including whether an item routes to \
                  a tool or to a person, which is exactly \
                  `Grounding::Sourced { tool }` being present or absent.",
        without_it: "Nothing can say whether the agent fabricated a value. This is \
                     the rung the artifact trace is actually waiting on: 89 of 96 \
                     real producing agents lack it, which is why 3,571 of 3,576 \
                     episodes carry no grounding stamp. An episode from an agent \
                     with no field contract has an empty journey, and rendering \
                     that as a clean one is the over-read the whole architecture \
                     refuses.",
    },
];

/// Which rungs `agents` can answer with one query, in the order [`LADDER`]
/// declares them.
///
/// `field_contract` is now SQL-measurable. The FIELD_CONTRACTS Rust const was
/// the original source of truth, but `output_contract.grounding` (compiled from
/// sketches by `contract-sketch`) carries the same information in the card's
/// JSONB column — which SQL can see. Both paths count: a FIELD_CONTRACTS entry
/// (legacy) or a non-empty `output_contract.grounding` object (new general
/// path). `has_grounding_contract` covers both.
pub const SQL_MEASURABLE_RUNGS: &[&str] =
    &["ports", "output_type", "output_schema", "field_contract"];

/// Presence of the four card-borne rungs, and the name, for one agent.
///
/// Read from `agents` by the caller. `$1` is nothing — this is the whole-fleet
/// census, because every consumer so far wants the distribution rather than one
/// row, and a per-agent read is this filtered.
pub const CENSUS_SQL: &str = "SELECT a.agent_name, \
                                     COALESCE(array_length(a.accepts, 1), 0) > 0 \
                                       AND COALESCE(array_length(a.produces, 1), 0) > 0 \
                                       AS ports, \
                                     a.output_contract ? 'produces_schema' AS output_type, \
                                     jsonb_typeof(a.output_contract -> 'schema') = 'object' \
                                       AS output_schema, \
                                     jsonb_typeof(a.output_contract -> 'grounding') = 'object' \
                                       AND (a.output_contract -> 'grounding') != '{}' \
                                       AS field_contract \
                                FROM agents a \
                               WHERE EXISTS (SELECT 1 FROM episodes e \
                                              WHERE e.agent_id = a.agent_id)";

/// Does this agent have a grounding contract on either path?
///
/// Two paths count equally:
/// 1. `output_contract.grounding` in the card JSONB (new general path —
///    compiled from a sketch, no Rust edit per agent)
/// 2. A `FIELD_CONTRACTS` entry in `grounding_trust` (legacy path — Rust const,
///    used by agents predating the sketch compiler)
///
/// The SQL column `field_contract` in CENSUS_SQL covers path 1 directly.
/// This function covers both, for callers that need a runtime answer.
pub fn has_grounding_contract(
    agent_name: &str,
    output_contract: Option<&serde_json::Value>,
) -> bool {
    // New path: output_contract.grounding in the card.
    let has_card_grounding = output_contract
        .and_then(|oc| oc.get("grounding"))
        .and_then(|g| g.as_object())
        .map(|g| !g.is_empty())
        .unwrap_or(false);
    // Legacy path: FIELD_CONTRACTS Rust const.
    has_card_grounding || grounding_trust::contracts_for(agent_name).next().is_some()
}

/// Legacy alias — checks FIELD_CONTRACTS only. Prefer `has_grounding_contract`
/// for new callers that have the output_contract available.
pub fn has_field_contract(agent_name: &str) -> bool {
    grounding_trust::contracts_for(agent_name).next().is_some()
}

// ── legibility ───────────────────────────────────────────────────────────

/// How much of the ladder one agent has climbed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "legibility", rename_all = "snake_case")]
pub enum Legibility {
    /// **Nothing declared.** No surface can say anything about this agent that
    /// is not a row count.
    ///
    /// Not a failure of the agent and not a failure of the platform — it is an
    /// agent that has never been brought onto the substrate. Every one of the 110
    /// `test_agent_*` rows is here, and so is any newly created agent.
    Opaque,
    /// Some rungs declared. `missing` names the rest, in ladder order.
    Partial {
        present: Vec<&'static str>,
        missing: Vec<&'static str>,
    },
    /// Every rung on the ladder. Fully traceable.
    Declared,
}

/// Classify one agent from the rungs it has.
///
/// `present` is the set of rung tokens the caller measured. Unknown tokens are
/// **ignored rather than counted**, because a token this module does not know is
/// a rung someone added elsewhere, and treating it as progress would let coverage
/// rise by inventing a rung name.
pub fn legibility(present: &[&str]) -> Legibility {
    let mut have: Vec<&'static str> = Vec::new();
    let mut lack: Vec<&'static str> = Vec::new();
    for d in LADDER {
        if present.contains(&d.rung) {
            have.push(d.rung);
        } else {
            lack.push(d.rung);
        }
    }
    if have.is_empty() {
        Legibility::Opaque
    } else if lack.is_empty() {
        Legibility::Declared
    } else {
        Legibility::Partial {
            present: have,
            missing: lack,
        }
    }
}

/// Whose work is it to move this agent up the ladder?
///
/// The distinction the whole module is for. Both worklists are real; they have
/// different owners, different costs and different exit conditions, and a single
/// number over both is the one thing that must not be served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    /// A real agent missing declarations. **Authoring work**, per agent, needing
    /// someone who knows the domain.
    Retrofit,
    /// Test cruft. **Not a retrofit target** — a delete behind
    /// `/api/admin/agents/cleanup-test-cruft`'s safety gate (zero executions,
    /// past a grace period, never curated or system tier).
    ///
    /// 110 of 206 producing agents. Counting these as retrofit work makes the
    /// real job look twice its size, and the real size is what decides whether it
    /// is worth doing.
    Prune,
    /// Nothing to do: every rung declared.
    Legible,
}

/// Which worklist this agent belongs on.
///
/// Cruft is checked **before** legibility, and the order is the decision. A
/// `test_agent_*` row that somehow declared every rung is still cruft — it is a
/// fixture, and reporting it as `Legible` would inflate the coverage numerator
/// with rows that are about to be deleted. The measured fleet has no such row
/// today, which is exactly why the ordering has to be asserted rather than
/// observed.
pub fn disposition(agent_name: &str, l: &Legibility) -> Disposition {
    if is_test_cruft(agent_name) {
        return Disposition::Prune;
    }
    match l {
        Legibility::Declared => Disposition::Legible,
        _ => Disposition::Retrofit,
    }
}

// ── attributing a silence ────────────────────────────────────────────────

/// Why is a surface showing `unknown`?
///
/// Four causes, four remedies, and until this type existed they were one word.
/// `panel_absence::Reading::Unknown` is correct and deliberately coarse — three
/// readings is the right number for a colour — but a reader who sees `unknown`
/// and cannot tell *which* of these it is has been told nothing actionable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "silence", rename_all = "snake_case")]
pub enum Silence {
    /// The counters are process-local and nothing has written since boot.
    ///
    /// Resolves itself on a long-running server with traffic. **Nobody's work.**
    ColdCounter,
    /// The path exists, is wired, and no artifact has traversed it.
    ///
    /// A product or throughput question. Platform work only if the path is
    /// unreachable — which is what `liveness_trust`'s opportunity count is for,
    /// and it is the only thing that can separate *unused* from *broken*.
    NothingTraversed,
    /// An artifact traversed and **nothing could be checked**, because the
    /// subject declared no structure to check against.
    ///
    /// `rung` names the lowest missing rung, so the remedy is a specific
    /// declaration rather than "add contracts". The agent author's work.
    Undeclared { rung: &'static str },
    /// No contract in the platform answers this at all. **Our work.**
    ///
    /// Kept last and kept narrow. This is the variant everything used to collapse
    /// into, and the collapse is what made 89 undeclared agents look like 89
    /// contracts the platform had failed to write.
    Unresolved,
}

/// Who has to act on this silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Owner {
    /// The platform team.
    Platform,
    /// Whoever authored the agent, or the retrofit effort.
    AgentAuthor,
    /// Nobody: it resolves on its own, or it is a throughput fact rather than a
    /// defect.
    NoOne,
}

/// Whose work is this silence?
///
/// The load-bearing decision. Getting it wrong does not produce a wrong number,
/// it produces a wrong **backlog** — and a backlog attributing 89 agents' missing
/// declarations to the platform is one nobody can act on, so nobody does.
pub fn whose_work(s: &Silence) -> Owner {
    match s {
        Silence::Undeclared { .. } => Owner::AgentAuthor,
        Silence::Unresolved => Owner::Platform,
        Silence::ColdCounter | Silence::NothingTraversed => Owner::NoOne,
    }
}

/// Attribute an `unknown` for one agent-scoped reading.
///
/// Order is the whole content of this function, and it is the opposite of the
/// order the causes were discovered in:
///
/// 1. **a cold counter first**, because it explains everything else spuriously —
///    on a freshly booted server every gate reads `never_asked` and none of it is
///    a finding;
/// 2. **then undeclared**, because an agent that declared nothing cannot produce
///    a checkable artifact, so asking whether anything traversed is premature;
/// 3. **then nothing traversed**, which is now a real statement about throughput
///    rather than a consequence of the two above;
/// 4. **`Unresolved` only when none of the three applies** — the platform's own
///    gap, and it should be rare.
pub fn attribute(cold: bool, l: &Legibility, traversed: i64) -> Silence {
    if cold {
        return Silence::ColdCounter;
    }
    match l {
        Legibility::Opaque => Silence::Undeclared {
            // The lowest rung: an opaque agent's remedy starts at the cheapest
            // declaration, not at the one the surface happens to want.
            rung: LADDER[0].rung,
        },
        Legibility::Partial { missing, .. } => Silence::Undeclared {
            rung: missing.first().copied().unwrap_or(LADDER[0].rung),
        },
        Legibility::Declared => {
            if traversed == 0 {
                Silence::NothingTraversed
            } else {
                Silence::Unresolved
            }
        }
    }
}

// ── the census ───────────────────────────────────────────────────────────

/// Fleet-wide coverage, per rung, split by disposition.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Census {
    /// Agents that have produced at least one episode.
    pub producing: usize,
    /// Real agents among them. The denominator that matters.
    pub real: usize,
    /// `test_agent_*` rows. The prune list's size.
    pub cruft: usize,
    /// Per rung, how many **real** agents have it.
    ///
    /// Real only, and that is not a filter for tidiness: 110 cruft rows declaring
    /// nothing would drag every rung's coverage down by more than half and make
    /// the retrofit look hopeless when the ports rung is at 93 of 96.
    pub by_rung: Vec<(&'static str, usize)>,
    /// Real agents with no rung at all.
    pub opaque: usize,
    /// Real agents with every rung.
    pub declared: usize,
}

/// Build the census from measured per-agent rungs.
///
/// `agents` is `(name, rungs_present)`. Pure, so the shape a surface receives is
/// testable without a database.
pub fn census(agents: &[(String, Vec<&'static str>)]) -> Census {
    let mut c = Census {
        producing: agents.len(),
        by_rung: LADDER.iter().map(|d| (d.rung, 0usize)).collect(),
        ..Default::default()
    };
    for (name, rungs) in agents {
        if is_test_cruft(name) {
            c.cruft += 1;
            continue;
        }
        c.real += 1;
        let l = legibility(rungs);
        match l {
            Legibility::Opaque => c.opaque += 1,
            Legibility::Declared => c.declared += 1,
            Legibility::Partial { .. } => {}
        }
        for entry in c.by_rung.iter_mut() {
            if rungs.contains(&entry.0) {
                entry.1 += 1;
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An agent that declared nothing is not the platform's failure.
    ///
    /// The whole point. Before this type existed, an undeclared agent's silence
    /// collapsed into `Resolver::Unresolved` — which reads as *the platform has
    /// not written a contract for this* — and 89 real agents in that state made
    /// the platform's backlog look like 89 contracts it owed. It owes none of
    /// them: they are declarations the agents have not made.
    #[test]
    fn an_undeclared_agent_is_the_authors_work_and_not_the_platforms() {
        let s = attribute(false, &Legibility::Opaque, 0);
        assert_eq!(s, Silence::Undeclared { rung: "ports" });
        assert_eq!(whose_work(&s), Owner::AgentAuthor);

        // And the platform's own gap is still reachable, or the distinction
        // would be a rename.
        let ours = attribute(false, &Legibility::Declared, 12);
        assert_eq!(ours, Silence::Unresolved);
        assert_eq!(whose_work(&ours), Owner::Platform);
    }

    /// A cold counter explains everything and must be checked first.
    ///
    /// On a freshly booted server every gate reads `never_asked` and no part of
    /// that is a finding. Attributing that to a missing declaration would send an
    /// author to write a contract for a reading that will fix itself on the next
    /// request.
    #[test]
    fn a_cold_counter_outranks_every_other_explanation() {
        assert_eq!(
            attribute(true, &Legibility::Opaque, 0),
            Silence::ColdCounter,
            "a cold counter was attributed to the agent's declarations"
        );
        assert_eq!(whose_work(&Silence::ColdCounter), Owner::NoOne);
    }

    /// `NothingTraversed` is only reachable for a declared agent.
    ///
    /// Otherwise it is a category error dressed as a measurement: an agent that
    /// declared no structure cannot produce a checkable artifact, so "nothing
    /// traversed" would be true of it forever and would read as a throughput
    /// problem. That is how a declaration gap becomes a product question and
    /// stops being anybody's job.
    #[test]
    fn nothing_traversed_is_not_offered_as_an_excuse_for_silence() {
        let partial = Legibility::Partial {
            present: vec!["ports"],
            missing: vec!["output_type", "output_schema", "field_contract"],
        };
        assert_eq!(
            attribute(false, &partial, 0),
            Silence::Undeclared {
                rung: "output_type"
            },
            "a partially declared agent with no traffic was reported as a \
             throughput fact rather than as the next declaration it needs"
        );
        assert_eq!(
            attribute(false, &Legibility::Declared, 0),
            Silence::NothingTraversed
        );
    }

    /// The remedy names the **lowest** missing rung.
    ///
    /// An author told "you need a field contract" when the agent has not declared
    /// its ports has been given the most expensive step first, and the ladder is
    /// ordered by author cost precisely so the answer is the cheapest useful one.
    #[test]
    fn the_remedy_offered_is_the_cheapest_missing_declaration() {
        let l = legibility(&["field_contract"]);
        let Silence::Undeclared { rung } = attribute(false, &l, 5) else {
            panic!("a partially declared agent was not reported as undeclared");
        };
        assert_eq!(rung, "ports", "the ladder's cheapest rung is `ports`");
    }

    /// Cruft is cruft even if it declares everything.
    ///
    /// The ordering inside `disposition`, asserted because the fleet has no such
    /// row today — so this is exactly the property that would be silently wrong
    /// while looking observationally fine, and would inflate the coverage
    /// numerator with rows that are about to be deleted.
    #[test]
    fn a_fully_declared_fixture_is_still_a_prune_target() {
        assert_eq!(
            disposition("test_agent_abc", &Legibility::Declared),
            Disposition::Prune
        );
        assert_eq!(
            disposition("weather_oracle", &Legibility::Declared),
            Disposition::Legible
        );
        assert_eq!(
            disposition("weather_oracle", &Legibility::Opaque),
            Disposition::Retrofit
        );
    }

    /// The census denominator excludes cruft, and the two lists stay apart.
    #[test]
    fn the_census_keeps_the_two_worklists_separate() {
        let agents = vec![
            ("weather_oracle".to_string(), vec!["ports"]),
            ("football_analyst".to_string(), vec!["ports", "output_type"]),
            ("test_agent_1".to_string(), vec![]),
            ("test_agent_2".to_string(), vec![]),
            ("test_agent_3".to_string(), vec![]),
        ];
        let c = census(&agents);
        assert_eq!(c.producing, 5);
        assert_eq!(c.real, 2, "cruft is in the denominator");
        assert_eq!(c.cruft, 3);
        assert_eq!(
            c.by_rung
                .iter()
                .find(|(r, _)| *r == "ports")
                .map(|(_, n)| *n),
            Some(2),
            "the ports rung must be 2 of 2 real agents, not 2 of 5 — 110 cruft \
             rows declaring nothing would halve every rung and make the retrofit \
             look hopeless when ports is at 93 of 96"
        );
        assert_eq!(c.opaque, 0);
        assert_eq!(c.declared, 0);
    }

    /// An invented rung name cannot raise coverage.
    #[test]
    fn a_rung_this_module_does_not_declare_is_not_progress() {
        assert_eq!(legibility(&["something_new"]), Legibility::Opaque);
    }

    /// Every rung argues for itself.
    ///
    /// `unlocks` is what stops the ladder becoming a checklist. The case that
    /// tested it: `fermi_contract` is present on 15 of 96 real agents, which would
    /// have more than doubled one rung's coverage, and it holds domain
    /// configuration for forecast agents (`finding_labels`, `multiplier_range`)
    /// that no trust surface can read. A rung unlocking nothing inflates the
    /// number with something no consumer can use.
    #[test]
    fn every_rung_names_what_it_unlocks_and_what_breaks_without_it() {
        for d in LADDER {
            assert!(
                d.unlocks.len() >= 80,
                "`{}` does not say what it unlocks, so nothing stops it being a \
                 checklist item",
                d.rung
            );
            assert!(
                d.without_it.len() >= 80,
                "`{}` does not say what reads `unknown` without it — which is the \
                 sentence a surface shows in place of a blank",
                d.rung
            );
            assert!(
                !d.owner.is_empty(),
                "`{}` does not say where the declaration lives, so a retrofit \
                 does not know what file to open",
                d.rung
            );
        }
        assert!(LADDER.len() >= 4, "the ladder has lost rungs");
    }

    /// Removing a field contract is unambiguously a regression.
    ///
    /// The one thing here that is safely ratcheted. Coverage itself is **not**:
    /// new agents arrive undeclared by definition, so a ratchet on the fleet
    /// count would fire on entirely correct behaviour, and §5.2 says what happens
    /// to a check that cries wolf. `FIELD_CONTRACTS` is different — it is a
    /// hand-maintained const, nothing arrives in it by accident, and a shrinking
    /// list means someone deleted a contract.
    #[test]
    fn the_field_contract_roster_does_not_shrink() {
        // 9 → 10 when `video_analyst` was brought under the contract. Raised
        // rather than relaxed, and recorded here rather than in a commit message
        // because the number is the claim: this ratchet is the only place the
        // platform asserts that its grounding coverage has not gone backwards,
        // and a reader needs to know whether a change to it was an improvement
        // or an accommodation.
        //
        // It also demonstrates the ratchet working on work that was not its
        // author's: `video_analyst` arrived from a parallel session and this went
        // red on the next full run, which is the two-way half earning its keep.
        const CONTRACTED_AGENTS: usize = 10;
        let mut names: Vec<&str> = grounding_trust::FIELD_CONTRACTS
            .iter()
            .map(|c| c.agent_id)
            .collect();
        names.sort_unstable();
        names.dedup();
        assert!(
            names.len() >= CONTRACTED_AGENTS,
            "{} agents have a field contract, down from {CONTRACTED_AGENTS}. \
             Removing one takes an agent's whole output back to unverifiable and \
             is the only movement on this ladder that is always a regression. If \
             the deletion was deliberate, lower the constant in the same commit \
             and say why.",
            names.len()
        );
        // Two-way: if it grew, the floor rises with it.
        assert_eq!(
            names.len(),
            CONTRACTED_AGENTS,
            "{} agents now have a field contract, up from {CONTRACTED_AGENTS}. \
             Raise the constant so the ratchet holds at the new floor.",
            names.len()
        );
    }
}
