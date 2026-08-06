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
