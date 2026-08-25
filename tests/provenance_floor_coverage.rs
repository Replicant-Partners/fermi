//! Every path that writes semantic rules must know how to grade them.
//!
//! # The failure this exists to prevent
//!
//! Rules do not stay in `semantic_rules`. `kg_context::build_kg_block_inner`
//! retrieves them and appends them to an agent's system prompt under "Learned
//! Knowledge", which makes them premises the agent reasons from. A rule with
//! no provenance floor renders as "grounding unknown" — honest, but the honest
//! reading of a corpus that is entirely unknown is that the column was never
//! wired, and at that point it is decoration.
//!
//! There were three production construction sites for `ConsolidationWorker`
//! and the first pass wired one of them. The one that was missed —
//! `handlers/creatures/agent_modules.rs` — is the higher-volume rule writer of
//! the two. Wiring the path you are looking at and missing the other one is the
//! normal shape of this mistake, and it does not announce itself: the rules
//! still get written, they just arrive ungraded.
//!
//! **Correction.** This comment used to say the creature path "runs on a timer
//! while the HTTP handler runs when somebody asks". That is not true of this
//! repository and was never checked. `creature_dream_handler` is reached only
//! from `POST /api/creatures/:creature_id/dream`; grep for a scheduler and
//! there is none, for either path. Both run when somebody asks.
//!
//! The error is left visible rather than quietly edited because it is this
//! project's own defect class applied to itself: a plausible mechanism, written
//! confidently in a place readers trust, never checked against the running
//! system. It also mattered — "runs by itself" is precisely the property that
//! made this path worth a coverage test, and the property is imaginary. The
//! test is still worth having; the reason given for it was wrong.
//!
//! That nothing schedules Loop 1 at all is now tracked as a liveness contract
//! (`consolidation_jobs (Loop 1 cadence)` in `src/liveness_trust.rs`) rather
//! than left as an assumption in a doc comment.
//!
//! # Why a source scan
//!
//! A type-level fix would be better — make the oracle a constructor argument
//! so the compiler refuses an unwired worker. It is not free: the argument
//! would have to be `Option`, because `agent-bestiary-consolidate` has no way
//! to supply one, and an `Option` parameter that every caller may pass `None`
//! to is the same hole with more ceremony. So the enforcement is a scan over
//! the source, with an exemption list that must name each exception and say
//! why — the pattern `grounding_trust::cross_check_exempt` already uses.
//!
//! The list may only shrink.

use std::path::{Path, PathBuf};

/// Files permitted to construct a `ConsolidationWorker` without an oracle.
///
/// Each entry is a decision, not an oversight. Adding to this list should
/// require an argument; removing from it should not.
const EXEMPT: &[(&str, &str)] = &[
    (
        "agent-bestiary/consolidate/src/main.rs",
        "Standalone CLI worker in a crate that does not depend on `fermi`, so \
         it cannot reach the field contracts without either a dependency on \
         the whole server graph or a second copy of the grounding arithmetic. \
         Nothing deploys it — no workflow, Dockerfile, or script references \
         the binary — so it is a manual tool, and `floor_for` logs a warning \
         naming the worker when it runs unwired. Wire it, or delete it, before \
         it is ever put on a timer.",
    ),
    (
        "agent-bestiary/memory/tests/test_llm_providers.rs",
        "Test fixture: constructs workers to assert they build, never runs a \
         cycle and never writes a rule.",
    ),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Recursively collect `.rs` files, skipping build output.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name == "target" || name == ".git" || name == "node_modules" {
                continue;
            }
            rust_files(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// How a construction site wired its oracle.
#[derive(Debug, PartialEq, Eq)]
enum Wiring {
    /// `.with_provenance_oracle(None)` — the letter of the contract and none
    /// of it.
    Nulled,
    /// A real oracle was passed.
    Wired,
    /// The builder was never called.
    Unwired,
}

/// The three-way reading, extracted from the walk so it can be shown a
/// known-bad source without a filesystem.
///
/// The needles are assembled with `concat!` so this file does not match itself.
/// It did, on the first run of the tightened scan: it reads source, this source
/// discusses the strings it looks for, and it duly reported itself.
fn wiring(src: &str) -> Wiring {
    const NULL_ORACLE: &str = concat!(".with_provenance", "_oracle(None)");
    const REAL_ORACLE: &str = concat!(".with_provenance", "_oracle(Some(");
    if src.contains(NULL_ORACLE) {
        Wiring::Nulled
    } else if src.contains(REAL_ORACLE) {
        Wiring::Wired
    } else {
        Wiring::Unwired
    }
}

/// The detector must tell a null oracle from a real one.
///
/// This scan has already been wrong once in exactly this way. It checked
/// `src.contains(".with_provenance_oracle(")` — the presence of the call,
/// nothing about its argument — so `.with_provenance_oracle(None)` satisfied it
/// completely, and a worker wired that way writes precisely the ungraded rules
/// the check exists to prevent. It was found by sabotage, by hand, after the
/// fact. Nothing in the build would have found it, and nothing in the build
/// would have found the next one.
#[test]
fn the_scan_sees_a_rule_written_without_a_floor() {
    let nulled = concat!(
        "    let worker = ConsolidationWorker::new(store)\n",
        "        .with_provenance",
        "_oracle(None);\n"
    );
    assert_eq!(
        wiring(nulled),
        Wiring::Nulled,
        "a null oracle reads as wired, which is the defect this scan already \
         had once"
    );

    let wired = concat!(
        "    let worker = ConsolidationWorker::new(store)\n",
        "        .with_provenance",
        "_oracle(Some(oracle.clone()));\n"
    );
    assert_eq!(wiring(wired), Wiring::Wired);

    assert_eq!(
        wiring("    let worker = ConsolidationWorker::new(store);\n"),
        Wiring::Unwired
    );
}

#[test]
fn every_consolidation_worker_is_given_a_provenance_oracle() {
    let root = repo_root();
    let mut files = Vec::new();
    rust_files(&root, &mut files);

    // The crate that defines the builder is not a caller of it.
    let definition = root.join("agent-bestiary/memory/src/consolidation.rs");

    let mut unwired: Vec<String> = Vec::new();
    let mut nulled: Vec<String> = Vec::new();
    let mut wired = 0usize;
    let mut exercised_exemptions: Vec<&str> = Vec::new();

    for file in &files {
        if file == &definition {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(file) else {
            continue;
        };
        if !src.contains("ConsolidationWorker::new")
            && !src.contains("ConsolidationWorker::with_llm")
        {
            continue;
        }
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");

        // A scanner must not scan itself. This file names the strings it looks
        // for — in the needles, and in the failure message that quotes them — so
        // without this it reports itself as a violation, which is what it did
        // on the first two runs after the check was tightened.
        if rel.ends_with("provenance_floor_coverage.rs") {
            continue;
        }

        if let Some((path, _why)) = EXEMPT.iter().find(|(p, _)| *p == rel) {
            exercised_exemptions.push(path);
            continue;
        }
        // `Some(`, not merely the call. See [`wiring`] and
        // [`the_scan_sees_a_rule_written_without_a_floor`]: this checked for
        // the presence of the call and nothing about its argument, so
        // `.with_provenance_oracle(None)` satisfied it completely.
        //
        // Which is this repository's own defect class, in the check built to
        // catch it: a spec-shaped artifact that is not spec-enforcing. The
        // remedy is the same one it prescribes elsewhere — assert the thing you
        // actually care about, not a proxy that is cheaper to satisfy.
        match wiring(&src) {
            Wiring::Nulled => nulled.push(rel),
            Wiring::Wired => wired += 1,
            Wiring::Unwired => unwired.push(rel),
        }
    }

    assert!(
        nulled.is_empty(),
        "these files call `.with_provenance_oracle(None)`, which satisfies the \
         letter of the contract and none of it — the worker still writes rules \
         with an UNKNOWN grounding floor:\n  {}\n\nPass a real oracle, or add \
         the file to EXEMPT with a reason someone can argue with.",
        nulled.join("\n  ")
    );

    assert!(
        unwired.is_empty(),
        "these files build a ConsolidationWorker without calling \
         `.with_provenance_oracle(...)`, so every rule they write records an \
         UNKNOWN grounding floor and will be injected into other agents' \
         prompts unlabelled:\n  {}\n\nWire it, or add the file to EXEMPT with \
         a reason.",
        unwired.join("\n  ")
    );

    // A scan that finds nothing to check is not a passing scan. If the call
    // sites are ever renamed or the builder replaced, this test would go
    // quietly green while enforcing nothing.
    assert!(
        wired >= 2,
        "found only {wired} wired construction site(s). Expected at least two \
         production writers (the HTTP consolidation handler and the creature \
         dream cycle). Fewer means the scan no longer matches the code it is \
         supposed to police."
    );

    // Exemptions must correspond to files that exist and still construct a
    // worker, so the list cannot rot into a set of permanent excuses for code
    // that has moved on.
    for (path, why) in EXEMPT {
        assert!(
            root.join(path).exists(),
            "EXEMPT names `{path}`, which does not exist. Remove the \
             exemption. Reason on file was: {why}"
        );
        assert!(
            exercised_exemptions.contains(path),
            "EXEMPT names `{path}`, but it no longer constructs a \
             ConsolidationWorker. Remove the exemption — a stale entry is a \
             standing permission nobody re-examined."
        );
    }
}

/// The floor must be written by every rule-construction site inside the
/// extraction engine, not just the LLM one.
///
/// There are three sites in `consolidation.rs` and a fourth is likely. A site
/// that forgets the field cannot compile — the struct has no default — but it
/// *can* pass `None` and look deliberate. Requiring the helper by name is the
/// difference between "this path decided the floor is unknown" and "this path
/// never thought about it".
#[test]
fn every_rule_construction_site_computes_a_floor() {
    let src =
        std::fs::read_to_string(repo_root().join("agent-bestiary/memory/src/consolidation.rs"))
            .expect("consolidation.rs");

    let constructions = src.matches("SemanticRule {").count();
    let floor_calls = src.matches("self.floor_for(").count();

    assert!(
        constructions > 0,
        "no SemanticRule constructions found — the scan has lost its target"
    );
    assert_eq!(
        constructions, floor_calls,
        "{constructions} SemanticRule construction site(s) but {floor_calls} \
         call(s) to `floor_for`. Every site must ask, because a site that \
         hardcodes `provenance_floor: None` is indistinguishable in the data \
         from one whose sources genuinely could not be graded — and the first \
         is a bug while the second is the state of the corpus."
    );
}
