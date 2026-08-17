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
//! `handlers/creatures/agent_modules.rs` — is the highest-volume rule writer
//! on the platform, because creature dreams run on a timer while the HTTP
//! handler runs when somebody asks. Wiring the path you are looking at and
//! missing the path that runs by itself is the normal shape of this mistake,
//! and it does not announce itself: the rules still get written, they just
//! arrive ungraded.
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

#[test]
fn every_consolidation_worker_is_given_a_provenance_oracle() {
    let root = repo_root();
    let mut files = Vec::new();
    rust_files(&root, &mut files);

    // The crate that defines the builder is not a caller of it.
    let definition = root.join("agent-bestiary/memory/src/consolidation.rs");

    let mut unwired: Vec<String> = Vec::new();
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

        if let Some((path, _why)) = EXEMPT.iter().find(|(p, _)| *p == rel) {
            exercised_exemptions.push(path);
            continue;
        }
        if src.contains(".with_provenance_oracle(") {
            wired += 1;
        } else {
            unwired.push(rel);
        }
    }

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
