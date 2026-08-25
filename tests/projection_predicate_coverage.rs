//! One definition of "this observation is a projection", enforced.
//!
//! # What this scan is for
//!
//! Three readers decided it inline:
//!
//! ```text
//! observations.rs      source == "simops_simulation"   -> skip
//! simops_tools.rs      source == "simops_simulation"   -> commit
//! eval_projection.rs   source =  'simops_simulation'   -> match
//! ```
//!
//! and the producer writes `source_kind = "dynamics_projection"`. Every reader
//! agreed with every other reader and all of them selected the empty set:
//! **12,167** projection rows on file, **0** carrying the tag they matched.
//! Loop 5.A (projection accuracy) has never written a signal and this is the
//! first of the three reasons.
//!
//! No reader was wrong on its own terms. The defect is only visible across
//! files, and only against row counts — which is why a review would not have
//! found it and a scan will.
//!
//! # Why the literal and not the behaviour
//!
//! Because the behaviour is unobservable while the tag matches nothing: a
//! lookup that returns no rows is indistinguishable from a world where nothing
//! has happened. The literal is the thing that can be grepped, so the literal
//! is what is fenced. `fermi::projection_kind` owns both tag constants and both
//! predicates; everyone else asks it.
//!
//! # Trap, and how it is avoided
//!
//! A scan whose needle appears in its own source matches itself and passes for
//! the wrong reason — twice over, because the failure message quotes the needle
//! too. The needles below are assembled with `concat!` so no contiguous copy
//! exists in this file, and the walker skips its own path regardless.

use std::path::{Path, PathBuf};

/// The module allowed to name the tags. Everything else must ask it.
const OWNER: &str = "src/projection_kind.rs";

/// Files that may still name a tag, with a reason. May only shrink.
///
/// `simops_tools.rs` writes `source = "simops_simulation"` when it creates a
/// synthetic observation — that is the *producer* stamping the tag, which is
/// the one legitimate reason to name it, and it is not a predicate.
const WRITERS: &[(&str, &str)] = &[(
    "src/agent_backend/simops_tools.rs",
    "stamps the tag onto observations it creates; a producer must name what it \
     writes. It no longer READS the tag — the classification comes from \
     projection_kind::is_projection.",
)];

fn needles() -> [String; 2] {
    // Assembled, so this file contains no contiguous copy of either literal.
    [
        concat!("simops_", "simulation").to_string(),
        concat!("dynamics_", "projection").to_string(),
    ]
}

fn rust_sources(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            let skip = matches!(
                p.file_name().and_then(|s| s.to_str()),
                Some("target") | Some("node_modules") | Some(".git")
            );
            if !skip {
                rust_sources(&p, out);
            }
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

/// Does this line of **code** name one of the projection tags?
///
/// Extracted so the detector can be shown a known-bad line without a
/// filesystem — see [`the_scan_sees_a_tag_spelled_outside_the_owning_module`].
fn names_a_tag(line: &str, needles: &[String]) -> bool {
    // Prose is allowed to discuss the tags; code may not test them.
    let code = line.trim_start();
    if code.starts_with("//") || code.starts_with('*') {
        return false;
    }
    needles.iter().any(|n| line.contains(n.as_str()))
}

/// The detector must see a hand-rolled tag test.
///
/// The offending line is built from
/// [`fermi::projection_kind::SOURCE_KIND_DYNAMICS_PROJECTION`] rather than
/// typed, for two reasons. It keeps this file free of a contiguous copy of the
/// literal, which is the discipline `needles()` already follows. And it makes
/// the fixture come from the owning module, so a rename there moves the
/// falsification with it instead of leaving a test that passes against a tag no
/// row carries — which is, precisely, the incident this scan exists for.
#[test]
fn the_scan_sees_a_tag_spelled_outside_the_owning_module() {
    let needles = needles();
    let tag = fermi::projection_kind::SOURCE_KIND_DYNAMICS_PROJECTION;

    assert!(
        names_a_tag(
            &format!("        if extra[\"source_kind\"] == \"{tag}\" {{"),
            &needles
        ),
        "the scan cannot see a hand-rolled tag comparison, which is the only \
         thing it is for"
    );
    assert!(
        names_a_tag(
            &format!("           AND extra->>'source_kind' = '{tag}'"),
            &needles
        ),
        "the scan cannot see the tag inside SQL, and the SQL readers are where \
         the 12,167 rows were lost"
    );

    // Prose may name it, and the fixed form must not fire — a scan that flags
    // the remedy is a scan that gets deleted.
    assert!(!names_a_tag(
        &format!("        // rows carrying {tag}"),
        &needles
    ));
    assert!(!names_a_tag(
        "        if projection_kind::is_projection(&extra) {",
        &needles
    ));
}

#[test]
fn only_one_module_names_the_projection_tags() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let this_file = repo.join(file!());

    let mut files = Vec::new();
    rust_sources(&repo.join("src"), &mut files);
    rust_sources(&repo.join("crates"), &mut files);
    rust_sources(&repo.join("tests"), &mut files);
    assert!(
        files.len() > 50,
        "the walker found {} files, which is too few to have walked anything. \
         A scan over an empty set passes for ever.",
        files.len()
    );

    let needles = needles();
    let mut offenders: Vec<String> = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(repo)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel == OWNER || path == &this_file {
            continue;
        }
        if WRITERS.iter().any(|(p, _)| *p == rel) {
            continue;
        }

        let Ok(body) = std::fs::read_to_string(path) else {
            continue;
        };
        for (lineno, line) in body.lines().enumerate() {
            if names_a_tag(line, &needles) {
                offenders.push(format!("{rel}:{}: {}", lineno + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "\n{} site(s) decide what a projection is without asking \
         `fermi::projection_kind`:\n\n  {}\n\n\
         Use `projection_kind::is_projection` / `is_measurement` in Rust, or \
         the `is_projection_sql!` macro in SQL. Every reader that hand-rolled \
         this agreed with every other reader and all of them matched a tag no \
         row has ever carried; the disagreement was with the WRITER, and it \
         cost Loop 5.A (projection accuracy) its entire input.\n",
        offenders.len(),
        offenders.join("\n  ")
    );
}

/// An exemption must come with a reason, and the list may only shrink.
#[test]
fn every_exempt_writer_gives_a_reason_and_still_exists() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (path, why) in WRITERS {
        assert!(
            why.len() > 40,
            "{path}: an exemption without a reason is a permanent one"
        );
        assert!(
            repo.join(path).exists(),
            "{path} is exempted and does not exist. A stale exemption is a hole \
             in the fence nobody can see."
        );
    }
}
