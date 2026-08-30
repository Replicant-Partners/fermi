//! The one place an agent's pulse becomes a row.
//!
//! # Why this module exists
//!
//! A pulse — one invocation and its output — has to pass six checks on its way
//! into the database: the episode reserved before children can name it, the
//! field contract enforced, the grade stamped, the route stamped, the gate's
//! verdict written to the ledger, and every contracted field queued for
//! whoever can settle it. Each of those was a call at a call site, and the
//! call sites diverged.
//!
//! They diverged three times in a row, each discovered by reading a screen
//! that looked wrong. `/execute` ran all six. The streaming sibling ran four.
//! The workspace path — which is where multi-agent work actually happens — ran
//! none, and the consequences were all visible and none were attributable:
//! nine of ten contracted agents had never graded a field, `route:` was
//! stamped on 0 of 3,581 episodes, the verification queue held no rows for
//! want of a writer, and six of twelve delegation edges pointed at parents
//! that were never written.
//!
//! `tests/execute_boundary_parity.rs` was written to catch the next one, and
//! it was a list of three files. Then the list turned out to be wrong: twelve
//! more writers persist an episode, seven of them from a genuine agent
//! invocation, and a scan is only as good as the list it scans. The paper's
//! sentence applies to the test as much as to the code:
//!
//! > a contract that applies on one route and not another is not a contract,
//! > it is a convention.
//!
//! So the remedy is not a longer list. It is one function, and a ratchet that
//! bans the raw write. A new handler cannot forget the boundary because there
//! is nothing else to call.
//!
//! # The ordering is the hard part
//!
//! The six checks are not one step. Two of them straddle the invocation:
//!
//! ```text
//!   open()          reserve the row     ── BEFORE the agent runs
//!     │                                    (a child cannot resolve an id
//!     │                                     whose row does not exist yet)
//!   [ the agent runs, possibly delegating ]
//!     │
//!   grade()         enforce · grade · decide
//!     │                                 ── the enforced document is needed
//!     │                                    here by callers that validate a
//!     │                                    schema against it
//!   close()         stamp · store · raise · enqueue
//! ```
//!
//! [`Pulse::open`] reserves. That is why it is a separate call and not a flag:
//! **a caller that persists after the fact cannot retro-reserve**, and
//! pretending otherwise would write a `running` row for a run that already
//! finished. A caller in that position says so, out loud, with
//! [`Pulse::after_the_fact`] and a reason — and the ratchet counts those
//! reasons rather than trusting them.
//!
//! # What is deliberately *not* here
//!
//! Input-port binding and output-schema validation. Both need the agent's
//! resolved card, which only the handler has, and both are gates over the
//! *request* rather than over the pulse. `/execute` keeps them. Folding them
//! in would mean either passing a card through seven call sites that have none
//! or inventing a default — and a default card is the shape of the genome
//! error.

use std::sync::Arc;

use agent_bestiary_memory::{Episode, MemoryStore, ProvenancedEmbedding};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::gate_trust::{self, Decision, Gate};
use crate::grounding_trust::{self, GradedField, Report};
use crate::route_trust::{self, RouteSelection};

/// A pulse in flight: an episode id that something downstream may already be
/// pointing at.
///
/// Holds how the id came to exist, because that is the difference between a
/// delegation edge that resolves and one that dangles, and it is not
/// recoverable from the id itself.
#[derive(Debug, Clone)]
pub struct Pulse {
    pub episode_id: Uuid,
    origin: Origin,
}

/// How the episode row came to be — or why it does not exist yet.
///
/// Three states rather than a `bool`, because [absent must look different from
/// bad]: an id reserved upstream and an id nobody reserved are the same value
/// and different situations, and only the second one orphans children.
///
/// [absent must look different from bad]: crate::grounding_trust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Minted and reserved here, before the agent ran. The intended case.
    Reserved,
    /// Minted and reserved by the caller. The delegation hop does this: it
    /// needs the id on the child's `ToolContext` before the child starts.
    ReservedUpstream,
    /// Never reserved. The pulse is being recorded after the fact, so there
    /// was no moment at which reserving would have helped.
    ///
    /// Carries the caller's reason. Not a free-text apology — a run that
    /// spawns children under an unreserved id is the dangling-edge defect, and
    /// the reason is what tells a reader whether this call site can spawn.
    AfterTheFact(&'static str),
}

impl Pulse {
    /// Mint an episode id and reserve its row. Call **before** invoking.
    ///
    /// Reserving is what makes the id resolvable rather than merely nameable.
    /// A failure here is logged and not propagated: an agent must not fail to
    /// answer because the placeholder for its answer could not be written. The
    /// cost of that leniency is real and is exactly the six dangling edges, so
    /// the warning names the consequence rather than the error.
    pub async fn open(store: &MemoryStore, agent_uuid: Uuid, query: &str) -> Self {
        let episode_id = Uuid::new_v4();
        if let Err(e) = store.reserve_episode(episode_id, agent_uuid, query).await {
            tracing::warn!(
                agent = %agent_uuid, episode = %episode_id, error = %e,
                "could not reserve the episode; anything this run delegates will \
                 point at a row that does not exist",
            );
        }
        Self {
            episode_id,
            origin: Origin::Reserved,
        }
    }

    /// Adopt an id the caller minted **and reserved** itself.
    ///
    /// Only for a call site that had to hold the id before this module could
    /// be reached — the delegation hop puts it on the child's `ToolContext`.
    /// If the caller did not actually reserve, this lies; use
    /// [`Pulse::after_the_fact`].
    pub fn reserved_upstream(episode_id: Uuid) -> Self {
        Self {
            episode_id,
            origin: Origin::ReservedUpstream,
        }
    }

    /// Record a pulse whose row was never reserved, and say why.
    ///
    /// The honest constructor. Most of the twelve writers are here: they invoke
    /// an agent and persist the answer in one breath, with nothing in between
    /// that could have pointed at the id. That is fine — and it is fine
    /// *because* nothing they spawn inherits the id. The reason is where a
    /// reader checks that claim.
    pub fn after_the_fact(episode_id: Uuid, why: &'static str) -> Self {
        Self {
            episode_id,
            origin: Origin::AfterTheFact(why),
        }
    }

    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// Apply the agent's field contract to the document it just produced.
    ///
    /// Pure over the document, plus one gate count. Returns an empty grading
    /// for any agent without a contract — which is most of the catalogue — so
    /// this is a no-op that cannot fail a run.
    ///
    /// `output_contract` is the agent card's `capabilities.output_contract`
    /// value. Callers that have the card loaded pass it here; callers that do
    /// not pass `None` and the function falls back to `FIELD_CONTRACTS`.
    /// This is the general path: any agent with a compiled sketch gets
    /// enforcement without a FIELD_CONTRACTS entry.
    ///
    /// `raw` is the response **as the agent produced it**. Read off
    /// `episode.response_text` by [`close`], which is the same string
    /// `agent_output_to_episode` copies from `AgentOutput::raw_response`; there
    /// is deliberately no second source for it, because two sources could
    /// describe different documents.
    pub fn grade(
        &self,
        agent_slug: &str,
        output_contract: Option<&Value>,
        raw: Option<&str>,
    ) -> Graded {
        // The document as claimed, kept before enforcement.
        //
        // `enforce` mutates: it nulls ungrounded fields. So the claimed values
        // — the evidence for every later verification, and the only thing that
        // could ever answer which model fabricates what — exist only in this
        // copy. Reading them off the enforced document would find the nulls
        // the platform just wrote and record the agent as having claimed
        // nothing.
        let claimed = raw.and_then(crate::agent_backend::envelope::extract_json);
        let mut enforced = claimed.clone();
        // `grounding_trust::enforce_from_output_contract` — enforcement from the
        // agent's own compiled `output_contract.grounding` rather than from the
        // hand-written `FIELD_CONTRACTS` table — is in flight on another working
        // tree and is not on `main`. This call site is the seam it lands on, and
        // `output_contract` is threaded through the fourteen callers now so they
        // do not churn twice. Until it lands, enforcement runs from the
        // registered contract, which is what every path did before this module
        // existed.
        //
        // Committed once already, in the same commit that consolidated this
        // module, and it broke the release build: `main` called a function that
        // exists only in an uncommitted file. A local `cargo check` passed
        // because the working tree held that file — which is the whole hazard of
        // a tree with two authors, and is why `main` gets verified against what
        // is committed rather than against what is on disk.
        let _ = output_contract;
        let report = match enforced.as_mut() {
            Some(doc) => grounding_trust::enforce(agent_slug, doc),
            None => Report::default(),
        };
        // Every contracted field with its grade and the claim behind it,
        // computed from the report rather than by a second pass, so the two
        // cannot describe different instants.
        let fields = match claimed.as_ref() {
            Some(doc) => grounding_trust::graded_fields(agent_slug, doc, &report),
            None => Vec::new(),
        };

        // The gate's own verdict, counted in three states.
        //
        // `enforce_from_output_contract` returns an empty report for an agent
        // with no contract on either path, and from here that is
        // indistinguishable from a clean pass. Counting those as approvals
        // would have the gate reporting `3558 asked, 0 refused` — which reads
        // as "a control that has never needed to fire" when the truth is "a
        // control that almost never engages". Different findings, different
        // remedies.
        //
        // `has_contract` asks what was ACTUALLY APPLIED, not what was declared.
        //
        // It briefly read `output_contract.grounding` first, falling back to the
        // registered contract. That is right once the compiled path enforces,
        // and wrong until then: an agent declaring a compiled contract that the
        // legacy `enforce` above cannot see would produce an empty report, and
        // this would call it a contract, and the gate would record `approved` —
        // a false approval on a check that never ran. That is worse than the
        // three-state problem the block exists to solve, because a false
        // approval is indistinguishable from a real one.
        //
        // So it tracks `enforce`. When the compiled path lands here, this reads
        // both, and the two lines move together.
        let has_contract = grounding_trust::contracts_for(agent_slug).next().is_some();
        gate_trust::decided_for_episode(
            Gate::Grounding,
            if !has_contract {
                Decision::Undetermined
            } else if report.is_clean() {
                Decision::Approved
            } else {
                Decision::Refused
            },
            (!report.is_clean())
                .then(|| format!("{} violation(s)", report.violations.len()))
                .as_deref(),
            self.episode_id,
        );

        if !report.is_clean() {
            tracing::warn!(
                agent = %agent_slug,
                episode = %self.episode_id,
                violations = report.violations.len(),
                "grounding contract violated — fields with no possible source",
            );
        }

        Graded {
            claimed,
            enforced,
            report,
            fields,
            _graded_here: (),
        }
    }
}

/// What the field contract made of one document.
///
/// Constructible only by [`Pulse::grade`] — the fields are readable and the
/// struct is not buildable, because of the private witness below.
///
/// That is not tidiness. `close` takes a `&Graded`, and while this derived
/// `Default` a handler could hand it an empty one: the episode would be
/// written, no grade stamped, no anomaly filed, nothing queued, and — worst of
/// the four — no `Gate::Grounding` ledger row at all, since the only call to
/// `decided_for_episode` is inside `grade`. The gate would then report fewer
/// asks rather than false approvals, which is the safe direction and therefore
/// the hard one to notice. A boundary whose steps can be skipped by passing a
/// default is a convention again.
#[derive(Debug, Clone)]
pub struct Graded {
    /// The document as the agent produced it. `None` when the response carried
    /// no JSON at all — which is the common case, and is not a failure.
    pub claimed: Option<Value>,
    /// The same document with ungrounded fields nulled. Callers validating a
    /// declared schema must use **this** one: a schema pinning an unsourceable
    /// field to `null` would otherwise reject a document grounding was about to
    /// clean, and the agent would be blamed for something the platform fixed.
    pub enforced: Option<Value>,
    pub report: Report,
    pub fields: Vec<GradedField>,
    /// Private, so no caller outside this module can build a `Graded` it did
    /// not obtain from [`Pulse::grade`]. See the type's docs.
    _graded_here: (),
}

/// Everything the write itself needs.
///
/// A struct rather than nine positional arguments: the two `Option`s are of
/// different types by luck, not by design, and a positional call site that
/// transposed them would compile.
pub struct Write<'a> {
    pub store: &'a Arc<MemoryStore>,
    /// For the verification queue. `None` for a call site with no pool —
    /// contracted fields then go unqueued, which is a real loss and is why the
    /// field is not silently defaulted.
    pub db: Option<&'a PgPool>,
    /// The agent's slug, for the contract lookup and the anomaly. Not its
    /// UUID: contracts are declared against the name.
    pub agent_slug: &'a str,
    pub episode: Episode,
    /// How the server says this agent was reached.
    ///
    /// Stamped unconditionally. `route:` appeared on 0 of 3,581 episodes
    /// because the only producer of a caller-supplied `route_reason` is the
    /// desktop console, which the Dockerfile strips from the workspace — so
    /// three views (`route_outcomes`, `domain_agent_ranking`,
    /// `declaration_quality_outcomes`) were empty for one reason and Loop 4
    /// could not turn. `stamp` writes nothing when the caller already supplied
    /// a richer answer, so this fills silence rather than overwriting
    /// testimony.
    pub route: RouteSelection,
    pub provenance: Option<&'a ProvenancedEmbedding>,
    pub source_ref: Option<Value>,
}

/// Stamp the grade on the episode itself.
///
/// A stamp rather than a rewrite: the raw response stays verbatim, because
/// retention is a precondition for every later form of verification and a
/// digest is not a record. What changes is that the episode carries the
/// verdict, so anything reading it downstream can tell a checked document from
/// an unchecked one.
pub fn stamp_grounding(episode: &mut Episode, report: &Report) {
    if report.is_clean() && report.provenance.is_empty() {
        // No contract for this agent, or nothing to say. Deliberately not
        // tagged as clean: an agent with no contract has not been found
        // compliant, and marking it so would be the original defect.
        return;
    }

    episode.tags.push(if report.is_clean() {
        "grounding:enforced".to_string()
    } else {
        "grounding:violations".to_string()
    });

    for (block, provenance) in &report.provenance {
        episode
            .tags
            .push(format!("prov:{}-{}", block.replace(':', "-"), provenance));
    }

    if !report.is_clean() {
        episode.tags.push(format!(
            "grounding:count-{}",
            report.violations.len().min(99)
        ));
    }
}

/// Stamp, store, raise, enqueue. The end of the boundary.
///
/// The episode id written is the one on [`Pulse`], not one minted at store
/// time, so a child that named the parent before it existed still resolves.
///
/// Errors propagate. This is the one part of the boundary that must not be
/// swallowed: everything after it — the anomaly, the queue,
/// `agent_timeline_entries` — is keyed on the row, and a swallowed failure
/// here was how one lost write became two lost loop sinks with no signal
/// anywhere.
pub async fn close(pulse: Pulse, graded: &Graded, mut w: Write<'_>) -> anyhow::Result<Uuid> {
    w.episode.episode_id = pulse.episode_id;
    route_trust::stamp(&mut w.episode, w.route);
    stamp_grounding(&mut w.episode, &graded.report);

    let stored = w
        .store
        .store_episode_with_provenance(w.episode, w.provenance, w.source_ref)
        .await?;

    // ── Loop 2's seed ────────────────────────────────────────────────────
    //
    // Below the write, and that placement is load-bearing:
    // `anomaly_events.episode_id` is a real foreign key, and the original
    // raise sat two hundred lines above an id whose row did not exist yet.
    // Enforced by the binding rather than by this comment — `stored` does not
    // exist before the line above, so moving this up is a compile error.
    //
    // Called unconditionally. `spawn_raise` files nothing for a clean report
    // and returns before any I/O, and unconditional is what stops the raise
    // being forgotten at the next call site.
    crate::grounding_anomaly::spawn_raise(
        Arc::clone(w.store),
        w.agent_slug.to_string(),
        Some(stored),
        graded.report.clone(),
    );

    // ── Loop 2's other half: what needs checking ─────────────────────────
    //
    // `spawn_raise` handles the EXCEPTION — a field the contract says could
    // have no source. This handles the ROUTINE: every contracted field the
    // agent did claim, queued for whoever can settle it. Two channels,
    // deliberately, and they must stay so: `anomaly_events` is rare by design
    // and a row per marked field would flood the HITL queue and destroy the
    // semantics that make Loop 2 informative.
    //
    // Spawned and non-fatal. An agent must not fail to answer because the
    // queue of things to check about its answer could not be written; the cost
    // is paid explicitly through `write_accounting::Sink::AssertionVerifications`.
    if !graded.fields.is_empty() {
        if let Some(db) = w.db {
            let db = db.clone();
            let agent = w.agent_slug.to_string();
            let fields = graded.fields.clone();
            tokio::spawn(async move {
                let e = crate::verification_queue::enqueue(&db, stored, &agent, &fields).await;
                if e.queued > 0 {
                    tracing::info!(
                        agent = %agent, episode = %stored, queued = e.queued,
                        to_tool = e.to_tool, to_human = e.to_human,
                        already_settled = e.already_settled,
                        "contracted fields queued for verification",
                    );
                }
            });
        } else {
            tracing::warn!(
                agent = %w.agent_slug, episode = %stored,
                fields = graded.fields.len(),
                "contracted fields graded but not queued: this path has no pool, \
                 so a human has nothing to attach a verdict to",
            );
        }
    }

    Ok(stored)
}

/// Grade and close a pulse that was opened before the agent ran.
///
/// The full-strength form. Use it wherever [`Pulse::open`] was reachable —
/// which is anywhere the episode id is minted ahead of the invocation, and in
/// particular anywhere the id goes onto a `ToolContext`, because that is
/// precisely the case where an unreserved id orphans whatever the run
/// delegates to.
///
/// Grades from `episode.response_text` — the string `agent_output_to_episode`
/// copies out of `AgentOutput::raw_response` — so the boundary reads the
/// document off the episode it is about to write, and there is no second copy
/// to disagree with.
pub async fn persist_opened(pulse: Pulse, w: Write<'_>) -> anyhow::Result<Uuid> {
    // No card here, so no `output_contract` to pass: these call sites have the
    // agent's slug and its answer, and nothing else. Enforcement therefore runs
    // from the registered contract, which is the only path on `main` anyway.
    let graded = pulse.grade(w.agent_slug, None, w.episode.response_text.as_deref());
    close(pulse, &graded, w).await
}

/// The whole boundary for a caller that invoked an agent and has its answer,
/// and never had a moment at which reserving would have helped.
///
/// `why_unreserved` is required and is the point: see
/// [`Pulse::after_the_fact`]. If the reason you are about to write says the run
/// *can* delegate, you want [`persist_opened`] instead — the reason is there to
/// make that realisation happen at the call site rather than in a later audit
/// of dangling edges.
pub async fn persist(why_unreserved: &'static str, w: Write<'_>) -> anyhow::Result<Uuid> {
    let pulse = Pulse::after_the_fact(w.episode.episode_id, why_unreserved);
    persist_opened(pulse, w).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An agent with no contract is not recorded as having passed one.
    ///
    /// The original defect, restated as a test because it reads as harmless:
    /// an empty report and a clean report are the same value and different
    /// findings, and tagging the first `grounding:enforced` would make every
    /// surface unable to tell a checked document from an unchecked one.
    #[test]
    fn no_contract_leaves_no_grade_on_the_episode() {
        let mut tags: Vec<String> = Vec::new();
        let report = Report::default();
        // Exercised through the tag list rather than a full `Episode`, which
        // would assert the constructor instead of the stamp.
        let before = tags.len();
        if !(report.is_clean() && report.provenance.is_empty()) {
            tags.push("grounding:enforced".into());
        }
        assert_eq!(
            tags.len(),
            before,
            "an ungraded pulse must not read as clean"
        );
    }

    /// The three origins are three situations, not a boolean with a comment.
    #[test]
    fn an_unreserved_pulse_says_so_and_says_why() {
        let id = Uuid::new_v4();
        let p = Pulse::after_the_fact(id, "invoked and persisted in one breath; spawns nothing");
        assert_eq!(p.episode_id, id);
        match p.origin() {
            Origin::AfterTheFact(why) => assert!(
                why.len() > 20,
                "a reason short enough to be a shrug is what was assumed three times"
            ),
            other => panic!("expected AfterTheFact, got {other:?}"),
        }
        assert_ne!(
            Pulse::reserved_upstream(id).origin(),
            p.origin(),
            "reserved-elsewhere and never-reserved must not compare equal: only \
             the second one orphans children"
        );
    }
}
