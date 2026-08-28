//! Every delegation root must say whether it has a parent, and why not.
//!
//! # The finding
//!
//! `episodes.parent_episode_id` is the correction and delegation chain, and it is
//! non-null on **4 of 3,576 rows**. The obvious diagnosis is that nothing writes
//! it. That diagnosis is wrong, and it is worth recording because it is the shape
//! this codebase keeps producing: `tools_legacy.rs` writes the column
//! (`episode.parent_episode_id = ctx.parent_episode_id`), both execute paths
//! populate the context that feeds it, and the chain is thin because **delegation
//! is rare** and because four of the ten `ToolContext` construction sites
//! legitimately have no episode to point at.
//!
//! So there is no missing writer. What was missing is enforcement of a discipline
//! the code was already following by hand: of the four sites passing `None`,
//! **three carried a reason and one did not**. That one was
//! `handlers::workspace::coherence`, and its silence was indistinguishable from an
//! oversight — which is exactly the state a reader cannot resolve without going
//! and reading the whole handler.
//!
//! # Why a scan and not a type
//!
//! The type-level fix would make the field impossible to leave unexplained —
//! `Option<Uuid>` becomes an enum with a `NoParent { because }` arm. That is the
//! better answer and it is a wider change than this earns: `ToolContext` is
//! constructed at ten sites across seven files, and the field is read by
//! delegation code in `tools_legacy` that predates all of this. So the enforcement
//! is a scan, and it says so.
//!
//! # What it cannot do
//!
//! It proves a reason is present, not that the reason is true. Three of the four
//! present reasons say *"this path persists no episode of its own"*, and that is a
//! claim about the handler that only reading it can settle — this suite checked
//! `coherence` by hand (it imports `agent_output_to_episode` and never calls it)
//! and cannot check the next one for you.

use std::fs;
use std::path::{Path, PathBuf};

/// The field whose absence must be argued.
const FIELD: &str = "parent_episode_id: None";

/// The struct whose `None` needs an argument.
///
/// # Why the scan is narrowed to this and not to the field
///
/// The first version matched the field anywhere and immediately found five more
/// sites — in `Episode` literals, not `ToolContext` ones. Those are a different
/// fact and the scan was wrong to conflate them:
///
/// * `ToolContext { parent_episode_id: None }` says *anything delegated from here
///   will be recorded as a root.* It is a statement about a whole subtree, made by
///   the caller, and its consequence is invisible at the site.
/// * `Episode { parent_episode_id: None }` says *this row has no parent*, which is
///   true of 3,572 of 3,576 episodes and is the ordinary case. Demanding a
///   paragraph for it would be a check that fires on correct code, and §5.2 says
///   what happens next: it gets deleted, and the deletion looks like cleanup.
///
/// So a scan must be no broader than the property it asserts — the same rule as
/// an exemption, pointed the other way. Caught by the scan's own first run.
const ENCLOSING: &str = "ToolContext {";

/// How much reason is a reason.
///
/// Modelled on `surface::door_problems`' floor for `why_manual` and
/// `panel_absence`'s for `Resolver::Unresolved`. Long enough that it cannot be
/// satisfied by `// none`, short enough not to reward padding.
const MIN_REASON: usize = 60;

fn rust_sources(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if !matches!(
                p.file_name().and_then(|s| s.to_str()),
                Some("target") | Some("node_modules") | Some(".git")
            ) {
                rust_sources(&p, out);
            }
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

/// Which struct literal encloses this line, by walking upwards to the nearest
/// opening.
///
/// Deliberately naive: it finds the nearest line ending in `{` that also names a
/// capitalised type. That is enough to tell `ToolContext` from `Episode`, which is
/// the only distinction this scan needs, and it fails **closed** — an
/// unrecognisable enclosure returns `None` and the site is skipped rather than
/// demanded of, so a reformat cannot turn this into a check that fires on
/// everything.
fn enclosing_struct(lines: &[&str], idx: usize) -> Option<String> {
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let t = lines[i].trim();
        if let Some(head) = t.strip_suffix('{') {
            let name = head.trim().rsplit(&[' ', '(', ':'][..]).next()?.trim();
            let name = name.trim_start_matches("::");
            if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                return Some(format!("{name} {{"));
            }
        }
    }
    None
}

/// The contiguous comment block immediately above a line, if any.
///
/// Walks upwards, so an argument may be as many lines as it needs. Blank lines
/// end the block: a comment separated from the field by whitespace is explaining
/// something else, and accepting it would let any nearby prose serve as the
/// reason.
fn reason_above(lines: &[&str], idx: usize) -> String {
    let mut collected: Vec<&str> = Vec::new();
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let t = lines[i].trim();
        if t.starts_with("//") {
            collected.push(t.trim_start_matches('/').trim());
        } else {
            break;
        }
    }
    collected.reverse();
    collected.join(" ")
}

#[test]
fn every_unparented_delegation_root_says_why() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&repo.join("src"), &mut files);
    assert!(!files.is_empty(), "no sources found; the walk is broken");

    let mut unexplained = Vec::new();
    let mut explained = 0usize;

    for path in &files {
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        if !src.contains(FIELD) {
            continue;
        }
        let lines: Vec<&str> = src.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            // The declaration itself, not a comment mentioning it — this file's
            // own prose names the field constantly.
            if !t.starts_with(FIELD) {
                continue;
            }
            // Only a delegation root. See `ENCLOSING`.
            match enclosing_struct(&lines, i) {
                Some(name) if name == ENCLOSING => {}
                _ => continue,
            }
            let reason = reason_above(&lines, i);
            if reason.len() < MIN_REASON {
                unexplained.push(format!(
                    "{}:{}  reason is {} char(s), needs {MIN_REASON}",
                    path.strip_prefix(&repo).unwrap_or(path).display(),
                    i + 1,
                    reason.len()
                ));
            } else {
                explained += 1;
            }
        }
    }

    assert!(
        unexplained.is_empty(),
        "\n\n  {} delegation root(s) pass `{FIELD}` with no argument:\n\n    {}\n\n\
         A child delegated from here is recorded as a root, so the delegation \
         tree loses a branch and `episodes.parent_episode_id` — already non-null \
         on only 4 of 3,576 rows — loses another. Either supply the parent, or \
         say why there is none: the three sites that already do say `this path \
         persists no episode of its own`, which is a real and checkable reason.\n",
        unexplained.len(),
        unexplained.join("\n    ")
    );

    // Non-vacuity. If the field is renamed this suite silently checks nothing,
    // and a scan that has stopped matching looks exactly like a clean tree.
    assert!(
        explained >= 4,
        "only {explained} explained site(s) found, expected at least 4. \
         `{FIELD}` inside a `{ENCLOSING}` has probably been renamed or \
         reformatted, and this scan is now asserting nothing."
    );
    println!("  {explained} unparented delegation root(s), each with a reason.");
}

/// The scan can tell an argued `None` from a bare one.
///
/// Its falsifier. Both directions, because a checker that rejected everything
/// would pass the assertion above only while the tree happened to be empty.
#[test]
fn the_scan_sees_a_bare_none_and_accepts_an_argued_one() {
    let bare: Vec<&str> = vec!["let ctx = ToolContext {", "    parent_episode_id: None,"];
    assert_eq!(
        enclosing_struct(&bare, 1).as_deref(),
        Some(ENCLOSING),
        "the enclosure finder does not recognise a `ToolContext` literal, so the \
         scan above is skipping every site and asserting nothing"
    );
    assert!(
        reason_above(&bare, 1).len() < MIN_REASON,
        "a `None` with no comment above it was accepted as argued"
    );

    // And an `Episode` literal must NOT be demanded of: 3,572 of 3,576 episodes
    // have no parent and that is the ordinary case.
    let episode: Vec<&str> = vec![
        "        let ep = Episode {",
        "            parent_episode_id: None,",
    ];
    assert_ne!(
        enclosing_struct(&episode, 1).as_deref(),
        Some(ENCLOSING),
        "an `Episode` literal was treated as a delegation root, which would make \
         this scan fire on the ordinary case"
    );

    let argued: Vec<&str> = vec![
        "let ctx = ToolContext {",
        "    // This path persists no episode of its own, so there is nothing for",
        "    // a child to point at: anything delegated from here is a root.",
        "    parent_episode_id: None,",
    ];
    assert!(
        reason_above(&argued, 3).len() >= MIN_REASON,
        "a real two-line argument was rejected, so the scan would fire on \
         correct code — and §5.2 says what happens to a check that cries wolf"
    );

    // A comment separated by a blank line is explaining something else. Accepting
    // it would let any nearby prose serve as the argument, which is how an
    // exemption becomes broader than the thing it exempts.
    let detached: Vec<&str> = vec![
        "    // A long comment about something entirely unrelated to lineage,",
        "    // which happens to sit above this field with a gap in between.",
        "",
        "    parent_episode_id: None,",
    ];
    assert!(
        reason_above(&detached, 3).len() < MIN_REASON,
        "a detached comment was counted as the argument for this field"
    );
}
