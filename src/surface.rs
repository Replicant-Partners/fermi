//! The shape every trust surface has, declared once.
//!
//! # What is shared and what deliberately is not
//!
//! Loops, gates and evaluators each have five parts. Three of them are
//! **answers**, and answers are never shared — §3.4: a trust calculation must
//! have exactly one implementation, and the layer that owns the vocabulary owns
//! the arithmetic.
//!
//! | part | loops | gates | evaluators | shared? |
//! |---|---|---|---|---|
//! | declared model | `loop_model::LOOPS` | `gate_trust::GATES` | `native_evaluators::registry` | no |
//! | measurement | rows per stage | approve/refuse counters | verdicts over a snapshot | no |
//! | interpretation | `panel_absence::Reading` | `GateAccount::*` | `Verdict` | no |
//! | **door** | who acts, and where | | | **yes** |
//! | **caveat** | what a green tick does not mean | | | **yes** |
//!
//! The last two rows existed only for loops, in `loop_api`. They are the same
//! idea in all three domains and the same set of ways to get them wrong, so
//! they live here — one [`Door`] type, one set of rules, and **one** contract
//! scanning the router rather than three that can drift.
//!
//! # Why the door is the load-bearing part
//!
//! A trust surface that only reports is a dashboard. These domains are
//! human-gated by design — Loop 2's `reviewed` stage *is* a person acting, a
//! gate that refuses everything needs someone to decide whether it should — and
//! a stalled manual stage with no visible door is indistinguishable from a
//! platform defect. Worse, it reads as one: the reviewer concludes the system is
//! broken when the truth is that nobody has been shown the queue.
//!
//! # Why the caveat is the other one
//!
//! Every check in this repository is narrower than the claim it serves. A
//! surface that renders a green tick against the claim rather than against the
//! proposition is the over-reading the whole audit is about, committed at the
//! last possible moment, in the one artifact a non-author reads.
//!
//! So [`Caveat`] is a required field, not a doc comment. If a surface cannot say
//! what its tick fails to establish, it should not render a tick.

/// A human door into a trust domain.
///
/// Keyed by `subject`, in the domain's own vocabulary, because the three domains
/// do not share a key: a loop's is `loop.stage`, a gate's is its id, an
/// evaluator's is its id. Forcing one key shape would produce a lowest common
/// denominator that fits none of them and reads as a leaky abstraction in all
/// three.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Door {
    /// `loop2.reviewed`, `gate.coherence`, `evaluator.refused_writes`.
    ///
    /// Checked against the owning domain's declared set by that domain's own
    /// tests — this module cannot know what a valid subject is, and pretending
    /// to would make it a fourth answer.
    pub subject: &'static str,
    /// HTTP method, as a UI would call it.
    pub method: &'static str,
    /// Path template, verbatim from the router so a client can substitute
    /// params. Checked to exist by [`crate::surface::doors_missing_from`].
    pub path: &'static str,
    /// What pressing it does. One line, suitable for a button tooltip.
    pub does: &'static str,
    /// **Why a person rather than the platform.**
    ///
    /// Required. A manual step that cannot say why it is manual should be
    /// automated, and the field exists to make that argument happen at
    /// declaration time rather than never. It is also what a reviewer needs
    /// before deciding a queue is worth working: the button says what it does,
    /// this says why it is theirs to press.
    pub why_manual: &'static str,
}

/// What a passing reading does **not** establish.
///
/// Carried into the API payload rather than left in a doc comment, because the
/// consumer of the payload is the surface that would over-read it.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Caveat {
    /// The subject this qualifies, in the domain's own key.
    pub subject: &'static str,
    /// The narrower thing that was actually checked.
    pub checked: &'static str,
    /// What a green tick here does not mean.
    pub does_not_show: &'static str,
}

/// Which declared doors name a path the router does not have.
///
/// The one scan, shared by every domain. Three copies of this would be three
/// chances for one of them to stop matching, and a door that 404s is worse than
/// a missing one: the reviewer presses it, believes they acted, and the failure
/// arrives after the belief.
///
/// Matches the **quoted** path, because axum routes are string literals and
/// matching unquoted would let `/api/loops` satisfy `/api/loops/actions`.
///
/// Returns the offenders rather than asserting, so the caller can name its own
/// domain in the failure message.
pub fn doors_missing_from(router_src: &str, doors: &[Door]) -> Vec<String> {
    doors
        .iter()
        .filter(|d| !router_declares(router_src, d.path))
        .map(|d| format!("{} → {} {}", d.subject, d.method, d.path))
        .collect()
}

/// Is `path` declared as a route in this router source?
pub fn router_declares(router_src: &str, path: &str) -> bool {
    router_src.contains(&format!("\"{path}\""))
}

/// Every rule a door must satisfy that does not depend on the domain.
///
/// Returned rather than asserted, for the same reason as above.
pub fn door_problems(doors: &[Door]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for d in doors {
        if !seen.insert((d.subject, d.path)) {
            out.push(format!("{}: declares `{}` twice", d.subject, d.path));
        }
        if d.why_manual.len() < 100 {
            out.push(format!(
                "{}: a manual step that cannot say why it is manual should be \
                 automated",
                d.subject
            ));
        }
        if d.does.len() < 40 {
            out.push(format!("{}: say what pressing it does", d.subject));
        }
        if !d.path.starts_with("/api/") {
            out.push(format!("{}: `{}` is not an API path", d.subject, d.path));
        }
        if !matches!(d.method, "GET" | "POST" | "PATCH" | "DELETE") {
            out.push(format!("{}: `{}` is not a method", d.subject, d.method));
        }
    }
    out
}

/// Every rule a caveat must satisfy.
pub fn caveat_problems(caveats: &[Caveat]) -> Vec<String> {
    let mut out = Vec::new();
    for c in caveats {
        if c.does_not_show.len() < 100 {
            out.push(format!(
                "{}: a surface showing a tick with no caveat over-reads it, and \
                 a caveat this short is not one",
                c.subject
            ));
        }
        if c.checked.len() < 20 {
            out.push(format!("{}: say what was actually checked", c.subject));
        }
        if c.checked == c.does_not_show {
            out.push(format!("{}: the caveat restates the check", c.subject));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door(path: &'static str) -> Door {
        Door {
            subject: "d.s",
            method: "POST",
            path,
            does: "A sentence long enough to describe what pressing it does.",
            why_manual: "A reason long enough to be a reason, which is a hundred \
                         characters at minimum so that it has to be an argument \
                         rather than a label.",
        }
    }

    /// A shorter route must not stand in for a longer one.
    ///
    /// The reason the match is on the quoted path. `/api/loops` and
    /// `/api/loops/actions` are different endpoints, and a substring match would
    /// declare the second present because the first is.
    #[test]
    fn a_prefix_of_a_real_route_does_not_satisfy_the_check() {
        let router = r#".route("/api/loops", get(h)).route("/api/x/:id/act", post(h))"#;
        assert!(router_declares(router, "/api/loops"));
        assert!(!router_declares(router, "/api/loops/actions"));
        assert!(!router_declares(router, "/api/x/:id/ac"));
        assert!(!router_declares(router, "/api/nothing"));
    }

    /// The scan must name the door that is missing, not merely fail.
    #[test]
    fn a_missing_door_is_reported_with_its_subject() {
        let router = r#".route("/api/real", post(h))"#;
        let doors = [door("/api/real"), door("/api/invented")];
        let missing = doors_missing_from(router, &doors);
        assert_eq!(missing.len(), 1);
        assert!(
            missing[0].contains("/api/invented") && missing[0].contains("d.s"),
            "the report does not say which door or which subject: {missing:?}"
        );
    }

    /// A door with no argument for being manual is rejected.
    #[test]
    fn a_door_that_cannot_argue_for_being_manual_is_a_problem() {
        let lazy = Door {
            why_manual: "because",
            ..door("/api/x")
        };
        let problems = door_problems(&[lazy]);
        assert!(
            problems.iter().any(|p| p.contains("should be automated")),
            "{problems:?}"
        );
        // And a well-formed one raises nothing.
        assert!(door_problems(&[door("/api/x")]).is_empty());
    }

    /// A caveat that restates the check is not a caveat.
    ///
    /// The failure mode this field exists to prevent: a surface that fills
    /// `does_not_show` by paraphrasing `checked` has satisfied the type and
    /// told a reader nothing about the gap between the proposition and the
    /// claim.
    #[test]
    fn a_caveat_that_restates_the_check_is_rejected() {
        let long = "A sentence long enough to clear the hundred character floor \
                    this module sets, so that the only thing being tested here \
                    is the restatement rule itself.";
        let c = Caveat {
            subject: "d.s",
            checked: long,
            does_not_show: long,
        };
        assert!(
            caveat_problems(&[c]).iter().any(|p| p.contains("restates")),
            "a caveat identical to the check was accepted"
        );
    }

    /// One path per subject.
    #[test]
    fn a_subject_cannot_declare_the_same_door_twice() {
        let problems = door_problems(&[door("/api/x"), door("/api/x")]);
        assert!(problems.iter().any(|p| p.contains("twice")), "{problems:?}");
    }
}
