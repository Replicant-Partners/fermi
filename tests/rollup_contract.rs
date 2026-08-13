//! # Rollup contract harness — catching columns that lie
//!
//! The check that would have caught `agents.total_executions`.
//!
//! ## What went wrong, and why nothing caught it
//!
//! `agents` carries five denormalised counters — `total_executions`,
//! `successful_executions`, `failed_executions`, `total_cost_usd`,
//! `avg_execution_time_ms` — added with the table and never wired to the
//! execution path. No code path ever ran `UPDATE agents SET
//! total_executions`. Meanwhile `episodes` recorded every run faithfully.
//!
//! Result: 3 of 743 agent rows had a non-zero rollup while 196 agents had
//! real episodes totalling ~$296 of measured spend. Six surfaces read the
//! empty ledger and served zeros to users:
//!
//!   1. console marketplace — every agent unpriced and ranked never-run
//!   2. Dashboard Research card — "cost n/a" on research that cost money
//!   3. public profiles (`users.rs`) — 0 runs for everyone
//!   4. own profile (`profile.rs`) — same
//!   5. orchestra membership inbox — 0 runs for every applicant
//!   6. ecology lens — 0 runs / $0.0000 as "vital signs" for the population
//!
//! Plus `admin_cleanup_test_cruft_handler`, whose `total_executions = 0`
//! deletion-safety predicate was therefore *always true* — a guard that
//! read like defense-in-depth and enforced nothing.
//!
//! **Every existing guard passed the entire time**, because all of them
//! reason about shape and this was a content failure:
//!
//! | Guard | Why it missed this |
//! | --- | --- |
//! | `schema_trust` boot probe | Declares the column and checks presence. It was present. |
//! | `SCHEMA_STRICT=1` | Same probe — nothing to abort on. |
//! | `lint-schema-consistency.py` | Flags refs to columns that don't exist. This one existed. |
//! | `schema_contract_check.sh` | Asserts the contract is satisfiable. It was. |
//! | Type checking | `i32` is `i32` whether it means 253 or 0. |
//! | Unit tests | Fixtures set the counters by hand, so they were never zero under test. |
//!
//! ## What this harness asserts
//!
//! **Tier 1 (offline, always runs).** Static: no request-serving handler
//! reads a write-orphaned column as truth. This is the tripwire — it fails
//! the moment a seventh surface reaches for the dead ledger, with no
//! database required, so it works in CI and in the pre-commit hook.
//!
//! **Tier 2 (live, `--ignored`, needs `DATABASE_URL`).** Content: the
//! source-of-truth view exists and returns real numbers; the columns it
//! replaces genuinely disagree with it (proving retirement was warranted);
//! and no `Maintained` rollup has drifted from its source.
//!
//! Run Tier 2 with `scripts/rollup_contract_live.sh`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fermi::rollup_trust::{
    reader_is_exempt, write_orphaned_columns, Disposition, ROLLUP_CONTRACTS, SCANNED_ROOTS,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` file under a scanned root.
fn rust_files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Strip `//`-comments and doc comments from a line.
///
/// Comments naming a dead column are not reads of it — this file, and every
/// rewritten call site, explains the bug in prose and would otherwise trip
/// its own tripwire. Crude but adequate: we only need to avoid false
/// positives on explanatory text, and a column name inside a string literal
/// (a SQL fragment) must still count as a read.
fn strip_comments(line: &str) -> &str {
    strip_comments_impl(line)
}

fn strip_comments_impl(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("/*") {
        return "";
    }
    match line.find("//") {
        // Only treat `//` as a comment when it isn't inside a string. A
        // bare heuristic: if there's an odd number of quotes before it,
        // we're inside a literal (e.g. a URL) — keep the whole line.
        Some(i) if line[..i].matches('"').count() % 2 == 0 => &line[..i],
        _ => line,
    }
}

/// Why a line counts as reading a write-orphaned column.
///
/// ## Why the column NAME alone isn't the signal
///
/// The first version of this check flagged any line containing
/// `total_executions` and reported 23 hits — almost all of them legitimate.
/// The name survives the fix in three innocent shapes, because the *wire
/// contract* keeps the old key while the *data source* changes:
///
/// ```text
///   "total_executions": m.executions,                      JSON output key
///   COALESCE(r.executions, 0) AS total_executions           SQL result alias
///   r.try_get::<i64, _>("total_executions")                 read of that alias
/// ```
///
/// A check that cannot tell those from a real read is a check nobody will
/// keep: it fails on correct code, so the first person to hit it deletes it.
///
/// The precise signal is a **qualified read** — `a.total_executions`,
/// `agents.total_executions` — because the replacement view deliberately
/// does *not* have a column of that name (it's `executions`). So any
/// `something.total_executions` is necessarily the dead column, in Rust
/// (struct field on an `Agent` row) or in SQL (qualified column ref). That
/// is the exact shape of all six original bugs.
///
/// The fallback rule catches an unqualified reference that isn't one of the
/// three innocent shapes — e.g. `SELECT total_executions FROM agents`.
fn read_reason(line: &str, column: &str) -> Option<&'static str> {
    let code = strip_comments(line);
    if !code.contains(column) {
        return None;
    }

    // Rule 1 — qualified read: `<ident>.<column>`. Unambiguous.
    //
    // Check EVERY occurrence, not just the first. `"total_executions":
    // a.total_executions,` — the pre-fix observatory line — opens with the
    // innocent JSON key and only then commits the sin. Scanning the first
    // match alone declared that line clean, which is how a detector ends up
    // certifying the exact code it was written to catch.
    for (i, _) in code.match_indices(column) {
        let before = code[..i].trim_end();
        if !before.ends_with('.') {
            continue;
        }
        // `..column` (Rust range) or `.  column` aren't field accesses; a
        // real qualifier ends in an identifier character.
        let ident = before.trim_end_matches('.');
        if ident
            .chars()
            .last()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            return Some(
                "qualified read of the dead column (`x.<column>`) — the \
                 replacement view has no column of this name, so this can \
                 only be the `agents` row",
            );
        }
    }

    // Rule 2 — the three innocent shapes, which keep the wire key while
    // sourcing the value from the rollup.
    let lower = code.to_lowercase();
    let is_result_alias =
        lower.contains(&format!("as {column}")) || lower.contains(&format!("as \"{column}\""));
    let is_json_key = code.contains(&format!("\"{column}\":"));
    let is_alias_getter = lower.contains("try_get") && code.contains(&format!("\"{column}\""));
    // `total_executions: 0,` in a struct literal, or a field declaration.
    // Writing a default on INSERT is not reading a stale value.
    let is_field_init = {
        let i = code.find(column).unwrap_or(0);
        let after = code[i + column.len()..].trim_start();
        let before = code[..i].trim_end();
        after.starts_with(':') && !before.ends_with('"')
    };

    if is_result_alias || is_json_key || is_alias_getter || is_field_init {
        return None;
    }

    Some(
        "unqualified reference that is not an alias, a JSON key, or a field \
         initialiser — if this selects the column from `agents`, it reads a \
         permanent zero",
    )
}

// ═══════════════════════════════════════════════════════════════════
// Tier 1 — offline, always runs
// ═══════════════════════════════════════════════════════════════════

/// **The tripwire.** No request-serving handler may read a write-orphaned
/// column as truth.
///
/// This is the check whose absence let six surfaces independently reach for
/// the same dead ledger. Each was written by someone who reasonably assumed
/// a column named `total_executions` contained the number of executions.
#[test]
fn no_handler_reads_a_write_orphaned_column() {
    let root = repo_root();
    let mut violations: Vec<String> = Vec::new();

    for scanned in SCANNED_ROOTS {
        let dir = root.join(scanned);
        if !dir.exists() {
            panic!(
                "SCANNED_ROOTS names `{scanned}`, which does not exist. A \
                 scanned root that has moved silently disables this check — \
                 update src/rollup_trust.rs."
            );
        }
        for file in rust_files_under(&dir) {
            let rel = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();
            let Ok(body) = std::fs::read_to_string(&file) else {
                continue;
            };
            for contract in write_orphaned_columns() {
                if reader_is_exempt(&rel, contract.column) {
                    continue;
                }
                for (n, line) in body.lines().enumerate() {
                    if let Some(reason) = read_reason(line, contract.column) {
                        violations.push(format!(
                            "{rel}:{}\n      {}\n      → {reason}\n      → `{}.{}` is never \
                             written by any code path. Use `{}` instead.\n      Truth lives \
                             in: {}",
                            n + 1,
                            strip_comments(line).trim(),
                            contract.table,
                            contract.column,
                            contract.replacement,
                            contract.source_of_truth,
                        ));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "\n{} read(s) of a write-orphaned column in request-serving code:\n\n  {}\n\n\
         These columns exist, are correctly typed, and are permanently zero. \
         Reading one ships a wrong number to a user, and no existing guard \
         catches it: `schema_trust` checks that the column is PRESENT, not \
         that it is TRUE.\n\n\
         If a read is legitimate (e.g. an explicitly-labelled fallback), add \
         it to `READER_EXEMPTIONS` in src/rollup_trust.rs with a reason.\n",
        violations.len(),
        violations.join("\n\n  "),
    );
}

/// The replacement each orphan names must actually exist in the codebase —
/// otherwise the contract sends the next reader somewhere useless.
#[test]
fn declared_replacements_exist_in_the_codebase() {
    let root = repo_root();
    // Collect the view/column names the replacements refer to, then confirm
    // each appears in a migration (the definition) and in Rust (a consumer).
    let migrations = root.join("migrations");
    let mut migration_text = String::new();
    for f in std::fs::read_dir(&migrations)
        .expect("migrations dir")
        .flatten()
    {
        if f.path().extension().is_some_and(|x| x == "sql") {
            migration_text.push_str(&std::fs::read_to_string(f.path()).unwrap_or_default());
        }
    }

    for contract in write_orphaned_columns() {
        // `agent_execution_rollup.executions` → ("agent_execution_rollup", "executions")
        let (relation, column) = contract
            .replacement
            .split_once('.')
            .unwrap_or((contract.replacement, ""));
        assert!(
            migration_text.contains(relation),
            "{}.{} says to use `{}`, but no migration defines `{relation}`. \
             A replacement nobody can find is worse than no advice.",
            contract.table,
            contract.column,
            contract.replacement,
        );
        if !column.is_empty() {
            assert!(
                migration_text.contains(column),
                "`{relation}` exists but no migration mentions column \
                 `{column}` (named by {}.{}'s replacement)",
                contract.table,
                contract.column,
            );
        }
    }
}

/// The replacement view must be under the schema contract, so it can't
/// vanish without the boot probe noticing. A source of truth with no
/// existence guarantee just relocates the problem.
#[test]
fn replacement_relations_are_in_the_schema_contract() {
    let declared: BTreeSet<&str> = fermi::schema_trust::SCHEMA_VIEWS
        .iter()
        .chain(fermi::schema_trust::SCHEMA_MATVIEWS.iter())
        .chain(fermi::schema_trust::SCHEMA_TABLES.iter())
        .copied()
        .collect();

    for contract in write_orphaned_columns() {
        let relation = contract
            .replacement
            .split_once('.')
            .map(|(r, _)| r)
            .unwrap_or(contract.replacement);
        assert!(
            declared.contains(relation),
            "`{relation}` is the replacement for {}.{} but is not declared in \
             any schema_trust manifest. If it disappears, six surfaces 500 \
             and the boot probe stays silent.",
            contract.table,
            contract.column,
        );
    }
}

/// Every column of the rollup view that Rust reads must be in
/// `SCHEMA_COLUMNS`. `pg_attribute` covers views, so this costs nothing and
/// turns "renamed a column in the view definition" into a boot-time failure
/// instead of a runtime one.
#[test]
fn rollup_view_columns_are_contracted() {
    let contracted: BTreeSet<(&str, &str)> = fermi::schema_trust::SCHEMA_COLUMNS
        .iter()
        .copied()
        .collect();

    let view = fermi::agent_economics::ROLLUP_VIEW;
    for col in [
        "agent_id",
        "executions",
        "successful",
        "failed",
        "cost_usd",
        "tokens_used",
        "avg_execution_time_ms",
        "episodes_missing_cost",
    ] {
        assert!(
            contracted.contains(&(view, col)),
            "`{view}.{col}` is read by src/agent_economics.rs but is not in \
             SCHEMA_COLUMNS. Add it, so a rename is caught at boot."
        );
    }
}

/// A `WriteOrphaned` column must not be silently re-adopted: if someone
/// wires up a writer, they have to change the disposition, which forces
/// them to make the mismatch query pass.
#[test]
fn no_writer_exists_for_orphaned_columns() {
    let root = repo_root();
    let mut writers: Vec<String> = Vec::new();

    // Look across all of `src` plus the memory crate that owns the row
    // mapping — a writer could legitimately live in either.
    for scanned in ["src", "agent-bestiary/memory/src"] {
        let dir = root.join(scanned);
        if !dir.exists() {
            continue;
        }
        for file in rust_files_under(&dir) {
            let rel = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();
            // This contract names the columns by definition.
            if rel.ends_with("src/rollup_trust.rs") {
                continue;
            }
            let Ok(body) = std::fs::read_to_string(&file) else {
                continue;
            };
            let flat = body.to_lowercase();
            for contract in write_orphaned_columns() {
                // The shape of an actual write: `SET <col> =` or
                // `SET <col>=`. Deliberately narrow — we want the signal
                // "someone started maintaining this", not every mention.
                for pat in [
                    format!("set {} =", contract.column),
                    format!("set {}=", contract.column),
                    format!("{} = {} +", contract.column, contract.column),
                ] {
                    if flat.contains(&pat) {
                        writers.push(format!("{rel}: matched `{pat}`"));
                    }
                }
            }
        }
    }

    assert!(
        writers.is_empty(),
        "\nFound what looks like a writer for a column declared \
         WriteOrphaned:\n\n  {}\n\n\
         If a write path now maintains it, that is good news — but change \
         its `disposition` to `Maintained` in src/rollup_trust.rs so the \
         live check starts asserting it agrees with its source of truth. \
         A half-maintained counter (written on one path, not another) is \
         worse than an obviously-dead one, because it looks plausible.\n",
        writers.join("\n  "),
    );
}

/// **Guard the guard.** The detector must flag the real bug shapes and stay
/// silent on the innocent ones.
///
/// Both directions matter. A detector that misses the bug is useless; one
/// that fails on correct code gets deleted by the first person it
/// inconveniences, which is worse than never having it — the deletion looks
/// like cleanup.
///
/// The `should_flag` cases are transcribed from the actual pre-fix source.
/// The `should_pass` cases are transcribed from the actual post-fix source.
#[test]
fn detector_flags_real_reads_and_ignores_innocent_ones() {
    const COL: &str = "total_executions";

    // Transcribed from the pre-fix source. Every one of these shipped a
    // zero to a user.
    let should_flag = [
        // src/handlers/admin.rs — admin agent list
        "                \"execution_count\": a.total_executions,",
        // src/handlers/observatory.rs — fleet view
        "                \"total_executions\": a.total_executions,",
        // src/handlers/ecology.rs — population census
        "                COUNT(*) AS n, SUM(total_executions) AS runs",
        // src/handlers/orchestras.rs — membership inbox
        "                a.status AS agent_status, a.total_executions, \\",
        // src/handlers/profile.rs — aggregate over a user's agents
        "        \"SELECT agent_name, total_executions FROM agents\",",
        // A Rust read through any binding name.
        "            let runs = agent.total_executions;",
    ];
    for line in should_flag {
        assert!(
            read_reason(line, COL).is_some(),
            "detector MISSED a real read — this exact line shipped a wrong \
             number to users:\n  {line}"
        );
    }

    // Transcribed from the post-fix source. The wire key survives; the
    // data source changed. Flagging these would make the check unkeepable.
    let should_pass = [
        // JSON output key, value from the rollup.
        "                \"total_executions\": m.executions,",
        "                \"execution_count\": m.executions,",
        // SQL result alias preserving the wire key.
        "                COALESCE(r.executions, 0) as total_executions",
        "         COALESCE(SUM(COALESCE(r.executions, 0)), 0)::bigint AS total_executions",
        // Reading that alias back out of the row.
        "            \"total_executions\": r.try_get::<i64, _>(\"total_executions\").unwrap_or(0),",
        // Writing a default on INSERT is not reading a stale value.
        "        total_executions: 0,",
        // A field declaration.
        "    pub total_executions: i32,",
        // Prose.
        "    // `agents.total_executions` is never written — see migrations/192",
        "    /// Why not read `agents.total_executions`?",
    ];
    for line in should_pass {
        assert_eq!(
            read_reason(line, COL),
            None,
            "detector FALSE-POSITIVED on correct post-fix code. A check that \
             fails on correct code gets deleted by the first person it \
             inconveniences:\n  {line}"
        );
    }
}

/// If `strip_comments` were too aggressive it would
/// silence the tripwire, and the whole harness would pass vacuously.
#[test]
fn comment_stripping_does_not_hide_real_reads() {
    // Prose mentioning the column is not a read.
    assert_eq!(strip_comments("    // uses total_executions"), "");
    assert_eq!(strip_comments("/// See total_executions"), "");
    assert_eq!(strip_comments("     * total_executions"), "");
    // Code is a read, including inside a SQL string literal.
    assert!(
        strip_comments("  \"SELECT total_executions FROM agents\",").contains("total_executions")
    );
    assert!(strip_comments("  let n = a.total_executions;").contains("total_executions"));
    // A trailing comment doesn't erase the code before it.
    assert!(strip_comments("  a.total_executions, // the dead one").contains("total_executions"));
    // A `//` inside a string literal must not truncate the line.
    assert!(
        strip_comments("  let u = \"https://x/total_executions\";").contains("total_executions")
    );
}

/// The contract must actually be non-empty. A harness that asserts things
/// about an empty list is the most expensive kind of green.
#[test]
fn the_contract_is_not_vacuous() {
    assert!(
        !ROLLUP_CONTRACTS.is_empty(),
        "ROLLUP_CONTRACTS is empty — this harness would pass unconditionally"
    );
    assert!(
        write_orphaned_columns().count() >= 5,
        "expected the five known-dead `agents` counters to be declared; \
         found {}",
        write_orphaned_columns().count()
    );
    assert!(
        !SCANNED_ROOTS.is_empty(),
        "SCANNED_ROOTS is empty — the tripwire would scan nothing"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Tier 2 — live content verification (requires DATABASE_URL)
// ═══════════════════════════════════════════════════════════════════

async fn try_pool() -> Option<sqlx::PgPool> {
    let _ = std::fs::read_to_string(repo_root().join(".env")).map(|contents| {
        for line in contents.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let (k, v) = (k.trim(), v.trim().trim_matches('"'));
                if !k.is_empty() && !k.starts_with('#') && std::env::var(k).is_err() {
                    std::env::set_var(k, v);
                }
            }
        }
    });
    let url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&url)
        .await
        .ok()
}

/// The source of truth must exist, be a view (not a table someone
/// materialised by hand), and return real numbers.
#[tokio::test]
#[ignore]
async fn live_rollup_view_is_a_view_and_has_data() {
    let Some(pool) = try_pool().await else {
        eprintln!("SKIP: DATABASE_URL unset");
        return;
    };
    let view = fermi::agent_economics::ROLLUP_VIEW;

    let kind: String = sqlx::query_scalar(
        "SELECT c.relkind::text FROM pg_catalog.pg_class c \
           JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
          WHERE n.nspname = 'public' AND c.relname = $1",
    )
    .bind(view)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("`{view}` must exist — six surfaces read it: {e}"));

    assert_eq!(
        kind,
        "v",
        "`{view}` must be a plain view (relkind 'v'), found `{kind}` ({}). \
         A matview would need a refresh path, and an unrefreshed matview is \
         the same silent-staleness bug in a different costume.",
        fermi::schema_trust::describe_relkind(&kind),
    );

    let sql = format!(
        "SELECT COUNT(*)::bigint AS agents_with_runs, \
                COALESCE(SUM(executions), 0)::bigint AS runs, \
                COALESCE(SUM(cost_usd), 0)::float8 AS cost \
           FROM {view}"
    );
    let row = sqlx::query(&sql)
        .fetch_one(&pool)
        .await
        .expect("the rollup view must be queryable");
    use sqlx::Row;
    let agents: i64 = row.try_get("agents_with_runs").unwrap_or(0);
    let runs: i64 = row.try_get("runs").unwrap_or(0);
    let cost: f64 = row.try_get("cost").unwrap_or(0.0);

    eprintln!("  {view}: {agents} agents, {runs} runs, ${cost:.2} measured");
    assert!(
        agents > 0 && runs > 0,
        "`{view}` is empty. Either no agent has ever run (then this database \
         cannot validate the contract) or `episodes` is not being written \
         — which would be a far worse bug than the one this replaced."
    );
}

/// Prove the retirement was warranted: a `WriteOrphaned` column must
/// actually disagree with its source of truth.
///
/// If one *agrees* everywhere, either someone quietly wired up a writer (in
/// which case it should be `Maintained`, and asserted) or this database has
/// no data to distinguish them. Both deserve a look, so both fail loudly
/// rather than passing on a coincidence.
#[tokio::test]
#[ignore]
async fn live_orphaned_columns_really_are_stale() {
    let Some(pool) = try_pool().await else {
        eprintln!("SKIP: DATABASE_URL unset");
        return;
    };

    let mut agreeing: Vec<String> = Vec::new();
    for contract in write_orphaned_columns() {
        let mismatches: i64 = sqlx::query_scalar(contract.mismatch_sql)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "mismatch_sql for {}.{} failed to execute: {e}\n  SQL: {}",
                    contract.table, contract.column, contract.mismatch_sql
                )
            });
        eprintln!(
            "  {}.{}: {mismatches} row(s) disagree with {}",
            contract.table, contract.column, contract.source_of_truth
        );
        if mismatches == 0 {
            agreeing.push(format!("{}.{}", contract.table, contract.column));
        }
    }

    assert!(
        agreeing.is_empty(),
        "\nThese columns are declared WriteOrphaned but AGREE with their \
         source of truth on every row: {}\n\n\
         Either a writer was added (change `disposition` to `Maintained` so \
         the agreement is asserted rather than assumed), or this database \
         has too little data to tell them apart (run against one that \
         does). Passing on a coincidence is how the contract rots.\n",
        agreeing.join(", "),
    );
}

/// Any `Maintained` rollup must agree with its source of truth on every
/// row. This is the assertion that catches a *writer* bug, as opposed to an
/// absent writer.
///
/// Currently vacuous — nothing is `Maintained`. It exists so that promoting
/// a column has an obvious, already-wired place to prove itself.
#[tokio::test]
#[ignore]
async fn live_maintained_rollups_agree_with_their_source() {
    let Some(pool) = try_pool().await else {
        eprintln!("SKIP: DATABASE_URL unset");
        return;
    };

    let maintained: Vec<_> = ROLLUP_CONTRACTS
        .iter()
        .filter(|c| c.disposition == Disposition::Maintained)
        .collect();

    if maintained.is_empty() {
        eprintln!(
            "  no Maintained rollups declared — nothing to check. This is \
             the intended steady state: derive it, don't cache it."
        );
        return;
    }

    for contract in maintained {
        let mismatches: i64 = sqlx::query_scalar(contract.mismatch_sql)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "mismatch_sql for {}.{} failed: {e}",
                    contract.table, contract.column
                )
            });
        assert_eq!(
            mismatches, 0,
            "{}.{} is declared Maintained but disagrees with {} on \
             {mismatches} row(s). Its writer is buggy or incomplete — a \
             counter that is right sometimes is read as right always.",
            contract.table, contract.column, contract.source_of_truth,
        );
    }
}
