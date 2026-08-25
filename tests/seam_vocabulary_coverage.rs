//! A declared token must not be re-spelled as a bare literal.
//!
//! # Why the literal and not the behaviour
//!
//! Because the behaviour is unobservable. A write carrying a token the column
//! rejects is refused, and on this codebase's swallowed paths the refusal goes
//! to a log line — so the sink stays empty, the liveness rung reads `Silent`,
//! and every remaining surface looks like a feature nobody has used yet. That
//! is the `L1` defect exactly, and it survived for the life of the feature.
//!
//! `tests/seam_vocabulary_contract.rs` compares Rust's declaration against the
//! live constraint. It can only compare what is *declared*. A literal typed at a
//! write site is invisible to it, which is why the literal is what gets fenced.
//!
//! # Reads and writes are not the same risk
//!
//! A wrong token in a **write** is refused and, here, usually swallowed.
//! A wrong token in a **read filter** returns no rows — which someone notices,
//! immediately, because the screen is empty. The two deserve different
//! treatment, so read-side filters are exempt **with a stated reason** rather
//! than silently ignored, and the sites that own them carry their own unit test
//! asserting the filter covers the declared set.
//!
//! # Trap
//!
//! A scan whose needles appear in its own source matches itself. The needles
//! here come from the declarations at runtime rather than being typed, and the
//! scanner skips its own file and the module that owns the tokens.

use std::path::{Path, PathBuf};

use fermi::seam_vocabulary::VOCABULARIES;

/// The module that owns every token.
const OWNER: &str = "src/seam_vocabulary.rs";

// Upstream owners are not listed here — they are read from the registry's
// `owned_by` field, so the exemption and the declaration cannot drift apart.
// A first version kept a hand-written list and immediately disagreed with the
// registry about `rate_card`, producing 64 hits on entirely correct code.

/// Read-side sites: `(file, line must contain, reason)`. May only shrink.
///
/// **Line-scoped, not file-scoped.** The first version exempted whole files,
/// and the deliberate break — putting a bare `"anomaly_delta"` back on the
/// write path — sailed through, because the write path is in the same file as
/// the read filter. An exemption broad enough to cover the thing it was not
/// written for is the defect this whole audit is about, reproduced in the
/// fence.
///
/// **Currently empty, and that is the point of the list.** The one entry it
/// ever held covered `process_spacetime_handler`, which spliced
/// `resolution_mode = 'any_reading'` into a WHERE clause. That filter now parses
/// the query string into `ResolutionMode` and binds it, so the read side and
/// the write side are the same closed type and there is nothing left to exempt.
/// An exemption is a debt; this one was paid rather than renewed.
const READ_FILTERS: &[(&str, &str, &str)] = &[];

fn rust_sources(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
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

/// Tokens worth fencing.
///
/// Two filters, both of which exist to stop the scan firing on correct code.
///
/// **Only registry-owned sets.** A vocabulary with an `owned_by` has an
/// upstream authority whose module — and whose legitimate callers, fixtures and
/// tests — spell its tokens constantly. Fencing the provenance ladder produced
/// 64 hits, none of them defects.
///
/// **Only distinctive tokens.** `tool`, `human`, `exact`, `over`, `under` are
/// ordinary English and appear everywhere for unrelated reasons. Nine
/// characters is the threshold that separates `any_reading` from `over`.
///
/// The surviving set is small, and the count is printed so the coverage is a
/// stated fact rather than an assumption.
fn fenced_tokens() -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for v in VOCABULARIES {
        if v.owned_by.is_some() {
            continue;
        }
        for t in v.tokens {
            if t.len() >= 9 {
                out.push((v.table, (*t).to_string()));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Does this line of **code** spell `token` as a literal?
///
/// Extracted from the loop below so the detector can be put in front of a
/// known-bad line without a filesystem — see
/// [`the_fence_sees_a_token_spelled_in_a_sql_string`]. Until that existed this
/// scan had never been shown able to fire by anything in the build; it was
/// broken by hand once, by its author, which is a habit rather than a property.
fn spells_token(line: &str, token: &str) -> bool {
    // Prose may discuss a token; code may not spell it.
    let code = line.trim_start();
    if code.starts_with("//") || code.starts_with('*') {
        return false;
    }
    // Quoted, in either Rust or embedded SQL.
    line.contains(&format!("\"{token}\"")) || line.contains(&format!("'{token}'"))
}

/// The detector must see the case the compiler cannot.
///
/// After the typed-enum conversion the write path binds `ResolutionMode`, so a
/// misspelt *variant* is a compile error and this scan is not what catches it.
/// What the compiler cannot see is a token typed inside a **SQL string**, and
/// that is the only thing left for this fence to do — so it is the thing that
/// has to be proved.
///
/// Confirmed against the tree, not only against these literals: putting
/// `AND resolution_mode = 'anomaly_delta'` back into a query in
/// `simops_benchmark.rs` made
/// [`no_declared_token_is_re_spelled_as_a_bare_literal`] report
/// `src/handlers/simops_benchmark.rs:172`. The same line, in the same shape,
/// is the first assertion below.
#[test]
fn the_fence_sees_a_token_spelled_in_a_sql_string() {
    assert!(
        spells_token(
            "               AND resolution_mode = 'anomaly_delta'",
            "anomaly_delta"
        ),
        "the fence cannot see a declared token spliced into SQL, which is the \
         only shape left that the type system does not already refuse"
    );
    assert!(spells_token(
        "    let m = \"anomaly_delta\";",
        "anomaly_delta"
    ));

    // And it must not fire on the fix. A check that flags correct code gets
    // deleted, and the deletion looks like cleanup (§5.2).
    assert!(!spells_token(
        "        modes.push((ResolutionMode::AnomalyDelta, None, None));",
        "anomaly_delta"
    ));
    assert!(!spells_token(
        "    // 'anomaly_delta' is the third mode",
        "anomaly_delta"
    ));
    assert!(!spells_token(
        "    let resolution_mode = mode.as_str();",
        "anomaly_delta"
    ));
}

#[test]
fn no_declared_token_is_re_spelled_as_a_bare_literal() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let this_file = repo.join(file!());

    let mut files = Vec::new();
    rust_sources(&repo.join("src"), &mut files);
    assert!(
        files.len() > 50,
        "the walker found {} files; a scan over an empty set passes for ever",
        files.len()
    );

    let tokens = fenced_tokens();
    assert!(
        tokens.len() >= 5,
        "only {} token(s) survive both filters, which is too few for this scan \
         to be worth trusting. If a vocabulary gained an `owned_by`, check that \
         the upstream module really is the authority.",
        tokens.len()
    );
    println!(
        "  fencing {} token(s) across {} files",
        tokens.len(),
        files.len()
    );

    let mut offenders: Vec<String> = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(repo)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let upstream = VOCABULARIES
            .iter()
            .any(|v| v.owned_by == Some(rel.as_str()));
        if rel == OWNER || path == &this_file || upstream {
            continue;
        }

        let Ok(body) = std::fs::read_to_string(path) else {
            continue;
        };
        for (lineno, line) in body.lines().enumerate() {
            // Line-scoped read-filter exemption.
            if READ_FILTERS
                .iter()
                .any(|(p, needle, _)| *p == rel && line.contains(needle))
            {
                continue;
            }
            for (table, t) in &tokens {
                if spells_token(line, t) {
                    offenders.push(format!("{rel}:{}  ({table}) {}", lineno + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "\n{} site(s) spell a declared token instead of referencing it:\n\n  {}\n\n\
         Use the constant from `fermi::seam_vocabulary` (or the upstream module \
         it points at). A literal is invisible to \
         `seam_vocabulary_contract`, which can only compare what is declared — \
         so a token the column rejects is refused at runtime, swallowed by the \
         write path, and reported as an empty table.\n",
        offenders.len(),
        offenders.join("\n  ")
    );
}

/// An exemption must carry a reason, and the list may only shrink.
#[test]
fn every_read_filter_exemption_is_justified_and_real() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    // No `assert!(!READ_FILTERS.is_empty())` here: an empty exemption list is
    // the goal state, not a scan that has stopped working. The scan itself is
    // the thing that must be proved non-vacuous, and
    // `no_declared_token_is_re_spelled_as_a_bare_literal` asserts its own
    // file and token counts for exactly that reason.
    for (path, needle, why) in READ_FILTERS {
        assert!(
            why.len() > 80,
            "{path}: an exemption without a reason is a permanent one"
        );
        let body = std::fs::read_to_string(repo.join(path))
            .unwrap_or_else(|e| panic!("{path} is exempted and unreadable: {e}"));
        // A stale needle exempts nothing and looks like it exempts something.
        assert!(
            body.contains(needle),
            "{path} no longer contains `{needle}`, so this exemption covers \
             nothing. Remove it — the list may only shrink."
        );
    }
    // An `owned_by` pointing at a file that does not exist would silently
    // exempt nothing and fence everything, or vice versa after a move.
    for v in VOCABULARIES {
        if let Some(owner) = v.owned_by {
            assert!(
                repo.join(owner).exists(),
                "{}.{} names `{owner}` as its token owner, and that file does \
                 not exist",
                v.table,
                v.column
            );
        }
    }
}
