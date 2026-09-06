//! What did the gate decide, and how often?
//!
//! # The gap this closes
//!
//! An audit of every refusal point in the system found that **no gate decision
//! is persisted anywhere.** The coherence gate computes a verdict, the handler
//! branches on it, it goes into an HTTP response body, and it is gone. Credit
//! refusals return before the `credit_ledger` INSERT. The rate limiter is an
//! in-memory `DashMap` with no export. A publish refusal returns a 400 — while
//! the *bypass* of that same check is audited to `admin_bypass_events`.
//!
//! The consequence, stated plainly: **the platform has a record of every
//! request it served and none of any it refused.**
//!
//! That is how the Γ arithmetic bug survived. The coherence gate rejected
//! *100% of agent-wide interventions* for reasons that had nothing to do with
//! their content, and because the two-reviewer path sits downstream of the gate,
//! the strongest control in Loop 2 was unreachable. Nobody could see it, because
//! there was no record the gate had ever been asked.
//!
//! # A gate that has never refused has not been tested
//!
//! `verification_for_agent_ecologies.md` §5.1 says a check that has never
//! failed has not been tested. The same sentence about a gate is sharper,
//! because a gate has two failure modes and they are symmetric:
//!
//! | counters | reading |
//! |---|---|
//! | asked = 0 | the gate has never been exercised. Not a pass. |
//! | asked > 0, approved = 0 | **refuses everything.** The Γ bug's exact signature. |
//! | asked > 0, refused = 0 | **admits everything.** Indistinguishable from no gate at all. |
//!
//! The third is the one nobody checks, and it is the more dangerous, because a
//! gate that never fires looks like a well-behaved system rather than a broken
//! control. `hud_contract::enforce` is a thousand lines of display gate with no
//! production caller; from every dashboard the platform has, that is
//! indistinguishable from a display that never needed correcting.
//!
//! Neither reading is available from a log line, and neither is available from
//! the thing the gate protects. They are only available by counting.
//!
//! # Counted always, recorded sometimes
//!
//! Two tiers, because the volumes differ by orders of magnitude and the
//! questions do too.
//!
//! * **Counted** — every gate, every decision, in memory. Free, cannot fail,
//!   answers "approved 0 of 47". Deliberately the same design as
//!   [`crate::write_accounting`]: a ledger that is itself a fallible database
//!   write is most silent when it is most needed.
//! * **Recorded** — additionally written to `gate_decisions`, for gates whose
//!   individual decisions are governance events that must survive a restart. A
//!   blocked agent-wide correction is one. A rate-limit tick is not, and
//!   recording one per request would turn a control into a load generator.
//!
//! Which tier a gate is in is **declared**, not inferred from volume at the call
//! site, so the decision is reviewable in one place.
//!
//! The recorded write goes through [`crate::write_accounting`], so a gate ledger
//! that cannot write is itself counted. The rungs compose in the right
//! direction: the thing that watches the gates is watched by the thing that
//! watches the writes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Which of the three clocks a gate runs on.
///
/// From `verification_for_agent_ecologies.md` §4.1. Retained as a field rather
/// than a comment because the clock decides what an absent verdict *means*: on
/// the admission and invocation clocks somebody is waiting, and an absent
/// verdict is an outage; on the standing clock nothing waits, and an absent
/// verdict is invisible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Clock {
    /// Slow. Blocks publish. Runs once per authored card.
    Admission,
    /// Fast. Blocks the response. Runs once per request.
    Invocation,
    /// Boot and sweep. Blocks nothing; can only report.
    Standing,
}

/// Whether individual decisions are durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Retention {
    /// Counters only.
    Counted,
    /// Counters, plus one row per decision in `gate_decisions`.
    Recorded,
}

/// Every point in the system that can refuse something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(usize)]
pub enum Gate {
    /// TEC settling of a human correction against the agent's world model.
    Coherence = 0,
    /// Grounding enforcement on an agent's output.
    Grounding = 1,
    /// Does the request match the agent's declared input port?
    InputBinding = 2,
    /// Card presence, typing and grounding, at publish.
    Admission = 3,
    /// Insufficient credits.
    Credit = 4,
    /// Too many requests.
    RateLimit = 5,
    /// A frame the model cannot see, or the provider cannot carry.
    Attachment = 6,
    /// A delegated document that contradicts its producer's own declared type.
    OutputSchema = 7,
    /// A caller's query that does not conform to the callee's declared input_contract.schema.
    ///
    /// Advisory (non-blocking) — recorded as a counter, never halts execution.
    /// Symmetric to OutputSchema. Absence means no input_contract was declared.
    InputSchema = 8,
    /// Did the agent fill the fields it was asked for?
    ///
    /// The question nothing asked. Grounding checks whether a tool *could* have
    /// supplied a value and never whether the agent *did* produce one, so an
    /// empty contracted field inherits its block's grade and reads as sourced.
    /// The artifact trace computes it on the page, from the values, precisely
    /// because no checkpoint computed it — and it was the only cell on that
    /// surface with no gate behind it, rendered `no gate` in a dotted box that
    /// read as the agent bypassing something rather than as our own gap.
    Completeness = 9,
}

/// One gate's declaration.
#[derive(Debug, Clone, Copy)]
pub struct GateSpec {
    pub gate: Gate,
    /// Stable identifier, used as the `gate` column and the JSON key.
    pub id: &'static str,
    pub clock: Clock,
    pub retention: Retention,
    /// The decision site. Named so a count points at a file.
    pub site: &'static str,
    /// What the gate refuses.
    pub refuses: &'static str,
    /// What it means if this gate refuses **nothing**.
    ///
    /// Required, and the field that makes `admits_everything` actionable. A
    /// count of zero refusals is only alarming if someone has written down why.
    pub if_never_refuses: &'static str,
    /// Does this gate decide **before** the artifact exists?
    ///
    /// # Why this is a field and not an inference
    ///
    /// It cannot be read off [`Clock`]: `credit` and `grounding` are both
    /// `Invocation`, and one fires before the agent runs while the other fires
    /// on its output. Nothing else in this struct distinguishes them.
    ///
    /// The distinction is load-bearing on the artifact trace. A gate that
    /// decides before the artifact can never name one, so a NULL `episode_id`
    /// is **permanent and correct** for it -- not a gap, not a backfill target,
    /// and not something a reader should be invited to chase. Rendering that
    /// identically to a gate that *should* have recorded and did not turns a
    /// correct absence into a standing debt on every artifact forever, which is
    /// what the UX team reported seeing.
    ///
    /// `true` also implies the decision may be the reason no artifact exists: a
    /// refused `credit` check means the run never happened.
    pub decides_before_the_artifact: bool,
}

/// Every gate, in discriminant order.
pub const GATES: &[GateSpec] = &[
    GateSpec {
        gate: Gate::Coherence,
        id: "coherence",
        // Decides about the artifact, so it can name one.
        decides_before_the_artifact: false,
        clock: Clock::Invocation,
        retention: Retention::Recorded,
        site: "agent-bestiary/coherence-gate::CoherenceGate::check_against, \
               via handlers::observatory",
        refuses: "an AgentWide correction the agent's world model rejects",
        if_never_refuses: "Loop 2's only structural control is passing everything \
                           a reviewer proposes. Since the two-reviewer consensus \
                           path sits downstream of this gate, a gate that never \
                           refuses is also a consensus requirement that is never \
                           reached. Note the inverse was the real defect: it \
                           refused 100% for arithmetic reasons and nothing could \
                           see it.",
    },
    GateSpec {
        gate: Gate::Grounding,
        id: "grounding",
        // Decides about the artifact, so it can name one.
        decides_before_the_artifact: false,
        clock: Clock::Invocation,
        // Promoted from `Counted` by migration 221, which is a file whose only
        // content is the argument: a change in what the platform durably records
        // is a decision, and a decision made in a constant is one nobody can
        // find.
        //
        // The short version. It was refused once, because a grounding verdict is
        // n per-field findings and `reason` is one free-text column -- and that
        // objection was about where the DETAIL lives, which is now
        // `assertion_verifications`. The row carries the decision, not the
        // findings. Volume measured rather than feared: ~30 episodes a day, and
        // 214's rate-limit argument does not transfer because a tick fires per
        // request including the floods it rejects, while this fires per completed
        // execute.
        retention: Retention::Recorded,
        site: "grounding_trust::enforce, at every execute boundary",
        refuses: "a field no tool of the agent's could have supplied, and prose \
                  that restates one",
        if_never_refuses: "Either no agent has ever fabricated a field, or the \
                           contract table has no entry for the agents actually \
                           being run — `enforce` returns an empty report when an \
                           agent has no declared contract, which is indistinguishable \
                           from a clean pass at the call site.",
    },
    GateSpec {
        gate: Gate::InputBinding,
        id: "input_binding",
        // Decides about the artifact, so it can name one.
        decides_before_the_artifact: false,
        clock: Clock::Invocation,
        retention: Retention::Counted,
        site: "port_trust::bind_input, from handlers::execution{,_stream}",
        refuses: "nothing — it records a mismatch and continues, by design",
        if_never_refuses: "Expected: this gate is declared as advisory. It is here \
                           so the mismatch RATE is visible, which is the number \
                           that would justify making it fatal.",
    },
    GateSpec {
        gate: Gate::Admission,
        id: "admission",
        // Decides about the artifact, so it can name one.
        decides_before_the_artifact: false,
        clock: Clock::Admission,
        retention: Retention::Recorded,
        site: "workflows::publish_pipeline, card_contract::validate",
        refuses: "publishing an agent with an untyped or ungrounded interface",
        if_never_refuses: "Every authored card has been perfect, or the checks are \
                           advisory in practice. Note the asymmetry this was found \
                           with: the admin BYPASS of this gate is audited to \
                           `admin_bypass_events`, and the refusal was not.",
    },
    GateSpec {
        gate: Gate::Credit,
        id: "credit",
        // Decides whether the run may happen at all, so a refusal is the
        // reason there is no artifact to name.
        decides_before_the_artifact: true,
        clock: Clock::Invocation,
        retention: Retention::Counted,
        site: "handlers::execution, gas::charge_gas, handlers::agent_wallet, \
               workflows::{publish_pipeline,fork}",
        refuses: "an action whose principal cannot pay for it",
        if_never_refuses: "Nobody has ever run out of credits. Worth knowing \
                           either way: the platform charges on success and, until \
                           now, recorded nothing on refusal, so spend was \
                           observable and demand was not.",
    },
    GateSpec {
        gate: Gate::RateLimit,
        id: "rate_limit",
        // Decides whether the request is served at all, before anything
        // downstream runs.
        decides_before_the_artifact: true,
        clock: Clock::Invocation,
        retention: Retention::Counted,
        site: "api_server::rate_limit_middleware, api_server::RateLimiter::check",
        refuses: "a caller exceeding the window",
        if_never_refuses: "The limits are set above real traffic, which is fine \
                           and should be a measured statement rather than an \
                           assumption. The limiter is in-memory and per-process, \
                           so it is also the gate most likely to be quietly \
                           ineffective behind more than one replica.",
    },
    GateSpec {
        gate: Gate::Attachment,
        id: "attachment",
        // Decides about the artifact, so it can name one.
        decides_before_the_artifact: false,
        clock: Clock::Invocation,
        retention: Retention::Counted,
        site: "attachments::check, from handlers::execution",
        refuses: "a frame the selected model cannot see or the provider cannot \
                  carry",
        if_never_refuses: "No caller has sent an attachment the model could not \
                           read — or attachments are not reaching this check.",
    },
    GateSpec {
        gate: Gate::OutputSchema,
        id: "output_schema",
        // Decides about the artifact, so it can name one.
        decides_before_the_artifact: false,
        clock: Clock::Invocation,
        // Promoted to Recorded by migration 230. Gate decisions now land in
        // `gate_decisions` so fidelity — `approved / (approved + refused)` per
        // agent — is queryable from the ledger. The constraint widened in
        // migration 217 and gate_decision_reviews widened in 219 already accept
        // the token; 230 is the argument for the promotion and widens for
        // input_schema in the same migration.
        //
        // Before the promotion, `unverified_no_schema` was 98% of outcomes
        // (almost nothing declares a schema), so the counter reported nearly
        // nothing. The ledger is more honest: it records what was checked,
        // including every undetermined, so fidelity is computable from real data.
        retention: Retention::Recorded,
        site: "agent_backend::envelope::build, at every delegation hop",
        refuses: "a delegated document that contradicts the schema its own \
                  producer declared",
        if_never_refuses: "The likely reading, and the one this gate exists to \
                           distinguish from health: almost no agent declares a \
                           schema, so there is nothing to contradict. Check \
                           `undetermined` before believing `approved` — an \
                           untyped producer, a prose-only answer and a schema \
                           keyword the validator cannot evaluate all land there, \
                           and all three are the absence of a check rather than \
                           the passing of one.",
    },
    GateSpec {
        gate: Gate::InputSchema,
        id: "input_schema",
        // Fires before dispatch — the caller can be told of a mismatch
        // without spending tokens. But advisory (non-blocking) so untyped
        // callers are not broken.
        decides_before_the_artifact: true,
        clock: Clock::Invocation,
        retention: Retention::Counted,
        site: "agent_backend::envelope::validate_input, called from \
               execute_execute_agent before every delegation hop",
        refuses: "a caller query that does not conform to the callee's declared \
                  input_contract.schema — detected before dispatch, never fatal",
        if_never_refuses: "Most agents do not declare an input_contract, so \
                           no validation runs — the common case and correct. \
                           When an agent does declare one, a gate that never \
                           refuses means either every caller sends valid queries \
                           or the schema is permissive (additionalProperties: \
                           true, so extra fields pass). Check `undetermined` \
                           before believing `approved`.",
    },
    GateSpec {
        gate: Gate::Completeness,
        id: "completeness",
        // Decides about the artifact: it reads the document the agent produced.
        decides_before_the_artifact: false,
        clock: Clock::Invocation,
        // Counted, deliberately, and this is the argument.
        //
        // A counter answers the only question that matters first: how often does
        // this fire at all. `WHAT_THE_PLATFORM_CAN_REFUSE.md` §4.2 is the reason
        // it does not start Recorded — there are 7 grounding refusals on this
        // platform and 0 reviews of any of them, so nothing here has earned a
        // per-decision ledger yet, and a ledger nobody reads is the state
        // `gate_review` exists to end. Promote it together with a review door,
        // which needs a migration to widen `gate_decisions.gate`.
        retention: Retention::Counted,
        site: "episode_boundary::Pulse::assess_completeness, from the execute \
               handlers",
        refuses: "nothing — it reports a contracted field the agent left empty \
                  that no tool of its own excuses",
        if_never_refuses: "It refuses nothing by construction and the COUNTER is \
                           the finding. All zero means either every agent fills \
                           every field it was asked for, or — far more likely — \
                           the excusals are swallowing everything: an absence is \
                           excused when the contract requires null, and when the \
                           named tool was asked and had nothing. If `refused` is \
                           flat zero while trace question three shows empties, \
                           the excusal rule and the display disagree and one of \
                           them is wrong.",
    },
];

impl Gate {
    pub fn spec(self) -> &'static GateSpec {
        &GATES[self as usize]
    }
    pub fn id(self) -> &'static str {
        self.spec().id
    }
}

/// What a gate decided.
///
/// Three outcomes, not two. `Undetermined` is the one that is usually missing
/// and it is load-bearing: a gate that cannot form an opinion has neither
/// approved nor refused, and folding it into either is how "the check could not
/// run" becomes indistinguishable from a verdict. The coherence gate returns it
/// when the agent has too little world model to settle against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approved,
    Refused,
    Undetermined,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Approved => "approved",
            Decision::Refused => "refused",
            Decision::Undetermined => "undetermined",
        }
    }
}

/// The `gate_decisions.decision` vocabulary, owned here.
///
/// Registered in [`crate::seam_vocabulary`] against the column's `CHECK`. The
/// registry indexes this array rather than restating it: a second copy of a
/// closed token set is a second answer to the same question, and the copy that
/// drifts is always the one nearest the writer.
pub const DECISIONS: &[&str] = &["approved", "refused", "undetermined"];

/// The `gate_decisions.gate` vocabulary, owned here.
///
/// Derived from [`GATES`] by a test rather than by hand, so adding a gate
/// without widening the constraint fails the build instead of failing every
/// insert in a spawned task.
pub const GATE_IDS: &[&str] = &[
    "coherence",
    "grounding",
    "input_binding",
    "admission",
    "credit",
    "rate_limit",
    "attachment",
    "output_schema",
    "input_schema",
    // `Retention::Counted`, so nothing it decides is inserted and the CHECK is
    // not yet reached. Listed anyway, because `gate_ids_match_the_declared_gates`
    // derives this from GATES and the drift it prevents is one-directional: the
    // day completeness is promoted to `Recorded`, every decision it makes
    // becomes unwritable in a batch insert whose error is swallowed by design.
    // A migration widening `gate_decisions_gate_check` has to land WITH that
    // promotion, not after it.
    "completeness",
];

const N: usize = GATES.len();

#[allow(clippy::declare_interior_mutable_const)]
const ZERO: AtomicU64 = AtomicU64::new(0);
static APPROVED: [AtomicU64; N] = [ZERO; N];
static REFUSED: [AtomicU64; N] = [ZERO; N];
static UNDETERMINED: [AtomicU64; N] = [ZERO; N];

/// Last refusal reason per gate, for the endpoint.
///
/// A `Vec` sized on first write rather than a const-initialised array: the
/// array form needs an inline-const whose element type the compiler could not
/// infer through the guard, and a clever initialiser is not worth an argument
/// with type inference in a module about legibility.
static LAST_REFUSAL: Mutex<Vec<Option<String>>> = Mutex::new(Vec::new());

/// One decision waiting to reach `gate_decisions`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingDecision {
    pub gate: &'static str,
    pub decision: &'static str,
    pub reason: Option<String>,
    pub subject: Option<String>,
    /// The artifact this decision was about, when one exists.
    ///
    /// `None` for gates that fire **before** the artifact does — `credit` and
    /// `rate_limit` decide whether to run at all, and there may never be an
    /// episode. That is the correct and final answer for them rather than a
    /// backfill target.
    pub episode_id: Option<uuid::Uuid>,
    pub decided_at: chrono::DateTime<chrono::Utc>,
}

/// How many decisions may wait before the oldest are dropped.
///
/// Bounded because an unbounded audit queue behind a dead recorder is a memory
/// leak that presents as a healthy gate.
const QUEUE_CAP: usize = 4096;

static QUEUE: Mutex<Vec<PendingDecision>> = Mutex::new(Vec::new());

/// Decisions dropped because the queue was full.
///
/// The number that makes the bound honest. A ledger that silently loses rows is
/// worse than no ledger, because it reads as complete.
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// Record a decision. Never fails, never blocks, no I/O.
///
/// For [`Retention::Recorded`] gates it additionally enqueues a row for
/// `gate_decisions`. The enqueue is a `Vec` push under a mutex — no await, no
/// syscall — so the promise in this function's first line survives.
pub fn decided(gate: Gate, decision: Decision, reason: Option<&str>) {
    decided_about(gate, decision, reason, None)
}

/// [`decided`], plus what the decision was about.
///
/// Separate rather than a fourth parameter on `decided` because every existing
/// call site passes no subject, and widening the common signature to serve the
/// rarer case would have meant touching all of them to add `None`.
pub fn decided_about(gate: Gate, decision: Decision, reason: Option<&str>, subject: Option<&str>) {
    decided_full(gate, decision, reason, subject, None)
}

/// [`decided_about`], plus the artifact the decision was about.
///
/// A third entry point rather than a fifth parameter on the common one, for the
/// reason [`decided_about`] gives about the fourth: every existing call site
/// passes no episode, and widening the common signature to serve the rarer case
/// means touching all of them to add `None`.
///
/// Only gates that fire **after** the artifact exists can pass one. `credit` and
/// `rate_limit` decide whether to run at all, so their `episode_id` is
/// permanently `None` — see migration 220 on why that is final rather than a gap.
///
/// # `subject` is required here, and that is the point
///
/// It was `None`, hardcoded, and the consequence was measurable: all 42
/// `grounding` rows in production carry a null subject, so the ledger cannot
/// say **which agent** any of them was about. The review door
/// (`gate_api::GATE_DOORS`) hands a reviewer a decision and asks whether it was
/// right; an anonymous refusal cannot be judged, and `gate_decision_reviews`
/// has zero rows.
///
/// So it is a required `&str` rather than an `Option<&str>` widened onto the
/// signature. A decision that reached the artifact stage always knows what it
/// was about — both call sites had the slug in scope and were passing `None`
/// past it — and an invariant the type system holds does not need a test to
/// notice when it stops being true. This is the one entry point where
/// anonymity was never defensible: `decided` and `decided_about` serve gates
/// that fire before there is a subject to name.
pub fn decided_for_episode(
    gate: Gate,
    decision: Decision,
    reason: Option<&str>,
    subject: &str,
    episode_id: uuid::Uuid,
) {
    decided_full(gate, decision, reason, Some(subject), Some(episode_id))
}

fn decided_full(
    gate: Gate,
    decision: Decision,
    reason: Option<&str>,
    subject: Option<&str>,
    episode_id: Option<uuid::Uuid>,
) {
    let i = gate as usize;
    match decision {
        Decision::Approved => &APPROVED[i],
        Decision::Refused => &REFUSED[i],
        Decision::Undetermined => &UNDETERMINED[i],
    }
    .fetch_add(1, Ordering::Relaxed);

    if gate.spec().retention == Retention::Recorded {
        enqueue(PendingDecision {
            gate: gate.id(),
            decision: decision.as_str(),
            // Approvals carry no reason: one per pass would make the table
            // mostly noise, and the question this ledger answers is what was
            // refused.
            reason: (decision != Decision::Approved)
                .then(|| reason.map(|r| r.chars().take(400).collect::<String>()))
                .flatten(),
            subject: subject.map(|s| s.chars().take(200).collect()),
            episode_id,
            decided_at: chrono::Utc::now(),
        });
    }

    if decision != Decision::Refused {
        return;
    }
    if let (Some(r), Ok(mut guard)) = (reason, LAST_REFUSAL.lock()) {
        if guard.len() < N {
            guard.resize(N, None);
        }
        guard[i] = Some(r.chars().take(400).collect());
    }
}

fn enqueue(d: PendingDecision) {
    let Ok(mut q) = QUEUE.lock() else {
        // A poisoned mutex means a writer panicked mid-push. Counting the loss
        // is the whole point of the counter; taking the panic with it is not.
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    };
    if q.len() >= QUEUE_CAP {
        // Drop the OLDEST. A full queue means the recorder is behind or dead,
        // and in that state the recent refusals are the ones someone is about
        // to go looking for.
        q.remove(0);
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    q.push(d);
}

/// Take everything queued. Called by the recorder.
pub fn drain() -> Vec<PendingDecision> {
    match QUEUE.lock() {
        Ok(mut q) => std::mem::take(&mut *q),
        Err(_) => Vec::new(),
    }
}

/// Put decisions back after a failed flush, oldest first.
///
/// Without this a transient database error loses the batch silently, which is
/// the failure this whole table exists to stop.
pub fn requeue(mut batch: Vec<PendingDecision>) {
    let Ok(mut q) = QUEUE.lock() else {
        DROPPED.fetch_add(batch.len() as u64, Ordering::Relaxed);
        return;
    };
    let overflow = (batch.len() + q.len()).saturating_sub(QUEUE_CAP);
    if overflow > 0 {
        batch.drain(..overflow.min(batch.len()));
        DROPPED.fetch_add(overflow as u64, Ordering::Relaxed);
    }
    batch.append(&mut q);
    *q = batch;
}

/// What the durable half of the ledger can currently claim.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LedgerStatus {
    /// Decisions enqueued and not yet written. Not durable.
    pub pending: usize,
    /// Decisions lost to a full queue. Should be zero.
    pub dropped: u64,
    /// Gates whose decisions are meant to reach `gate_decisions`.
    pub recorded_gates: Vec<&'static str>,
    /// Gates that are counted in memory only, and vanish on restart.
    pub counted_only_gates: Vec<&'static str>,
}

/// The honest description of what survives a restart.
///
/// Every surface reporting gate counters must be able to say `since boot`, and
/// this is where it learns that it has to.
pub fn ledger_status() -> LedgerStatus {
    LedgerStatus {
        pending: QUEUE.lock().map(|q| q.len()).unwrap_or(0),
        dropped: DROPPED.load(Ordering::Relaxed),
        recorded_gates: GATES
            .iter()
            .filter(|g| g.retention == Retention::Recorded)
            .map(|g| g.id)
            .collect(),
        counted_only_gates: GATES
            .iter()
            .filter(|g| g.retention == Retention::Counted)
            .map(|g| g.id)
            .collect(),
    }
}

/// Convenience for the common `Result`-shaped gate.
pub fn decided_ok<T, E: std::fmt::Display>(gate: Gate, r: &Result<T, E>) {
    match r {
        Ok(_) => decided(gate, Decision::Approved, None),
        Err(e) => decided(gate, Decision::Refused, Some(&e.to_string())),
    }
}

/// One gate's running totals.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GateAccount {
    pub id: &'static str,
    pub clock: Clock,
    pub retention: Retention,
    pub site: &'static str,
    pub approved: u64,
    pub refused: u64,
    pub undetermined: u64,
    pub last_refusal: Option<String>,
    /// `never_asked` | `refuses_everything` | `admits_everything` | `null`
    pub reading: Option<&'static str>,
    /// Echoed so the reading is actionable without a second lookup.
    pub if_never_refuses: &'static str,
}

impl GateAccount {
    pub fn asked(&self) -> u64 {
        self.approved + self.refused + self.undetermined
    }
    /// Never exercised. Not a pass — the same rule as liveness's `Inert`.
    pub fn never_asked(&self) -> bool {
        self.asked() == 0
    }
    /// Asked, and has approved nothing. The Γ bug's signature.
    pub fn refuses_everything(&self) -> bool {
        self.asked() > 0 && self.approved == 0
    }
    /// Asked, and has refused nothing.
    ///
    /// Reported, never asserted. A gate legitimately refuses nothing when
    /// nothing has warranted refusal, and asserting otherwise would assert that
    /// violations must exist — the same error as asserting on `anomaly_events`'
    /// row count. What makes it worth surfacing is that it is otherwise
    /// invisible: a control that never fires and a control that is not wired
    /// produce identical observations everywhere else.
    pub fn admits_everything(&self) -> bool {
        self.asked() > 0 && self.refused == 0
    }
}

fn reading_for(a: &GateAccount) -> Option<&'static str> {
    if a.never_asked() {
        Some("never_asked")
    } else if a.refuses_everything() {
        Some("refuses_everything")
    } else if a.admits_everything() {
        Some("admits_everything")
    } else {
        None
    }
}

pub fn account(gate: Gate) -> GateAccount {
    let i = gate as usize;
    let spec = gate.spec();
    let mut a = GateAccount {
        id: spec.id,
        clock: spec.clock,
        retention: spec.retention,
        site: spec.site,
        approved: APPROVED[i].load(Ordering::Relaxed),
        refused: REFUSED[i].load(Ordering::Relaxed),
        undetermined: UNDETERMINED[i].load(Ordering::Relaxed),
        last_refusal: LAST_REFUSAL
            .lock()
            .ok()
            .and_then(|g| g.get(i).cloned().flatten()),
        reading: None,
        if_never_refuses: spec.if_never_refuses,
    };
    a.reading = reading_for(&a);
    a
}

pub fn accounts() -> Vec<GateAccount> {
    GATES.iter().map(|g| account(g.gate)).collect()
}

/// Gates that have been asked and have approved nothing.
///
/// The assertable finding. Unlike `admits_everything`, this one carries no
/// claim about the world: it says the gate ran and let nothing through, which
/// is either a correct refusal of every single input or a broken control, and
/// both deserve a look.
pub fn refusing_everything() -> Vec<&'static str> {
    accounts()
        .into_iter()
        .filter(GateAccount::refuses_everything)
        .map(|a| a.id)
        .collect()
}

// ── The recorder ───────────────────────────────────────────────────────

/// Write one batch to `gate_decisions`. Returns how many rows landed.
///
/// Separate from the loop so it is callable from a test with a real pool and
/// no timer. On failure the batch is requeued rather than dropped: a transient
/// database error losing the audit trail is the failure this table exists to
/// stop.
pub async fn flush(pool: &sqlx::PgPool) -> usize {
    let batch = drain();
    if batch.is_empty() {
        return 0;
    }

    let gates: Vec<String> = batch.iter().map(|d| d.gate.to_string()).collect();
    let decisions: Vec<String> = batch.iter().map(|d| d.decision.to_string()).collect();
    let reasons: Vec<Option<String>> = batch.iter().map(|d| d.reason.clone()).collect();
    let subjects: Vec<Option<String>> = batch.iter().map(|d| d.subject.clone()).collect();
    let episodes: Vec<Option<uuid::Uuid>> = batch.iter().map(|d| d.episode_id).collect();
    let decided: Vec<chrono::DateTime<chrono::Utc>> = batch.iter().map(|d| d.decided_at).collect();

    // One statement for the batch. `UNNEST` rather than a loop because a
    // recorder that issues one round trip per refusal becomes a cost of
    // refusing, and a gate whose expense scales with how often it says no is a
    // gate under pressure to say yes.
    let result = sqlx::query(
        "INSERT INTO gate_decisions (gate, decision, reason, subject, episode_id, decided_at)
         SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::uuid[], \
                              $6::timestamptz[])",
    )
    .bind(&gates)
    .bind(&decisions)
    .bind(&reasons)
    .bind(&subjects)
    .bind(&episodes)
    .bind(&decided)
    .execute(pool)
    .await;

    let n = batch.len();
    match crate::write_accounting::observe(crate::write_accounting::Sink::GateDecisions, result) {
        Some(_) => n,
        None => {
            requeue(batch);
            0
        }
    }
}

/// Drain the queue into `gate_decisions` on a timer.
///
/// `GATE_LEDGER_FLUSH_SECS=0` disables it, matching the other sweepers. Note
/// what disabling costs: the counters keep working and nothing survives a
/// restart, which is the state the platform was in before migration 214.
pub fn spawn_gate_recorder(db: sqlx::PgPool) {
    const DEFAULT_FLUSH_SECS: u64 = 15;

    let secs = std::env::var("GATE_LEDGER_FLUSH_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_FLUSH_SECS);

    if secs == 0 {
        println!(
            "[gates] decision ledger disabled (GATE_LEDGER_FLUSH_SECS=0) — counters are \
             in-memory and will not survive a restart"
        );
        return;
    }
    println!("[gates] flushing the decision ledger every {secs}s");

    tokio::spawn(async move {
        // Past boot, so the migration that creates the table has run.
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        loop {
            flush(&db).await;

            // Loud only when it is actionable. A dropped decision is a hole in
            // the audit trail and there is no second copy of it anywhere.
            let s = ledger_status();
            if s.dropped > 0 {
                eprintln!(
                    "[gates] {} decision(s) DROPPED — the queue filled because the recorder \
                     could not write. Those refusals are not recoverable.",
                    s.dropped
                );
            }
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
        }
    });
}

#[cfg(test)]
fn reset() {
    for i in 0..N {
        APPROVED[i].store(0, Ordering::Relaxed);
        REFUSED[i].store(0, Ordering::Relaxed);
        UNDETERMINED[i].store(0, Ordering::Relaxed);
    }
    if let Ok(mut g) = LAST_REFUSAL.lock() {
        g.clear();
    }
    if let Ok(mut q) = QUEUE.lock() {
        q.clear();
    }
    DROPPED.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    static SERIAL: Mutex<()> = Mutex::new(());
    fn exclusive() -> std::sync::MutexGuard<'static, ()> {
        let g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        g
    }

    #[test]
    fn gates_are_indexed_by_discriminant() {
        for (i, spec) in GATES.iter().enumerate() {
            assert_eq!(spec.gate as usize, i, "{} is at the wrong index", spec.id);
        }
    }

    /// `GATE_IDS` is the Postgres CHECK's twin and must be derivable from GATES.
    ///
    /// Adding a gate without widening `gate_decisions_gate_check` makes every
    /// decision by the new gate unwritable, in a batch insert whose error is
    /// swallowed by design. This is the cheap half of that check; the seam
    /// contract holds the Postgres half.
    #[test]
    fn gate_ids_match_the_declared_gates() {
        let from_gates: Vec<&str> = GATES.iter().map(|g| g.id).collect();
        assert_eq!(
            GATE_IDS,
            &from_gates[..],
            "GATE_IDS has drifted from GATES. Update it AND widen the CHECK in \
             a migration — the constant alone will not make the insert land."
        );
    }

    #[test]
    fn the_decision_vocabulary_covers_every_variant() {
        for d in [
            Decision::Approved,
            Decision::Refused,
            Decision::Undetermined,
        ] {
            assert!(
                DECISIONS.contains(&d.as_str()),
                "{} is a Decision the column would reject",
                d.as_str()
            );
        }
        assert_eq!(
            DECISIONS.len(),
            3,
            "three outcomes, and undetermined is one"
        );
    }

    /// Only `Recorded` gates enqueue, and approvals carry no reason.
    #[test]
    fn recorded_gates_enqueue_and_counted_ones_do_not() {
        let _g = exclusive();

        // Counted-tier: counts, does not enqueue.
        decided(Gate::RateLimit, Decision::Refused, Some("too fast"));
        assert_eq!(ledger_status().pending, 0);
        assert_eq!(account(Gate::RateLimit).refused, 1);

        // Recorded-tier: both.
        decided(
            Gate::Coherence,
            Decision::Refused,
            Some("world model rejects"),
        );
        assert_eq!(ledger_status().pending, 1);

        decided(Gate::Admission, Decision::Approved, Some("ignored"));
        let q = drain();
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].gate, "coherence");
        assert_eq!(q[0].decision, "refused");
        assert_eq!(q[0].reason.as_deref(), Some("world model rejects"));
        assert_eq!(q[1].decision, "approved");
        assert_eq!(
            q[1].reason, None,
            "an approval reason would make the table mostly noise"
        );
        assert_eq!(ledger_status().pending, 0, "drain empties the queue");
    }

    /// **A decision must reach the queue naming its agent and its fault.**
    ///
    /// This is the test whose absence produced the defect. `gate_decisions`
    /// held 42 grounding rows in production; every one had a null `subject`
    /// and every refusal had the reason `1 violation(s)`. Nothing was wrong
    /// with the queue, the flush, or the table — the writer simply never
    /// passed what it knew, and no test looked at what arrived, so the column
    /// `gate_api::LEDGER_SQL` selects had been null for the life of the ledger.
    ///
    /// It goes through `Pulse::grade` rather than calling `decided_full`
    /// directly, because the two halves being checked are the ones the real
    /// writer supplies: the agent slug it was grading against, and
    /// `Report::refusal_reason` rather than a count. A unit test on
    /// `refusal_reason` alone would still pass if `grade` threw the value away.
    ///
    /// Two `Unsourced` fields are filled, so this also exercises the grouping
    /// end to end: one `ungrounded_field` clause carrying both paths, which is
    /// what keeps a many-field refusal inside the 400-character truncation.
    #[test]
    fn an_episode_decision_reaches_the_queue_naming_its_agent_and_its_fault() {
        let _g = exclusive();

        // `genome.ploidy` and `conservation.iucn_status` are both declared
        // Unsourced for this agent: no tool of its own could supply either, so
        // a value in them is fabricated by construction. Two of them, so the
        // grouping has something to group.
        let raw = serde_json::json!({
            "genome": { "ploidy": "diploid" },
            "conservation": { "iucn_status": "Least Concern" }
        })
        .to_string();

        let pulse = crate::episode_boundary::Pulse::after_the_fact(
            uuid::Uuid::nil(),
            "a gate-ledger fixture",
        );
        let _graded = pulse.grade("genome_profiler", None, Some(&raw));

        let queued = drain();
        let rows: Vec<&PendingDecision> =
            queued.iter().filter(|d| d.gate == "grounding").collect();
        assert_eq!(
            rows.len(),
            1,
            "one graded document is one grounding decision; got {:?}",
            queued.iter().map(|d| d.gate).collect::<Vec<_>>()
        );
        let row = rows[0];

        assert_eq!(row.decision, "refused", "two fabricated fields is a refusal");

        assert_eq!(
            row.subject.as_deref(),
            Some("genome_profiler"),
            "the decision reached the ledger without naming the agent it was \
             about. That is the state all 42 production rows are in, and it is \
             what makes them unreviewable: `gate_api::LEDGER_SQL` selects this \
             column so a reviewer can see whose refusal they are judging."
        );

        let reason = row
            .reason
            .as_deref()
            .expect("a refusal must carry a reason a reviewer can read");
        assert!(
            reason.contains("genome.ploidy") && reason.contains("conservation.iucn_status"),
            "the reason does not name both stripped fields, so the ledger \
             under-reports what was refused: {reason:?}"
        );
        assert!(
            !reason.contains("violation(s)"),
            "the writer is counting what it refused again instead of naming it. \
             `1 violation(s)` was the reason on every refusal in the ledger and \
             the reason the review door went unused: {reason:?}"
        );
        assert_eq!(
            reason.matches("ungrounded_field").count(),
            1,
            "both faults are the same kind and must group into one clause, or a \
             document failing a dozen fields loses its last paths to the \
             400-character truncation: {reason:?}"
        );

        assert!(
            row.episode_id.is_some(),
            "a decision about an artifact must point at the artifact, or a \
             reviewer cannot reach the document the refusal was about"
        );
    }

    /// A full queue must lose the OLDEST and say so.
    ///
    /// A bounded queue that drops silently is worse than an unbounded one: the
    /// ledger reads as complete while holding a hole.
    #[test]
    fn a_full_queue_drops_the_oldest_and_counts_it() {
        let _g = exclusive();

        for i in 0..QUEUE_CAP + 10 {
            decided(Gate::Coherence, Decision::Refused, Some(&format!("r{i}")));
        }
        let s = ledger_status();
        assert_eq!(s.pending, QUEUE_CAP, "the bound holds");
        assert_eq!(s.dropped, 10, "and every loss is counted");

        let q = drain();
        assert_eq!(
            q[0].reason.as_deref(),
            Some("r10"),
            "the oldest went; the recent refusals someone is about to look for stayed"
        );
    }

    /// A failed flush must put the batch back, not lose it.
    #[test]
    fn requeue_preserves_order_and_bounds() {
        let _g = exclusive();

        decided(Gate::Coherence, Decision::Refused, Some("first"));
        decided(Gate::Coherence, Decision::Refused, Some("second"));
        let batch = drain();
        assert_eq!(batch.len(), 2);

        // A decision arrives while the flush is in flight.
        decided(Gate::Coherence, Decision::Refused, Some("third"));
        requeue(batch);

        let q = drain();
        let reasons: Vec<_> = q.iter().map(|d| d.reason.as_deref().unwrap()).collect();
        assert_eq!(
            reasons,
            vec!["first", "second", "third"],
            "the requeued batch goes back in front, in order"
        );
        assert_eq!(ledger_status().dropped, 0);
    }

    /// The status must name what does not survive a restart.
    #[test]
    fn the_ledger_says_which_gates_are_memory_only() {
        let s = ledger_status();
        // `grounding` joined the ledger in migration 221.
        // `output_schema` joined the ledger in migration 230 (promoted from
        // Counted to Recorded so fidelity is queryable per agent).
        // The pin is updated rather than relaxed: this list is the claim
        // `GateView::since` makes to a reader -- "these counters survive a
        // restart" -- and it must move only when somebody means it to.
        assert_eq!(
            s.recorded_gates,
            vec!["coherence", "grounding", "admission", "output_schema"]
        );
        assert!(
            s.counted_only_gates.contains(&"rate_limit"),
            "a surface reporting rate-limit counters must be able to say `since boot`"
        );
        assert_eq!(
            s.recorded_gates.len() + s.counted_only_gates.len(),
            GATES.len(),
            "every gate is in exactly one tier"
        );
    }

    #[test]
    fn every_gate_says_what_silence_would_mean() {
        let mut ids = std::collections::HashSet::new();
        for g in GATES {
            assert!(ids.insert(g.id), "duplicate gate id `{}`", g.id);
            assert!(
                g.if_never_refuses.len() > 60,
                "{}: a gate that refuses nothing is only actionable if someone \
                 wrote down what that would mean",
                g.id
            );
            assert!(g.site.contains("::"), "{}: `site` must name code", g.id);
        }
    }

    /// The bug this module exists for, as a decision table.
    #[test]
    fn the_three_readings_are_distinguishable() {
        let _g = exclusive();

        // Never exercised. Not a pass, and not a fault either.
        let a = account(Gate::Attachment);
        assert!(a.never_asked());
        assert_eq!(a.reading, Some("never_asked"));
        assert!(
            !a.refuses_everything(),
            "an unasked gate must not read as broken"
        );

        // The Γ bug: asked repeatedly, approved nothing.
        for _ in 0..47 {
            decided(Gate::Coherence, Decision::Refused, Some("gamma 0.01 < 0.5"));
        }
        let c = account(Gate::Coherence);
        assert_eq!((c.approved, c.refused, c.asked()), (0, 47, 47));
        assert!(c.refuses_everything());
        assert_eq!(c.reading, Some("refuses_everything"));
        assert_eq!(refusing_everything(), vec!["coherence"]);

        // The inverse, and the one nobody checks: a control that never fires.
        for _ in 0..500 {
            decided(Gate::Grounding, Decision::Approved, None);
        }
        let gr = account(Gate::Grounding);
        assert!(gr.admits_everything());
        assert_eq!(gr.reading, Some("admits_everything"));
        assert!(!gr.refuses_everything());
    }

    /// `Undetermined` is neither, and must not be laundered into either.
    #[test]
    fn undetermined_is_not_an_approval() {
        let _g = exclusive();
        decided(Gate::Coherence, Decision::Undetermined, None);
        let a = account(Gate::Coherence);
        assert_eq!((a.approved, a.refused, a.undetermined), (0, 0, 1));
        // It counts as having been asked — the gate ran — but it approved
        // nothing, so the strong reading holds and the gate is not silently
        // credited with a pass it did not give.
        assert_eq!(a.asked(), 1);
        assert!(a.refuses_everything());
    }

    #[test]
    fn a_mixed_gate_gets_no_reading() {
        let _g = exclusive();
        decided(Gate::Credit, Decision::Approved, None);
        decided(Gate::Credit, Decision::Refused, Some("need 5, have 0"));
        let a = account(Gate::Credit);
        assert_eq!(a.reading, None);
        assert_eq!(a.last_refusal.as_deref(), Some("need 5, have 0"));
    }

    #[test]
    fn decided_ok_maps_result_to_decision() {
        let _g = exclusive();
        decided_ok(Gate::Admission, &Ok::<_, String>(()));
        decided_ok(Gate::Admission, &Err::<(), _>("failing checks".to_string()));
        let a = account(Gate::Admission);
        assert_eq!((a.approved, a.refused), (1, 1));
    }

    #[test]
    fn counters_do_not_bleed_between_gates() {
        let _g = exclusive();
        decided(Gate::RateLimit, Decision::Refused, Some("429"));
        assert_eq!(account(Gate::RateLimit).refused, 1);
        assert_eq!(account(Gate::Credit).refused, 0);
    }
}
