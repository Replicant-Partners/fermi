//! Tests for the schema trust contract (`src/schema_trust.rs`).
//!
//! ## Why these live in `tests/` rather than inline
//!
//! Two reasons, both deliberate:
//!
//! 1. **The contract was previously untestable.** Until v0.11.9 the module
//!    was `#[path]`-included into the `api-server` binary only, so it was
//!    invisible to `cargo test`. That is *why* a contract which could never
//!    return healthy (see `fermi_leaderboard_is_declared_as_a_matview`)
//!    survived from v0.11.0 to v0.11.8 without anyone noticing.
//!
//! 2. **Integration tests link the library without its inline `#[cfg(test)]`
//!    modules.** That keeps contract verification runnable even while an
//!    unrelated in-flight refactor has broken some other module's unit
//!    tests — schema integrity should not be blocked on that.
//!
//! ## Two tiers
//!
//! * **Contract hygiene** — no database. Asserts the contract is internally
//!   coherent: no relation declared as two kinds, no orphaned column
//!   references, every verdict axis counted. These always run.
//!
//! * **Live verification** — requires `DATABASE_URL` pointing at a migrated
//!   database. Asserts `verify()` actually returns healthy. This is the test
//!   that makes "the contract is satisfiable" a machine-checked property
//!   rather than an operator's belief. Skips (loudly) when unset, so local
//!   `cargo test` stays useful offline.

use std::collections::HashSet;

use fermi::schema_trust::{
    describe_relkind, BootDecision, SchemaVerdict, MATVIEW_KINDS, SCHEMA_COLUMNS, SCHEMA_FUNCTIONS,
    SCHEMA_MATVIEWS, SCHEMA_TABLES, TABLE_KINDS,
};

// ═══════════════════════════════════════════════════════════════════
// Tier 1 — contract hygiene (no database)
// ═══════════════════════════════════════════════════════════════════

/// Regression test for the v0.11.0 → v0.11.8 defect.
///
/// `fermi_leaderboard` is a MATERIALIZED VIEW (`migrations/094:178`,
/// rebuilt by `migrations/167:77`). Listing it in `SCHEMA_TABLES` while
/// probing `information_schema.tables` — which omits materialized views
/// entirely — made `verify()` permanently unhealthy and `SCHEMA_STRICT=1`
/// un-enablable. The drift detector was itself an always-failing guard.
#[test]
fn fermi_leaderboard_is_declared_as_a_matview() {
    assert!(
        SCHEMA_MATVIEWS.contains(&"fermi_leaderboard"),
        "fermi_leaderboard is a MATERIALIZED VIEW and must be declared in SCHEMA_MATVIEWS"
    );
    assert!(
        !SCHEMA_TABLES.contains(&"fermi_leaderboard"),
        "fermi_leaderboard must NOT be in SCHEMA_TABLES — the table probe requires \
         relkind in {:?}, and a matview reports 'm'",
        TABLE_KINDS
    );
}

#[test]
fn no_relation_is_declared_as_both_table_and_matview() {
    // Three categories now, so check every pair. A relation has exactly one
    // relkind; declaring it in two categories makes one declaration
    // permanently unsatisfiable, which is the defect that made the whole
    // contract un-enablable from v0.11.0 to v0.11.8.
    let views = fermi::schema_trust::SCHEMA_VIEWS;
    for (a_label, a, b_label, b) in [
        (
            "SCHEMA_TABLES",
            SCHEMA_TABLES,
            "SCHEMA_MATVIEWS",
            SCHEMA_MATVIEWS,
        ),
        ("SCHEMA_TABLES", SCHEMA_TABLES, "SCHEMA_VIEWS", views),
        ("SCHEMA_MATVIEWS", SCHEMA_MATVIEWS, "SCHEMA_VIEWS", views),
    ] {
        for name in a {
            assert!(
                !b.contains(name),
                "{name} is declared in both {a_label} and {b_label}; a relation \
                 has exactly one relkind, so one of those declarations can \
                 never be satisfied"
            );
        }
    }
}

#[test]
fn relation_contracts_have_no_duplicates() {
    for (label, contract) in [
        ("SCHEMA_TABLES", SCHEMA_TABLES),
        ("SCHEMA_MATVIEWS", SCHEMA_MATVIEWS),
        ("SCHEMA_VIEWS", fermi::schema_trust::SCHEMA_VIEWS),
    ] {
        let mut seen = HashSet::new();
        for name in contract {
            assert!(seen.insert(name), "{} lists {} more than once", label, name);
        }
    }
}

#[test]
fn schema_columns_have_no_duplicates() {
    let mut seen = HashSet::new();
    for pair in SCHEMA_COLUMNS {
        assert!(
            seen.insert(pair),
            "SCHEMA_COLUMNS lists {}.{} more than once",
            pair.0,
            pair.1
        );
    }
}

/// Every relation named in `SCHEMA_COLUMNS` must also be declared as a
/// relation. Otherwise a typo'd table name yields a column check that fails
/// forever with no corresponding "missing table" line to explain why — the
/// same silent-permanent-failure shape as the matview bug.
#[test]
fn every_column_belongs_to_a_declared_relation() {
    // Plain views count too: `pg_attribute` covers them, so a view's
    // columns can be (and are) contracted. `agent_execution_rollup` is the
    // first — see SCHEMA_VIEWS.
    let declared: HashSet<&str> = SCHEMA_TABLES
        .iter()
        .chain(SCHEMA_MATVIEWS.iter())
        .chain(fermi::schema_trust::SCHEMA_VIEWS.iter())
        .copied()
        .collect();

    let mut orphans: Vec<&str> = SCHEMA_COLUMNS
        .iter()
        .map(|(t, _)| *t)
        .filter(|t| !declared.contains(t))
        .collect();
    orphans.sort_unstable();
    orphans.dedup();

    assert!(
        orphans.is_empty(),
        "SCHEMA_COLUMNS references relations absent from \
         SCHEMA_TABLES/SCHEMA_MATVIEWS/SCHEMA_VIEWS: {:?}",
        orphans
    );
}

#[test]
fn function_contract_is_well_formed() {
    let mut seen = HashSet::new();
    for (name, sig, ret) in SCHEMA_FUNCTIONS {
        assert!(!name.is_empty(), "function contract has an empty name");
        assert!(
            !ret.is_empty(),
            "{} declares no return type; format_type() never yields empty, so that entry \
             could never match",
            name
        );
        assert!(
            seen.insert((name, sig)),
            "SCHEMA_FUNCTIONS lists {}({}) more than once",
            name,
            sig
        );
    }
}

/// Canary: if a new axis is added to `SchemaVerdict` and the author forgets
/// to wire it into `is_healthy`/`total_issues`, drift on that axis becomes
/// invisible. Bump the expected count deliberately when adding an axis.
#[test]
fn every_verdict_axis_counts_toward_unhealthy() {
    let verdict = SchemaVerdict {
        missing_tables: vec!["t"],
        missing_matviews: vec!["mv"],
        // migrations/192 added `agent_execution_rollup`, the first plain
        // view in the contract. This canary is what made the new axis
        // impossible to forget: it failed to compile until `missing_views`
        // was wired into is_healthy()/total_issues().
        missing_views: vec!["v"],
        relation_kind_mismatches: vec![("r", "table", "view".into())],
        missing_columns: vec![("t", "c")],
        missing_functions: vec![("f", "", "void")],
        function_sig_mismatches: vec![("f", "", "text".into())],
        function_return_mismatches: vec![("f", "void", "real".into())],
    };

    assert_eq!(
        verdict.total_issues(),
        8,
        "SchemaVerdict gained an axis that total_issues() does not count"
    );
    assert!(!verdict.is_healthy());

    let clean = SchemaVerdict::default();
    assert!(clean.is_healthy());
    assert_eq!(clean.total_issues(), 0);
}

#[test]
fn healthy_verdict_never_aborts_boot() {
    let clean = SchemaVerdict::default();
    assert_eq!(
        fermi::schema_trust::emit_boot_report(&clean, true),
        BootDecision::Healthy,
        "a healthy contract must boot even under SCHEMA_STRICT=1"
    );
    assert_eq!(
        fermi::schema_trust::emit_boot_report(&clean, false),
        BootDecision::Healthy
    );
}

#[test]
fn drift_aborts_only_under_strict() {
    let drifted = SchemaVerdict {
        missing_tables: vec!["nope"],
        ..Default::default()
    };
    assert_eq!(
        fermi::schema_trust::emit_boot_report(&drifted, true),
        BootDecision::DriftAbortBoot
    );
    assert_eq!(
        fermi::schema_trust::emit_boot_report(&drifted, false),
        BootDecision::DriftContinueBoot
    );
}

#[test]
fn health_json_reports_matviews_and_distinguishes_kind_drift_from_absence() {
    let verdict = SchemaVerdict {
        relation_kind_mismatches: vec![(
            "fermi_leaderboard",
            "materialized view",
            "ordinary table".into(),
        )],
        ..Default::default()
    };
    let j = verdict.to_health_json();

    assert_eq!(j["status"], "degraded");
    assert_eq!(j["summary"]["relation_kind_drift"], 1);
    assert_eq!(j["summary"]["matviews"]["total"], SCHEMA_MATVIEWS.len());

    let mv = j["matviews"]
        .as_array()
        .expect("health json must carry a matviews array")
        .iter()
        .find(|m| m["name"] == "fermi_leaderboard")
        .expect("fermi_leaderboard must appear in the matviews array");

    // Present-but-wrong-kind must not read as absent: it sends the operator
    // looking for a missing migration instead of a changed object.
    assert_eq!(mv["present"], true);
    assert_eq!(mv["kind_drift"], "ordinary table");
}

#[test]
fn contracted_relkinds_are_all_described() {
    for k in TABLE_KINDS
        .iter()
        .chain(MATVIEW_KINDS.iter())
        .chain(fermi::schema_trust::VIEW_KINDS.iter())
    {
        assert_ne!(
            describe_relkind(k),
            "unknown relkind",
            "relkind {:?} is contracted but has no human-readable description",
            k
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tier 2 — live verification (requires DATABASE_URL)
// ═══════════════════════════════════════════════════════════════════

/// The Phase-0 exit criterion, as a test.
///
/// Against a fully-migrated database, `verify()` must return **healthy**.
/// If this passes, `SCHEMA_STRICT=1` is safe to enable; if it fails, the
/// listed items are either genuine drift or an over-declared contract, and
/// either way strict mode would refuse to boot.
///
/// Skips when `DATABASE_URL` is unset so offline `cargo test` still works.
#[tokio::test]
async fn live_contract_is_satisfied_by_a_migrated_database() {
    let url = match std::env::var("DATABASE_URL") {
        Ok(u) if !u.trim().is_empty() => u,
        _ => {
            eprintln!(
                "SKIP live_contract_is_satisfied_by_a_migrated_database: DATABASE_URL unset. \
                 This is the test that proves the contract is satisfiable — run it in CI."
            );
            return;
        }
    };

    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("SKIP live contract check: could not connect to DATABASE_URL: {e}");
            return;
        }
    };

    let verdict = fermi::schema_trust::verify(&pool)
        .await
        .expect("the contract probe itself must succeed against a reachable database");

    if !verdict.is_healthy() {
        // Reuse the boot reporter so the failure output is identical to what
        // an operator sees in deploy logs.
        fermi::schema_trust::emit_boot_report(&verdict, false);
    }

    assert!(
        verdict.is_healthy(),
        "schema trust contract is not satisfied by this database ({} issue(s)). \
         Either the DB is drifted or the contract over-declares. Full detail above; \
         machine-readable form: {}",
        verdict.total_issues(),
        verdict.to_health_json()
    );
}

/// Guards the specific mechanism the v0.11.9 fix depends on: that a
/// materialized view is visible to the probe at all.
///
/// If someone "simplifies" `verify()` back to `information_schema`, the
/// hygiene tests above still pass (the contract stays coherent) but this
/// fails — because `information_schema` cannot see matviews.
#[tokio::test]
async fn live_matviews_are_visible_to_the_probe() {
    let url = match std::env::var("DATABASE_URL") {
        Ok(u) if !u.trim().is_empty() => u,
        _ => {
            eprintln!("SKIP live_matviews_are_visible_to_the_probe: DATABASE_URL unset.");
            return;
        }
    };

    if SCHEMA_MATVIEWS.is_empty() {
        return;
    }

    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("SKIP live matview visibility check: {e}");
            return;
        }
    };

    let verdict = fermi::schema_trust::verify(&pool)
        .await
        .expect("probe must succeed");

    for name in SCHEMA_MATVIEWS {
        assert!(
            !verdict.missing_matviews.contains(name),
            "matview {} reported missing. If it exists in the database, the probe is \
             using information_schema (which omits materialized views) rather than \
             pg_catalog — that is the v0.11.0 bug regressing.",
            name
        );
    }
}
