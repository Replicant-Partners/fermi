//! The verification ladder, as the paper names it.
//!
//! # Why a map and not a rename
//!
//! `verification_for_agent_ecologies.md` §3 defines five rungs, ordered by
//! difficulty, and the load-bearing claim is that **a check answering an easier
//! question will pass while a harder one fails**. Naming them separately is
//! what stops a presence check being mistaken for a truth check.
//!
//! Three of the five modules are named for their *mechanism* instead, and the
//! modules disagree with the paper about their own positions:
//!
//! | rung | paper | module | module says of itself |
//! |---|---|---|---|
//! | 1 | Presence | `schema_trust` | — |
//! | 2 | Liveness | `liveness_trust` | "the **fifth** trust contract" |
//! | 3 | Truth | `rollup_trust` | — |
//! | 4 | Grounding | `grounding_trust` | "**Third** sibling" |
//! | 5 | Binding | `port_trust` | "**Fourth** in the family" |
//!
//! Renaming the modules would touch every call site and change no behaviour.
//! The drift that costs something is not the file name — it is that **no
//! artifact anywhere states which module answers which of the paper's
//! questions**, so a reader has to reconstruct it, and the modules' own
//! ordinals actively mislead them.
//!
//! This is that artifact. It is a declaration with tests, which is the same
//! remedy the rest of this audit applied to vocabularies and gates: make the
//! mapping explicit, then make it impossible for it to rot quietly.
//!
//! # The ordinals are the finding
//!
//! Each module's self-declared position was written when it was added, relative
//! to the modules that existed then. That is a chronology, not a ladder, and
//! the two were never reconciled — so `liveness_trust` calls itself fifth while
//! being the paper's second, and its own docs explain at length that it is the
//! rung *beneath* the others.
//!
//! The order matters because it is the order of **cost and of strength**:
//! presence is a catalogue read, binding is a string comparison, and each rung
//! is invisible to the one below it. A reader who believes liveness sits above
//! grounding will reach for the wrong check.

/// One rung of the ladder.
#[derive(Debug, Clone, Copy)]
pub struct Rung {
    /// Position in the paper's ordering, 1-indexed.
    pub position: u8,
    /// The paper's name for it. The canonical term.
    pub name: &'static str,
    /// The question it answers, from the paper's table.
    pub question: &'static str,
    /// The module that implements it.
    pub module: &'static str,
    /// Which of the three clocks it runs on (§4.1).
    pub clock: &'static str,
    /// What it catches that the rung below it cannot.
    pub catches: &'static str,
    /// What passes this rung while the rung above it fails. The paper's
    /// "passes while" column, and the reason the rungs are separate contracts
    /// rather than one validation layer.
    pub passes_while: &'static str,
}

/// The five rungs, in the paper's order.
pub const LADDER: &[Rung] = &[
    Rung {
        position: 1,
        name: "Presence",
        question: "Does the declared object exist?",
        module: "schema_trust",
        clock: "boot, then sweep",
        catches: "a renamed column, a dropped view",
        passes_while: "the table exists and is permanently empty",
    },
    Rung {
        position: 2,
        name: "Liveness",
        question: "Does the writer ever run?",
        module: "liveness_trust",
        clock: "sweep only",
        catches: "a ledger nothing has ever written",
        passes_while: "rows accumulate and disagree with their source",
    },
    Rung {
        position: 3,
        name: "Truth",
        question: "Does the stored value equal its source of truth?",
        module: "rollup_trust",
        clock: "CI, and on demand",
        catches: "a counter that disagrees with reality",
        passes_while: "the field is well-typed and invented",
    },
    Rung {
        position: 4,
        name: "Grounding",
        question: "Could this value have come from any available tool?",
        module: "grounding_trust",
        clock: "admission, then per invocation",
        catches: "a fabricated measurement",
        passes_while: "the output is grounded but the request never matched the interface",
    },
    Rung {
        position: 5,
        name: "Binding",
        question: "Does the invocation match the declared interface?",
        module: "port_trust",
        clock: "per request",
        catches: "prose sent to a structured-only port",
        passes_while: "— the top of the ladder",
    },
];

/// The rung a module implements, if any.
pub fn rung_of(module: &str) -> Option<&'static Rung> {
    LADDER.iter().find(|r| r.module == module)
}

/// The rung by the paper's name.
pub fn rung(name: &str) -> Option<&'static Rung> {
    LADDER.iter().find(|r| r.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ladder_is_five_rungs_in_order() {
        assert_eq!(LADDER.len(), 5);
        for (i, r) in LADDER.iter().enumerate() {
            assert_eq!(
                r.position as usize,
                i + 1,
                "{} is declared at index {i} and claims position {}",
                r.name,
                r.position
            );
        }
    }

    #[test]
    fn every_rung_names_a_distinct_module_and_question() {
        let mut modules = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for r in LADDER {
            assert!(modules.insert(r.module), "{} shares a module", r.name);
            assert!(names.insert(r.name), "duplicate rung name {}", r.name);
            assert!(r.question.ends_with('?'), "{}: state a question", r.name);
            assert!(r.catches.len() > 10, "{}: say what it catches", r.name);
        }
    }

    /// Each module named here must exist.
    ///
    /// A map pointing at a module that has been renamed or deleted is worse
    /// than no map: it reads as current and sends the reader somewhere empty.
    #[test]
    fn every_named_module_exists() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for r in LADDER {
            let p = root.join(format!("{}.rs", r.module));
            assert!(
                p.exists(),
                "{} names `{}`, and src/{}.rs does not exist",
                r.name,
                r.module,
                r.module
            );
        }
    }

    /// The rung names must be the paper's, not the modules'.
    ///
    /// Two of the five modules are named after the rung they implement and
    /// three after their mechanism. That is tolerable; what is not is losing
    /// the paper's word for the concept, because it is the word the
    /// architecture documents and the release notes use.
    #[test]
    fn the_canonical_names_survive_the_module_names() {
        assert_eq!(rung("truth").map(|r| r.module), Some("rollup_trust"));
        assert_eq!(rung("presence").map(|r| r.module), Some("schema_trust"));
        assert_eq!(rung("binding").map(|r| r.module), Some("port_trust"));
        // ...and the reverse direction, which is the lookup a reader in a
        // module actually needs.
        assert_eq!(rung_of("liveness_trust").map(|r| r.position), Some(2));
        assert_eq!(rung_of("grounding_trust").map(|r| r.position), Some(4));
        assert!(rung_of("write_accounting").is_none());
    }

    /// Liveness is the second rung, whatever its module says.
    ///
    /// Pinned on its own because the module calls itself "the fifth trust
    /// contract" in its first three lines, and that sentence has been copied
    /// into two other modules' docs. The chronology is real — it was written
    /// fifth — and it is not the ladder.
    #[test]
    fn liveness_is_the_second_rung_not_the_fifth() {
        let l = rung("liveness").expect("liveness is on the ladder");
        assert_eq!(l.position, 2);
        assert!(
            l.position < rung("grounding").unwrap().position,
            "liveness sits BENEATH grounding: a fabricated value in a table \
             nothing writes is not a grounding problem, it is an empty table"
        );
    }
}
