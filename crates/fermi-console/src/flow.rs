//! What Ctrl+Enter should do.
//!
//! One key now covers three things — run staged research, decompose, or
//! refuse to overwrite — and two of them are irreversible: decomposition
//! discards the whole forecast, and running research bills real money
//! (about $6 for a five-driver fan-out). Getting the branch wrong loses
//! an operator's afternoon or spends their budget on the wrong agents.
//!
//! That decision therefore does not belong inside a GPUI event handler
//! where it cannot be tested. `cockpit.rs` lives in the binary target,
//! where rustc segfaults expanding the element tree under `--test`; see
//! the crate docs. The handler now asks this module and does as it is
//! told.
//!
//! GPUI-free. `cargo test -p fermi-console --lib` runs in seconds.

/// What the operator's research key should trigger next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResearchAction {
    /// Execute the agents decomposition staged for review.
    RunStaged { count: usize },
    /// Decompose the question. Safe: nothing would be lost.
    Decompose,
    /// Decomposition would discard existing work. Warn and wait for a
    /// second press rather than doing it.
    ArmOverwrite {
        drivers: usize,
        evidence: usize,
        agents: usize,
    },
    /// Decompose, overwriting — the operator has confirmed.
    ConfirmOverwrite,
    /// Nothing to do (no question typed).
    Nothing,
}

/// Everything the decision depends on, named so the call site can't
/// silently pass the wrong flag.
#[derive(Debug, Clone, Copy)]
pub struct ResearchContext {
    /// Agents assigned by decomposition but not yet executed.
    pub staged: usize,
    /// A non-empty question exists to decompose.
    pub has_question: bool,
    pub drivers: usize,
    pub evidence: usize,
    /// Agents bound to drivers (excluding `fermi` itself).
    pub agents: usize,
    /// The operator has already been warned and is pressing again.
    pub armed: bool,
}

/// Decide what the research key does.
///
/// Priority order, and why:
///
/// 1. **Staged research wins.** If decomposition has staged agents, the
///    operator's next intent is overwhelmingly "run them" — they have
///    just been told to review and then press again. Re-decomposing at
///    that moment would throw away the assignment they were reviewing.
/// 2. **No question, nothing to do.** Guarded before the destructive
///    branch so an accidental press on an empty composer cannot arm an
///    overwrite prompt about work it would not actually touch.
/// 3. **Confirmed overwrite proceeds.**
/// 4. **Unconfirmed overwrite arms** when there is anything to lose.
///    Drivers OR evidence is enough: a forecast with hand-tuned
///    estimates and no evidence is still hours of work.
/// 5. **Otherwise decompose**, which is what a fresh forecast wants.
pub fn next_research_action(ctx: ResearchContext) -> ResearchAction {
    if ctx.staged > 0 {
        return ResearchAction::RunStaged { count: ctx.staged };
    }
    if !ctx.has_question {
        return ResearchAction::Nothing;
    }
    if ctx.armed {
        return ResearchAction::ConfirmOverwrite;
    }
    if ctx.drivers > 0 || ctx.evidence > 0 {
        return ResearchAction::ArmOverwrite {
            drivers: ctx.drivers,
            evidence: ctx.evidence,
            agents: ctx.agents,
        };
    }
    ResearchAction::Decompose
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ResearchContext {
        ResearchContext {
            staged: 0,
            has_question: true,
            drivers: 0,
            evidence: 0,
            agents: 0,
            armed: false,
        }
    }

    // ── The money guard ─────────────────────────────────────────────

    #[test]
    fn staged_research_runs_before_anything_else() {
        // Decomposition has just told the operator "review, then press
        // again". Pressing again must run what they reviewed, not
        // re-decompose and discard it.
        let c = ResearchContext {
            staged: 5,
            drivers: 5,
            evidence: 3,
            agents: 5,
            ..ctx()
        };
        assert_eq!(
            next_research_action(c),
            ResearchAction::RunStaged { count: 5 }
        );
    }

    #[test]
    fn staged_research_wins_even_when_armed() {
        // Belt and braces: an arm left over from an earlier press must
        // never turn into an overwrite while staged work exists.
        let c = ResearchContext {
            staged: 2,
            drivers: 4,
            armed: true,
            ..ctx()
        };
        assert_eq!(
            next_research_action(c),
            ResearchAction::RunStaged { count: 2 }
        );
    }

    #[test]
    fn nothing_is_staged_on_a_fresh_forecast() {
        assert_eq!(next_research_action(ctx()), ResearchAction::Decompose);
    }

    // ── The data guard ──────────────────────────────────────────────

    #[test]
    fn a_populated_forecast_arms_instead_of_overwriting() {
        // The reported defect: this used to go straight to
        // `Program::empty()`.
        let c = ResearchContext {
            drivers: 5,
            evidence: 7,
            agents: 6,
            ..ctx()
        };
        assert_eq!(
            next_research_action(c),
            ResearchAction::ArmOverwrite {
                drivers: 5,
                evidence: 7,
                agents: 6
            }
        );
    }

    #[test]
    fn hand_tuned_drivers_with_no_evidence_still_count_as_work() {
        let c = ResearchContext {
            drivers: 4,
            evidence: 0,
            ..ctx()
        };
        assert!(matches!(
            next_research_action(c),
            ResearchAction::ArmOverwrite { .. }
        ));
    }

    #[test]
    fn evidence_with_no_drivers_still_counts_as_work() {
        // Restored-from-server forecasts can land in this shape.
        let c = ResearchContext {
            drivers: 0,
            evidence: 3,
            ..ctx()
        };
        assert!(matches!(
            next_research_action(c),
            ResearchAction::ArmOverwrite { .. }
        ));
    }

    #[test]
    fn the_second_press_goes_through() {
        let c = ResearchContext {
            drivers: 5,
            evidence: 7,
            armed: true,
            ..ctx()
        };
        assert_eq!(next_research_action(c), ResearchAction::ConfirmOverwrite);
    }

    #[test]
    fn arming_is_not_required_when_there_is_nothing_to_lose() {
        // A confirm prompt on an empty forecast is noise, and noise
        // trains people to dismiss the prompt that matters.
        let c = ResearchContext {
            drivers: 0,
            evidence: 0,
            agents: 0,
            ..ctx()
        };
        assert_eq!(next_research_action(c), ResearchAction::Decompose);
    }

    // ── Degenerate input ────────────────────────────────────────────

    #[test]
    fn no_question_does_nothing_even_with_drivers_present() {
        // Must be checked BEFORE the destructive branch, or an empty
        // composer would arm a scary prompt about work it cannot touch.
        let c = ResearchContext {
            has_question: false,
            drivers: 5,
            evidence: 7,
            ..ctx()
        };
        assert_eq!(next_research_action(c), ResearchAction::Nothing);
    }

    #[test]
    fn staged_research_runs_even_with_no_question() {
        // The agents are already built and carry their own queries; the
        // question text is irrelevant to executing them.
        let c = ResearchContext {
            staged: 3,
            has_question: false,
            ..ctx()
        };
        assert_eq!(
            next_research_action(c),
            ResearchAction::RunStaged { count: 3 }
        );
    }
}
