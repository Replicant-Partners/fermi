//! # An episode that never concluded is not a successful one
//!
//! An episode row is inserted in `running` when work is dispatched, and
//! transitioned to `success` or `failure` when it returns. If it never returns,
//! nothing reconciles the row — a `tokio::time::timeout` or a client
//! disconnect **drops the handler future**, so the code that would finalise the
//! episode never executes. Seven such rows exist, across five agents.
//!
//! Both daily aggregates in `src/handlers/metrics.rs` counted failures as
//! `COUNT(*) FILTER (WHERE execution_status = 'failure')`, which reports every
//! one of those rows as though it were fine.
//!
//! ## The incident
//!
//! `docs/plans/NOTE_CARBON_ZERO_TOKEN_EPISODE.md`. `carbon_accountant` read as
//! "2 executions, 1 failure" — a 50% success rate for an agent that has never
//! resolved a single emission factor. The second row was episode `ccf4ae90`,
//! still `running`, `tokens_used` NULL, `execution_time_ms` 0. It is the sole
//! reason the agent did not read as 100% failing.
//!
//! Two distinct wrongnesses, which is why the probe asserts them separately:
//!
//!   * the **count** — an unfinished run in neither the success nor the failure
//!     column reads as a success by subtraction;
//!   * the **duration** — `execution_time_ms` on such a row is a literal
//!     stored `0`, not a NULL, so averaging across all rows halves the
//!     reported latency. The carbon series read ~78s against a only real run
//!     of 157s, and that average is the input to the iteration-budget
//!     decision. Getting the count right and the duration wrong would have
//!     looked like a fix.
//!
//! ## Why a Postgres probe and not a unit test
//!
//! The thing that can be wrong here is SQL. A Rust test can only assert that
//! some text appears in a string literal, which proves the query was typed and
//! not that it aggregates correctly — and `FILTER` placement relative to a
//! `::BIGINT` cast is exactly the kind of thing that parses and then means
//! something else. `scripts/episode_metrics_probe.sh` spins a throwaway
//! cluster, seeds one `success`, one `failure` and one `running` episode, and
//! **executes the SQL read out of `metrics.rs`** rather than a restatement of
//! it, so the probe cannot drift from the handler.
//!
//! ## Skipped rather than assumed
//!
//! Where no local `initdb` exists the probe exits 0 after saying so on stderr,
//! and this test reports that it could not run. The same distinction the rest
//! of this codebase makes between `unverified` and `valid`.

use std::path::Path;
use std::process::Command;

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn an_unfinished_episode_is_not_counted_as_a_successful_one() {
    let probe = repo().join("scripts/episode_metrics_probe.sh");
    assert!(
        probe.exists(),
        "scripts/episode_metrics_probe.sh is gone, so nothing executes the \
         metrics SQL against a `running` episode any more"
    );

    let out = Command::new("bash")
        .arg("scripts/episode_metrics_probe.sh")
        .current_dir(repo())
        .output()
        .expect("run scripts/episode_metrics_probe.sh");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    if stderr.contains("SKIPPED") {
        eprintln!(
            "SKIPPED: no local postgres, so the episode metrics SQL was not \
             executed. This is an absence of a check, not a passing one.\n{stderr}"
        );
        return;
    }

    assert!(
        out.status.success(),
        "the episode metrics aggregates mis-report an unfinished run. An \
         episode in `running` is neither a success nor a failure, and it must \
         appear in `unfinished` and be excluded from `avg_time_ms`.\n\n\
         {stdout}\n{stderr}"
    );

    // A probe that ran nothing exits 0 too. The count is what distinguishes
    // "every assertion held" from "the extraction returned no SQL", which is
    // the failure mode the sibling suite in `inline_js_syntax.rs` was written
    // after — a check that reported OK for a page that did not load.
    let passes = stdout.matches("PASS").count();
    assert!(
        passes >= 14,
        "only {passes} assertion(s) ran in the probe; it asserts 14. The SQL \
         extraction from src/handlers/metrics.rs is probably broken, which \
         would make this vacuously pass.\n\n{stdout}"
    );

    // And the case the note is actually about: a day whose only episode never
    // concluded must not report a duration. A zero there reads as an instant
    // run and drags every average computed over the series.
    assert!(
        stdout.contains("PASS  avg_time_ms is NULL, not 0"),
        "the probe no longer checks that a day of nothing-but-unfinished runs \
         reports no duration rather than a zero.\n\n{stdout}"
    );
}
