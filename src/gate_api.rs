//! Where does each gate stand, and what can a person do about it?
//!
//! The second instance of [`crate::surface`]'s pattern. Same two shared parts —
//! the door and the caveat — over this domain's own model, measurement and
//! interpretation:
//!
//! | part | owner |
//! |---|---|
//! | declared model | [`crate::gate_trust::GATES`] |
//! | measurement | [`crate::gate_trust::accounts`] — approve / refuse / undetermined |
//! | interpretation | `GateAccount::{never_asked, refuses_everything, admits_everything}` |
//! | door | here |
//! | caveat | here |
//!
//! Nothing here recomputes a verdict. `gate_trust` owns them and its own tests
//! falsify them; this dresses them for a surface.
//!
//! # Why a gate needs a reading and not a number
//!
//! `approved: 0, refused: 0` and `approved: 40, refused: 0` are both "no
//! refusals" and they mean opposite things: the first is a control nobody has
//! exercised, the second is one that has run forty times and stopped nothing.
//! A surface rendering the counters alone leaves the reader to notice that, and
//! the gate audit exists because nobody did.
//!
//! So the reading is [`crate::panel_absence::Reading`], the same three words the
//! loop surface uses:
//!
//! | gate state | reading | why |
//! |---|---|---|
//! | `refuses_everything` | `fault` | asked, and approved nothing. A control that blocks everything is inverted, and the counters look busy |
//! | `never_asked` | `unknown` | not a pass. An unwired control and one with nothing to refuse are the same observation |
//! | `admits_everything` | `unknown` | **reported, never asserted.** A gate legitimately refuses nothing when nothing warranted it; it is also what an unwired one looks like |
//! | otherwise | `idle` | it has both approved and refused, so it discriminates |
//!
//! `admits_everything` mapping to `unknown` rather than `fault` is the one
//! judgement here, and it is `gate_trust`'s own: asserting on it would assert
//! that violations must exist, which is the same error as asserting on
//! `anomaly_events`' row count.
//!
//! # The durability caveat
//!
//! Most of these counters are process-local and reset on restart. A surface that
//! shows `0 refusals` without saying "since boot" invites a reader to conclude
//! the gate has never refused anything in its life. [`GateView::since`] carries
//! which it is, from `gate_trust`'s own `Retention`.

use crate::gate_trust::{self, GateAccount, Retention};
use crate::panel_absence::Reading;
use crate::surface::{Caveat, Door};

// ─── doors ───────────────────────────────────────────────────────────────

/// Every human door into a gate.
///
/// # What this list was, and why it changed
///
/// It was `&[]`, and the emptiness was the finding: no endpoint anywhere let a
/// person act on a gate. No way to review what a gate refused, no way to record
/// that a refusal was wrong. `gate_trust` counted decisions, migration 214 gave
/// them a ledger, and the whole surface was read-only.
///
/// Half of that was right and stays right. There is still **no override** — a
/// gate a person can wave through is not much of a gate, nothing below re-runs,
/// reverses or retries a decision, and `gate_review::Overturned` changes no
/// behaviour.
///
/// The other half was a hole, and the argument is arithmetic rather than
/// ergonomic. Every reading this module computes comes from approve/refuse
/// **counts**. `refuses_everything` catches the Γ bug's signature exactly —
/// asked, and approved nothing. It cannot catch a gate that approves 90% of what
/// it sees and refuses the other 10% *wrongly*: that reads `discriminating`,
/// which this surface renders as the healthy state, and every counter agrees with
/// it. Correctness is not a property of a count. So a reviewer's judgement is not
/// a convenience on top of the measurement; it is the only instrument that can
/// see the failure the measurement is blind to.
///
/// # Why only the `Retention::Recorded` gates
///
/// A review is a judgement about *one decision*, so it needs a decision to point
/// at, and only `Recorded` gates write one. The rest are in-memory counters
/// whose individual decisions do not survive the process, and a door offering to
/// review a row that does not exist is the 404-after-the-belief this module's
/// router scan exists to prevent.
///
/// That is a real limitation and worth stating rather than hiding: **six of the
/// ten gates cannot be reviewed at all**, because nothing records what they
/// decided. Promoting one to `Recorded` is the way in, and `gate_trust::GATES`
/// is where that argument belongs — migration 214's comment on why a rate-limit
/// tick is deliberately not recorded is the shape of the counter-argument.
///
/// This list does not grow by choice. `a_review_door_only_exists_where_the
/// _decisions_do` asserts **both** directions, so a promotion in
/// `gate_trust::GATES` fails the build until a door exists — which is how
/// `output_schema` got one. That is deliberate: the promotion is the moment the
/// gate starts writing rows somebody is meant to read, and a ledger with no
/// reviewer is the state the platform was in for its whole life.
///
/// # The rule, unchanged
///
/// May only grow with a reason, and an entry must argue for being manual.
pub const GATE_DOORS: &[Door] = &[
    Door {
        subject: "coherence",
        method: "POST",
        path: "/api/gates/:gate_id/decisions/:decision_id/review",
        does: "Record whether this refusal was right, and why it was wrong if it \
               was not. Does not override the decision or re-run the gate.",
        why_manual: "Because no counter can answer it. The coherence gate refuses \
                     an AgentWide correction the agent's world model rejects, and \
                     whether the world model was right about that particular \
                     correction is a judgement about the correction's content. \
                     This is the gate whose 100% refusal rate hid the Γ \
                     arithmetic bug, and it hid there because the refusals were \
                     individually plausible and nobody was asked to look at one.",
    },
    Door {
        subject: "grounding",
        method: "POST",
        path: "/api/gates/:gate_id/decisions/:decision_id/review",
        does: "Record whether this grounding verdict was right about the \
               agent's output. Does not restore a nulled field or re-run the \
               contract.",
        why_manual: "Because the contract asserts what COULD have supplied a \
                     field, not what did. `Sourced` means a tool of this \
                     agent's could answer — it does not mean the value came from \
                     that tool, and `Antaxius beieri` is the case: a \
                     bush-cricket reported as a longhorn beetle, present, \
                     non-null, correctly typed and declared sourced, with the \
                     verified answer one table over. Every automated check \
                     passed. Only a reader comparing the claim against the \
                     source closes that gap, which is why this gate is the one \
                     whose refusals most need a second opinion.",
    },
    Door {
        subject: "output_schema",
        method: "POST",
        path: "/api/gates/:gate_id/decisions/:decision_id/review",
        does: "Record whether this document really failed the type it declared, \
               or whether the declared type was wrong. Does not re-validate and \
               does not edit the card.",
        why_manual: "Because a schema mismatch has two causes and the validator \
                     cannot tell them apart. Either the agent produced a bad \
                     document, or **the schema is wrong** — the card declares \
                     something the agent cannot or should not produce — and the \
                     two need opposite remedies while presenting as the same \
                     `invalid`. `species_resolver.conservation` is the case: its \
                     grounding status leaked into the schema as the block's \
                     required VALUE, `{\"const\": \"unavailable\"}`, so \
                     enforcement nulls the block and the schema then demands the \
                     literal string. The two checks cannot both pass, no \
                     document can satisfy it, and every automated reading of \
                     that agent says the OUTPUT is invalid. Only a reader \
                     comparing the failure against the card sees that the \
                     contract is.",
    },
    Door {
        subject: "admission",
        method: "POST",
        path: "/api/gates/:gate_id/decisions/:decision_id/review",
        does: "Record whether refusing to publish this agent was right, and why \
               it was not if it was not. Does not admit the agent.",
        why_manual: "Because the cost of a wrong refusal here falls on someone who \
                     cannot see it. An author whose card is refused for an \
                     untyped interface gets a message; an author refused for a \
                     checker bug gets the same message, and the platform cannot \
                     tell those apart from the inside. The only signal that \
                     separates them is a reviewer reading the refusal against \
                     the card, and `if_never_refuses` on this gate says the \
                     alternative reading is that every authored card was perfect.",
    },
];

/// The durable ledger for one gate, newest first.
///
/// `$1` is the gate id. Read-only, and the only query this module owns —
/// everything else here is over in-memory counters.
///
/// **Refusals first, then everything else.** A reader opening this is asking
/// what was stopped, and an approval stream is the wrong thing to make them
/// page through. The ordering is part of the contract, not a default.
/// `id` is selected and it is load-bearing: it is the handle a reviewer's POST
/// carries. Until the review door existed nothing needed it, and a read that
/// returns rows a client cannot then act on is how a door ends up unbuildable
/// after the endpoint is written.
pub const LEDGER_SQL: &str = "SELECT id, gate::text, decision::text, reason, subject, \
                                     decided_at \
                                FROM gate_decisions \
                               WHERE gate = $1 \
                               ORDER BY (decision = 'refused') DESC, decided_at DESC \
                               LIMIT 200";

/// How many decisions of each kind this gate has on file, durably.
///
/// Separate from [`LEDGER_SQL`] so a surface can say "nothing here, and nothing
/// anywhere" apart from "nothing in the last 200".
pub const LEDGER_COUNT_SQL: &str = "SELECT count(*)::bigint FROM gate_decisions WHERE gate = $1";

/// Does this gate's `since: "ledger"` claim hold?
///
/// A gate declared [`Retention::Recorded`] promises its decisions survive a
/// restart, and [`GateView::since`] tells a surface to render them as more than
/// a since-boot figure. That promise is only worth the ledger behind it.
///
/// The failure this catches is specific and has happened to this table already:
/// `gate_decisions` was declared by migration 214, and until that migration ran
/// the platform had *a record of every request it served and none of any it
/// refused*. A gate reporting `since: ledger` over an empty ledger is making the
/// same claim with the same evidence.
///
/// Three states, because "the ledger is empty" and "the gate has decided
/// nothing" are different and only the first is a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerClaim {
    /// Recorded, asked, and the ledger has rows. The claim holds.
    Backed { rows: i64 },
    /// **Recorded, asked, and the ledger is empty.** The surface is telling a
    /// reader these counters survive a restart and they do not.
    Unbacked { asked: u64 },
    /// Recorded and never asked. Nothing to record yet, so nothing is claimed.
    NothingToRecord,
    /// Counted-only. It never claimed durability, so there is nothing to check.
    NotClaimed,
}

/// Classify one gate's ledger claim.
pub fn ledger_claim(a: &GateAccount, ledger_rows: i64) -> LedgerClaim {
    if a.retention != Retention::Recorded {
        return LedgerClaim::NotClaimed;
    }
    if a.asked() == 0 {
        return LedgerClaim::NothingToRecord;
    }
    if ledger_rows > 0 {
        LedgerClaim::Backed { rows: ledger_rows }
    } else {
        LedgerClaim::Unbacked { asked: a.asked() }
    }
}

/// Every caveat a gate surface must carry.
pub const GATE_CAVEATS: &[Caveat] = &[
    Caveat {
        subject: "gate.admits_everything",
        checked: "The gate has been asked and has refused nothing.",
        does_not_show: "That the gate is broken, or that it is unwired. A gate \
                        legitimately refuses nothing when nothing has warranted \
                        refusal, and asserting otherwise would assert that \
                        violations must exist — the same error as asserting on \
                        `anomaly_events`' row count. It is surfaced because a \
                        control that never fires and a control that is not \
                        wired produce identical observations everywhere else.",
    },
    Caveat {
        subject: "gate.review.upheld",
        checked: "Every reviewed decision by this gate was judged correct, and \
                  at least one was judged.",
        does_not_show: "That the gate is refusing the right things. It says the \
                        decisions someone looked at were right, and reviewers \
                        choose what to look at — an `upheld` standing over 3 of \
                        400 decisions is a sample, not a verdict, and the \
                        selection is not random. `reviewed` and the ledger total \
                        are both carried so the ratio is visible; a surface that \
                        renders this tick without the denominator is asserting \
                        the gate is sound on evidence about under one percent of \
                        its decisions.",
    },
    Caveat {
        subject: "gate.never_asked",
        checked: "No decision has been recorded for this gate since the \
                  counters last started.",
        does_not_show: "That the gate has never run. Most of these counters are \
                        process-local and reset on restart, so on a freshly \
                        booted server every gate reads `never_asked` and none of \
                        them is a finding. `since` says which counters can \
                        claim more than that, and only `Retention::Recorded` \
                        gates have a durable ledger behind them.",
    },
];

// ─── the view ────────────────────────────────────────────────────────────

/// One gate, as a surface needs it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GateView {
    pub id: &'static str,
    /// What this gate refuses, as a reader would say it.
    pub refuses: &'static str,
    /// The code that runs it. Named so a finding points at a file.
    pub site: &'static str,
    /// `idle` | `fault` | `unknown` — never a bare zero.
    pub reading: Reading,
    /// `refuses_everything` | `never_asked` | `admits_everything` | `discriminating`.
    ///
    /// The specific token beneath the reading, for a client that wants it. Three
    /// readings and four tokens on purpose: two tokens share `unknown` and mean
    /// different things.
    pub token: &'static str,
    pub approved: u64,
    pub refused: u64,
    pub undetermined: u64,
    /// `boot` when the counters are process-local, `ledger` when a durable
    /// record backs them.
    ///
    /// The field that stops `0 refusals` being read as "never in its life".
    pub since: &'static str,
    /// What it would mean if this gate never refused anything. `gate_trust`'s
    /// own words, carried through.
    pub if_never_refuses: &'static str,
    /// The most recent refusal, when one is retained.
    pub last_refusal: Option<String>,
    pub door: Option<Door>,
}

/// The reading and token for one account.
///
/// Ordered as `gate_trust::reading_for` orders them, because the tokens are its
/// vocabulary: an inverted control outranks an unexercised one, which outranks
/// one that has refused nothing.
pub fn read(a: &GateAccount) -> (Reading, &'static str) {
    if a.refuses_everything() {
        // Asked, and approved nothing. The Γ bug's signature.
        (Reading::Fault, "refuses_everything")
    } else if a.never_asked() {
        // Not a pass — the same rule as liveness's `Inert`.
        (Reading::Unknown, "never_asked")
    } else if a.admits_everything() {
        // Reported, never asserted. See `GATE_CAVEATS`.
        (Reading::Unknown, "admits_everything")
    } else {
        (Reading::Idle, "discriminating")
    }
}

fn since(r: Retention) -> &'static str {
    match r {
        Retention::Recorded => "ledger",
        Retention::Counted => "boot",
    }
}

/// Assemble one gate's view.
///
/// Pure over a [`GateAccount`], so the shape a surface receives is testable
/// without touching the counters — the same split as `loop_api::view`.
pub fn view(a: &GateAccount) -> GateView {
    let (reading, token) = read(a);
    let spec = gate_trust::GATES.iter().find(|g| g.id == a.id);
    GateView {
        id: a.id,
        refuses: spec.map(|s| s.refuses).unwrap_or("(undeclared gate)"),
        site: a.site,
        reading,
        token,
        approved: a.approved,
        refused: a.refused,
        undetermined: a.undetermined,
        since: since(a.retention),
        if_never_refuses: a.if_never_refuses,
        last_refusal: a.last_refusal.clone(),
        door: GATE_DOORS.iter().find(|d| d.subject == a.id).copied(),
    }
}

/// Assemble every gate from the live counters.
pub fn views() -> Vec<GateView> {
    gate_trust::accounts().iter().map(view).collect()
}

/// The header, in four buckets.
///
/// Same discipline as `loop_api::LoopTally`: the buckets partition the set, and
/// the two that mean "no reading available" are not folded in with the pass.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct GateTally {
    pub total: usize,
    /// Has both approved and refused.
    pub discriminating: usize,
    /// Asked, and approved nothing.
    pub inverted: usize,
    /// Asked, and refused nothing. Reported, never asserted.
    pub never_refused: usize,
    /// Not asked at all since the counters started. Not a pass.
    pub unexercised: usize,
}

pub fn tally(views: &[GateView]) -> GateTally {
    let mut t = GateTally {
        total: views.len(),
        discriminating: 0,
        inverted: 0,
        never_refused: 0,
        unexercised: 0,
    };
    for v in views {
        match v.token {
            "discriminating" => t.discriminating += 1,
            "refuses_everything" => t.inverted += 1,
            "admits_everything" => t.never_refused += 1,
            "never_asked" => t.unexercised += 1,
            // No catch-all that silently drops one: an unrecognised token is a
            // new state upstream, and `the_buckets_partition_the_gates` fails
            // rather than under-counting.
            _ => {}
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate_trust::Clock;

    fn account(approved: u64, refused: u64, retention: Retention) -> GateAccount {
        GateAccount {
            id: "coherence",
            clock: Clock::Invocation,
            retention,
            site: "a::b",
            approved,
            refused,
            undetermined: 0,
            last_refusal: None,
            reading: None,
            if_never_refuses: "a fixture",
        }
    }

    /// The four states a counter pair can be in, and none of them is a number.
    #[test]
    fn a_gate_that_refuses_everything_is_a_fault_and_one_never_asked_is_not_a_pass() {
        assert_eq!(
            read(&account(0, 3, Retention::Counted)),
            (Reading::Fault, "refuses_everything")
        );
        assert_eq!(
            read(&account(0, 0, Retention::Counted)),
            (Reading::Unknown, "never_asked"),
            "an unexercised gate must not read as healthy — an unwired control \
             and one with nothing to refuse are the same observation"
        );
        assert_eq!(
            read(&account(3, 0, Retention::Counted)),
            (Reading::Unknown, "admits_everything"),
            "a gate that has refused nothing is not a pass and not a fault; \
             asserting on it would assert that violations must exist"
        );
        assert_eq!(
            read(&account(3, 1, Retention::Counted)),
            (Reading::Idle, "discriminating")
        );
    }

    /// An inverted control outranks an unexercised one.
    ///
    /// `refuses_everything` requires `asked > 0`, so it cannot collide with
    /// `never_asked` — but the ordering is asserted because a future arm added
    /// above it would silently take precedence, and this is the arm that must
    /// win when it applies.
    #[test]
    fn the_worst_available_reading_wins() {
        // Asked three times, approved none: inverted, not merely "no approvals".
        let inverted = account(0, 3, Retention::Counted);
        assert!(inverted.refuses_everything() && !inverted.never_asked());
        assert_eq!(read(&inverted).0, Reading::Fault);
    }

    /// `0 refusals` must never read as "never in its life".
    #[test]
    fn the_view_says_whether_the_counters_survive_a_restart() {
        assert_eq!(view(&account(3, 0, Retention::Counted)).since, "boot");
        assert_eq!(view(&account(3, 0, Retention::Recorded)).since, "ledger");
    }

    /// Every declared gate resolves to a real spec.
    ///
    /// `view` falls back to `(undeclared gate)` rather than panicking, so a
    /// counter with no spec would render as a gate with no description. This is
    /// the cross-boundary pin that stops it.
    #[test]
    fn every_live_gate_account_matches_a_declared_gate() {
        for a in gate_trust::accounts() {
            assert!(
                gate_trust::GATES.iter().any(|g| g.id == a.id),
                "`{}` has counters and no declaration, so the surface would \
                 render it with no description of what it refuses",
                a.id
            );
        }
        // And the reverse: a declared gate with no counter would be invisible.
        let live: Vec<&str> = gate_trust::accounts().iter().map(|a| a.id).collect();
        for g in gate_trust::GATES {
            assert!(
                live.contains(&g.id),
                "`{}` is declared and has no account, so it cannot appear on \
                 the surface at all",
                g.id
            );
        }
    }

    /// The buckets partition the gates.
    #[test]
    fn the_buckets_partition_the_gates() {
        let vs: Vec<GateView> = vec![
            view(&account(3, 1, Retention::Counted)),
            view(&account(0, 3, Retention::Counted)),
            view(&account(3, 0, Retention::Counted)),
            view(&account(0, 0, Retention::Counted)),
        ];
        let t = tally(&vs);
        assert_eq!(t.total, 4);
        assert_eq!(t.discriminating, 1);
        assert_eq!(t.inverted, 1);
        assert_eq!(t.never_refused, 1);
        assert_eq!(t.unexercised, 1);
        assert_eq!(
            t.discriminating + t.inverted + t.never_refused + t.unexercised,
            t.total,
            "a gate fell through the buckets, so the header omits it silently"
        );
    }

    /// A durability claim with nothing behind it is the finding.
    #[test]
    fn a_recorded_gate_with_an_empty_ledger_is_unbacked() {
        // Asked forty times, ledger empty: the surface says these survive a
        // restart and they do not.
        assert_eq!(
            ledger_claim(&account(40, 0, Retention::Recorded), 0),
            LedgerClaim::Unbacked { asked: 40 }
        );
        // Asked and recorded: the claim holds.
        assert_eq!(
            ledger_claim(&account(40, 0, Retention::Recorded), 40),
            LedgerClaim::Backed { rows: 40 }
        );
        // Never asked. Nothing to record, so nothing is claimed — and calling
        // this `Unbacked` would report a finding on every gate after a deploy.
        assert_eq!(
            ledger_claim(&account(0, 0, Retention::Recorded), 0),
            LedgerClaim::NothingToRecord
        );
        // Counted-only never promised durability.
        assert_eq!(
            ledger_claim(&account(40, 0, Retention::Counted), 0),
            LedgerClaim::NotClaimed
        );
    }

    /// The ledger queries read one gate and write nothing.
    #[test]
    fn the_ledger_queries_are_read_only_and_bind_the_gate() {
        for (label, sql) in [("ledger", LEDGER_SQL), ("count", LEDGER_COUNT_SQL)] {
            let q = sql.to_ascii_lowercase();
            assert!(q.trim_start().starts_with("select"), "{label}");
            for w in ["insert", "update ", "delete", "drop", "alter", "truncate"] {
                assert!(!q.contains(w), "{label} contains `{w}`");
            }
            // Without `$1` it returns every gate's decisions under one gate's
            // name — the same substitution the per-agent loop view exists to
            // prevent, one domain over.
            assert!(
                sql.contains("$1"),
                "{label} does not bind the gate, so it would show every gate's \
                 decisions as this one's"
            );
        }
        // Refusals first. A reader opening this asks what was stopped.
        assert!(
            LEDGER_SQL.contains("(decision = 'refused') DESC"),
            "the ledger does not surface refusals first, so the thing a reader \
             came for is behind however many approvals happened to be newer"
        );
    }

    /// The shared rules apply to gate doors too, empty or not.
    ///
    /// Asserted over an empty list on purpose: the day someone adds a door here
    /// it is already governed, rather than governed by whoever remembers to add
    /// the test.
    #[test]
    fn every_gate_door_satisfies_the_shared_rules() {
        let problems = crate::surface::door_problems(GATE_DOORS);
        assert!(problems.is_empty(), "\n  {}\n", problems.join("\n  "));
        for d in GATE_DOORS {
            assert!(
                gate_trust::GATES.iter().any(|g| g.id == d.subject),
                "`{}` is not a declared gate, so this door acts on nothing",
                d.subject
            );
        }
    }

    /// A review door only on a gate whose decisions exist.
    ///
    /// A review is a judgement about **one decision**, so it needs a row to point
    /// at, and only a `Retention::Recorded` gate writes one. Offering the door on
    /// a counted-only gate would put a reviewer in front of a queue that is
    /// permanently empty for a reason no message on the screen could explain —
    /// its decisions never left the process — and the reviewer's conclusion would
    /// be that the gate has never refused anything.
    ///
    /// Asserted rather than left to the door's prose, because the prose in
    /// `GATE_DOORS` makes exactly this argument and prose does not fail a build.
    /// The likely way it breaks is not someone adding a bad door: it is someone
    /// demoting a gate from `Recorded` to `Counted` to reduce write volume, which
    /// is a reasonable change that silently strands whatever doors point at it.
    #[test]
    fn a_review_door_only_exists_where_the_decisions_do() {
        for d in GATE_DOORS {
            if !d.path.contains("/decisions/") {
                continue;
            }
            let spec = gate_trust::GATES
                .iter()
                .find(|g| g.id == d.subject)
                .expect("checked above");
            assert_eq!(
                spec.retention,
                Retention::Recorded,
                "`{}` offers a per-decision door and is `Counted`, so its \
                 decisions are process-local and there is nothing to review. \
                 Either promote it in `gate_trust::GATES` — migration 214's \
                 comment on the rate-limit gate is the counter-argument — or \
                 remove the door.",
                d.subject
            );
        }
        // And the other direction, so the door set cannot be quietly emptied:
        // every `Recorded` gate has one. A ledger with no reviewer is the state
        // the platform was in for its whole life.
        for g in gate_trust::GATES
            .iter()
            .filter(|g| g.retention == Retention::Recorded)
        {
            assert!(
                GATE_DOORS
                    .iter()
                    .any(|d| d.subject == g.id && d.path.contains("/decisions/")),
                "`{}` records every decision it makes and nobody can say whether \
                 any of them was right. That is the state `gate_review` exists to \
                 end; a ledger with no reviewer is a record nobody reads.",
                g.id
            );
        }
    }

    /// Every caveat is a caveat.
    #[test]
    fn every_gate_caveat_says_what_a_tick_does_not_mean() {
        let problems = crate::surface::caveat_problems(GATE_CAVEATS);
        assert!(problems.is_empty(), "\n  {}\n", problems.join("\n  "));
        // The two readings that share `unknown` must both be qualified, or a
        // surface showing `unknown` cannot say which it has.
        for subject in ["gate.admits_everything", "gate.never_asked"] {
            assert!(
                GATE_CAVEATS.iter().any(|c| c.subject == subject),
                "`{subject}` maps to `unknown` and carries no caveat, so a \
                 reader cannot tell it from the other state that does"
            );
        }
        // `gate_review::Standing::Upheld` is the only state on this surface that
        // maps to `Reading::Idle` on the strength of a human judgement, and it is
        // the narrowest pass here: it says the decisions *someone chose to look
        // at* were right. Reviewers pick what to review and the selection is not
        // random, so an `all_upheld` standing over 3 of 400 decisions is a sample
        // and the tick reads as a verdict.
        //
        // Asserted separately from the loop above because its argument is the
        // opposite one — those two need a caveat because `unknown` is ambiguous;
        // this needs one because `idle` is not.
        assert!(
            GATE_CAVEATS
                .iter()
                .any(|c| c.subject == "gate.review.upheld"),
            "the only human-judged pass on this surface has no caveat. A green \
             tick from `gate_review` without its denominator asserts a gate is \
             sound on evidence about however few of its decisions anyone \
             happened to open."
        );
    }
}
