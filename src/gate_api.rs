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
/// **Empty, and that is the finding.** There is no endpoint anywhere that lets a
/// person act on a gate: no way to review what a gate refused, no way to
/// override a refusal, no way to record that a refusal was wrong. `gate_trust`
/// counts decisions and migration 214 gave them a ledger, and the whole surface
/// is read-only.
///
/// That is not obviously wrong — a gate a person can wave through is not much of
/// a gate — but it is a decision nobody has made explicitly, and until this list
/// existed there was nowhere to notice it. The rule is the same as everywhere
/// else here: it may only grow with a reason, and an entry must argue for being
/// manual.
///
/// The nearest thing that exists is Loop 2's HITL queue, which acts on
/// *anomalies* rather than on gate decisions. Those are different objects: an
/// anomaly is a defect found in an output, a gate decision is a refusal to
/// produce one.
pub const GATE_DOORS: &[Door] = &[];

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
    }
}
