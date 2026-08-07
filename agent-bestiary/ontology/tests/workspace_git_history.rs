//! Spec 31: the git operations forecast version history depends on.
//!
//! These drive a real git repo in a temp dir — no mocks. The substrate was
//! shipped and left idle (one commit across every workspace in production),
//! so "it compiles" was never evidence that it worked. The three properties
//! asserted here are the ones the collaboration model actually rests on:
//!
//!   1. commits carry the ACTING HUMAN, not the platform — otherwise
//!      "which teammate changed this" is unanswerable no matter how much we
//!      commit;
//!   2. one action is ONE commit across several files — otherwise the log
//!      is unreadable and diffs are meaningless;
//!   3. earlier content is RECOVERABLE — otherwise there is no revert, and
//!      without revert shared `edit` stays frightening.

use agent_bestiary_ontology::types::GitConfig;
use agent_bestiary_ontology::{CommitAuthor, WorkspaceGitManager};

fn manager() -> (WorkspaceGitManager, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = GitConfig {
        base_path: dir.path().to_string_lossy().to_string(),
        author_name: "Fermi System".into(),
        author_email: "system@fermi.test".into(),
        // auto_push off: these tests must not touch a network or a remote.
        auto_push: false,
        ..GitConfig::default()
    };
    (WorkspaceGitManager::new(cfg).expect("manager"), dir)
}

/// `init_or_open` seeds every new repo with a `workspace(<slug>): initial
/// structure` commit (README + .gitkeep) authored by the system identity.
/// That is deliberate and part of the real history, so these tests count
/// only the commits they themselves make rather than asserting on totals.
fn authored_by(git: &WorkspaceGitManager, slug: &str, name: &str) -> usize {
    git.get_log(slug, 50)
        .expect("log")
        .iter()
        .filter(|c| c.author == name)
        .count()
}

fn files(fpl: &str, prob: &str) -> Vec<(String, String)> {
    vec![
        ("forecast.fpl".to_string(), fpl.to_string()),
        (
            "state.json".to_string(),
            format!("{{\n  \"predicted_probability\": {}\n}}\n", prob),
        ),
    ]
}

/// The whole point: a commit must be attributable to the person who made it.
#[test]
fn commits_carry_the_acting_human_not_the_platform() {
    let (git, _d) = manager();
    let alice = CommitAuthor {
        name: "Alice Labra".into(),
        email: "alice@example.test".into(),
    };

    git.commit_files_as(
        "f-1",
        &files("driver a = 1", "0.40"),
        "created",
        Some(&alice),
    )
    .expect("commit")
    .expect("a first commit always changes the tree");

    // Newest first, so the commit under test leads the log.
    let log = git.get_log("f-1", 10).expect("log");
    assert_eq!(
        log[0].author, "Alice Labra",
        "commit was attributed to the platform instead of the actor — this is \
         exactly the gap that made teammate attribution impossible"
    );
    assert_eq!(authored_by(&git, "f-1", "Alice Labra"), 1);
}

/// A systemic write (cron, refit with no operator) legitimately has no human
/// behind it and must fall back rather than inventing one.
#[test]
fn absent_author_falls_back_to_the_system_identity() {
    let (git, _d) = manager();
    git.commit_files_as("f-2", &files("x = 1", "0.10"), "refit", None)
        .expect("commit")
        .expect("first commit");

    let log = git.get_log("f-2", 10).expect("log");
    assert_eq!(log[0].author, "Fermi System");
    assert!(log[0].message.contains("refit"));
}

/// One logical action changes the program, the drivers and the probability.
/// Three commits for one act would make the history unreadable.
#[test]
fn one_action_is_one_commit_across_several_files() {
    let (git, _d) = manager();
    let bo = CommitAuthor {
        name: "Bo".into(),
        email: "bo@example.test".into(),
    };

    git.commit_files_as("f-3", &files("a = 1", "0.40"), "created", Some(&bo))
        .unwrap()
        .unwrap();
    git.commit_files_as("f-3", &files("a = 2", "0.55"), "revised", Some(&bo))
        .unwrap()
        .unwrap();

    assert_eq!(
        authored_by(&git, "f-3", "Bo"),
        2,
        "two actions must produce exactly two commits, not one per changed file"
    );
}

/// The commit hook fires on every mutating request, including ones that
/// change nothing. Those must not become empty commits — and must be
/// distinguishable, so the caller can skip its DB bookkeeping.
#[test]
fn unchanged_state_is_a_no_op_not_an_empty_commit() {
    let (git, _d) = manager();
    let f = files("a = 1", "0.40");

    assert!(git
        .commit_files_as("f-4", &f, "created", None)
        .unwrap()
        .is_some());
    let after_first = git.get_log("f-4", 10).unwrap().len();

    assert!(
        git.commit_files_as("f-4", &f, "saved again", None)
            .unwrap()
            .is_none(),
        "re-committing identical state must report None rather than \
         fabricating a commit"
    );
    assert_eq!(
        git.get_log("f-4", 10).unwrap().len(),
        after_first,
        "a no-op save must not add a commit"
    );
}

/// Revert's prerequisite. `diff_commits` could already say what changed;
/// without this, nothing could recover the earlier content.
#[test]
fn earlier_content_is_recoverable_at_a_sha() {
    let (git, _d) = manager();

    let first = git
        .commit_files_as("f-5", &files("driver a = 1", "0.40"), "created", None)
        .unwrap()
        .unwrap();
    git.commit_files_as("f-5", &files("driver a = 999", "0.90"), "revised", None)
        .unwrap()
        .unwrap();

    // HEAD has the new value…
    assert!(git
        .read_file("f-5", "forecast.fpl")
        .unwrap()
        .contains("999"));

    // …and the old one is still retrievable, which is what revert restores.
    let old = git
        .read_file_at("f-5", "forecast.fpl", &first.sha)
        .expect("read at sha")
        .expect("file existed at that commit");
    assert!(old.contains("driver a = 1"), "got: {}", old);
    assert!(!old.contains("999"));
}

/// A path added later did not exist earlier. That's an answer, not a failure
/// — revert must be able to tell "absent then" from "repo broken".
#[test]
fn a_path_absent_at_that_commit_is_none_not_an_error() {
    let (git, _d) = manager();
    let first = git
        .commit_files_as("f-6", &files("a = 1", "0.40"), "created", None)
        .unwrap()
        .unwrap();
    git.commit_files_as(
        "f-6",
        &[("evidence.json".to_string(), "[]".to_string())],
        "added evidence",
        None,
    )
    .unwrap()
    .unwrap();

    assert!(
        git.read_file_at("f-6", "evidence.json", &first.sha)
            .expect("must not error")
            .is_none(),
        "a file that did not exist yet must read as None, not an error"
    );
}

/// The diff is what the console renders as "what did they change".
#[test]
fn diff_between_revisions_shows_the_actual_change() {
    let (git, _d) = manager();
    let a = git
        .commit_files_as("f-7", &files("elo = 1780", "0.40"), "created", None)
        .unwrap()
        .unwrap();
    let b = git
        .commit_files_as("f-7", &files("elo = 1815", "0.47"), "revised", None)
        .unwrap()
        .unwrap();

    let diff = git.diff_commits("f-7", &a.sha, &b.sha).expect("diff");
    assert!(
        diff.contains("1780"),
        "old value missing from diff:\n{}",
        diff
    );
    assert!(
        diff.contains("1815"),
        "new value missing from diff:\n{}",
        diff
    );
}

/// Repos are per-workspace, and a forecast's history must never bleed into
/// another's — they share one base directory.
#[test]
fn workspaces_are_isolated_from_each_other() {
    let (git, _d) = manager();
    git.commit_files_as("f-8a", &files("a = 1", "0.10"), "created a", None)
        .unwrap()
        .unwrap();
    git.commit_files_as("f-8b", &files("b = 2", "0.20"), "created b", None)
        .unwrap()
        .unwrap();

    let a = git.get_log("f-8a", 10).unwrap();
    let b = git.get_log("f-8b", 10).unwrap();
    assert!(a[0].message.contains("created a"));
    assert!(b[0].message.contains("created b"));
    // Neither log may contain the other's work.
    assert!(!a.iter().any(|c| c.message.contains("created b")));
    assert!(!b.iter().any(|c| c.message.contains("created a")));
}

// ─────────────────────────────────────────────────────────────────────
// The path the console actually takes
//
// `diff_between_revisions_shows_the_actual_change` passes two full shas,
// which is the one input shape the old `Oid::from_str` implementation
// accepted. The History pane doesn't do that — it asks "what did this
// commit change", which is a question about a commit and its parent. That
// gap is why a diff endpoint that had a passing test shipped in v0.11.11
// and never rendered a single diff in production.
// ─────────────────────────────────────────────────────────────────────

/// A revision string, not just a raw sha. This is the assertion the
/// original test was missing.
#[test]
fn diff_accepts_git_revision_syntax_not_only_raw_shas() {
    let (git, _d) = manager();
    git.commit_files_as("f-9", &files("elo = 1780", "0.40"), "created", None)
        .unwrap()
        .unwrap();
    let b = git
        .commit_files_as("f-9", &files("elo = 1815", "0.47"), "revised", None)
        .unwrap()
        .unwrap();

    // `<sha>^` is how git spells "the parent of", and what the forecast
    // history handler sent for every request it ever made.
    let diff = git
        .diff_commits("f-9", &format!("{}^", b.sha), &b.sha)
        .expect("parent syntax must resolve");
    assert!(diff.contains("1780") && diff.contains("1815"), "{}", diff);

    // Abbreviated shas too — anything rev-parse takes.
    let short = &b.sha[..8];
    git.diff_commits("f-9", &format!("{}^", short), short)
        .expect("abbreviated sha must resolve");
}

/// "What did this commit change?" — including for the commit that has no
/// parent to compare against.
#[test]
fn diff_with_parent_works_on_every_commit_including_the_root() {
    let (git, _d) = manager();
    let a = git
        .commit_files_as("f-10", &files("elo = 1780", "0.40"), "created", None)
        .unwrap()
        .unwrap();
    let b = git
        .commit_files_as("f-10", &files("elo = 1815", "0.47"), "revised", None)
        .unwrap()
        .unwrap();

    let latest = git.diff_commit_with_parent("f-10", &b.sha).expect("diff b");
    assert!(
        latest.contains("1780") && latest.contains("1815"),
        "{}",
        latest
    );

    // `a` is not the root — `init_or_open` seeds every repo with an
    // `initial structure` commit — but it is still a normal parent diff.
    let first = git.diff_commit_with_parent("f-10", &a.sha).expect("diff a");
    assert!(first.contains("1780"), "{}", first);

    // The real root. `<sha>^` is an ERROR here, not an empty diff, which
    // is why this needs its own method rather than string-building a rev.
    // Identified by message, not by position: log order is a separate
    // concern, asserted by `log_order_never_puts_a_child_before_its_parent`.
    let log = git.get_log("f-10", 50).unwrap();
    let root = log
        .iter()
        .find(|c| c.message.contains("initial structure"))
        .expect("every repo is seeded with a root commit");
    assert!(
        git.diff_commits("f-10", &format!("{}^", root.sha), &root.sha)
            .is_err(),
        "the root has no parent; `^` must fail rather than silently succeed"
    );
    let root_diff = git
        .diff_commit_with_parent("f-10", &root.sha)
        .expect("the root must diff against the empty tree, not error");
    assert!(
        !root_diff.is_empty(),
        "a root commit's whole tree reads as added"
    );
}

/// The History pane's header says "newest first". This is that promise.
///
/// Git commit timestamps have ONE-SECOND resolution and these commits are
/// written milliseconds apart — a repo's seeded `initial structure` commit
/// and the first real save land in the same second almost every time. Under
/// `Sort::TIME` alone the tie was broken arbitrarily, and the log really did
/// come back as [revised, initial structure, created]: the root commit
/// sandwiched between two of its own descendants.
///
/// No sleeps here. Same-second commits are the case that breaks, so the
/// test has to reproduce it rather than tiptoe around it.
#[test]
fn log_order_never_puts_a_child_before_its_parent() {
    let (git, _d) = manager();

    let mut created: Vec<String> = Vec::new();
    for i in 0..6 {
        let c = git
            .commit_files_as(
                "f-11",
                &files(&format!("elo = {}", 1700 + i), "0.40"),
                &format!("revision {}", i),
                None,
            )
            .unwrap()
            .unwrap();
        created.push(c.sha);
    }

    let log = git.get_log("f-11", 50).expect("log");
    let got: Vec<&str> = log.iter().map(|c| c.sha.as_str()).collect();

    // History is linear here, so "newest first" has exactly one correct
    // answer: creation order reversed, with the seeded root commit last.
    let mut want: Vec<&str> = created.iter().rev().map(String::as_str).collect();
    let root = log
        .iter()
        .find(|c| c.message.contains("initial structure"))
        .expect("every repo is seeded with a root commit");
    want.push(&root.sha);

    assert_eq!(
        got,
        want,
        "log is not newest-first.\n  got:  {:?}\n  want: {:?}",
        log.iter()
            .map(|c| c.message.lines().next().unwrap_or(""))
            .collect::<Vec<_>>(),
        "revision 5..0 then initial structure",
    );
}
