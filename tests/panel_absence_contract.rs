//! Why is each panel empty, against the live platform?
//!
//! ```text
//! cargo test --test panel_absence_contract -- --ignored --nocapture
//! ```
//!
//! Read-only. Every query it reaches belongs to `liveness_trust` or
//! `loop_model`, both of which assert read-only-ness at the unit level.
//!
//! # What is asserted and what is only reported
//!
//! An empty panel is **not** a test failure. Most of them are empty because
//! nothing has happened, and asserting otherwise would assert that anomalies
//! must occur and that owners must change their teams — the same error as
//! asserting on a row count.
//!
//! Two things *are* asserted, and neither is about whether a panel has data:
//!
//! * **Every panel resolves to a stamped answer.** The point of
//!   `panel_absence` is that no surface authors its own blank, so a panel that
//!   the resolver cannot answer at all is a defect in the resolver.
//! * **No `Unknown` arrives unexplained.** `Unknown` is legitimate — it is the
//!   honest verdict when no contract can speak — but it must carry a reason,
//!   because an unexplained `Unknown` is a blank with extra steps.
//!
//! The reading distribution itself is reported, not asserted. It is expected to
//! be mostly `Unknown` at first; each one is a work item, and the ratchet in
//! `panel_absence::the_unresolved_list_may_only_shrink` is what makes the
//! backlog shrink rather than the report get ignored.

use fermi::native_evaluators::Observation;
use fermi::panel_absence::{
    resolve_all, resolve_for_subject, scoped_probe, Reading, Scope, PANELS, SCOPED_PROBES,
};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

/// Gather a real snapshot: a liveness sweep, the loop walk, and the counters.
///
/// The counters are per-process and start at zero here, so gate and
/// write-accounting readings are weaker in this suite than in a long-lived
/// server. That is a property of the substrate rather than of the test, and
/// `DESIGN_UX_PANEL_ARCHITECTURE.md` §2.9 records what it costs.
async fn snapshot(pool: &PgPool) -> Observation {
    let report = fermi::liveness_trust::sweep(pool).await;
    Observation {
        writes: fermi::write_accounting::accounts(),
        gates: fermi::gate_trust::accounts(),
        loops: fermi::loop_model::evaluate(pool).await,
        liveness: Some(report),
        gate_ledger: Some(fermi::gate_trust::ledger_status()),
        // The real census. `None` here would make every declaration-resolved
        // panel report `no_census`, and the suite would pass while proving
        // nothing about the one resolver that reads the fleet.
        declarations: fermi::native_evaluators::declaration_census(pool).await,
        // The real scan, for the same reason as the census above: `None` would
        // make the conformance evaluator report inconclusive and the suite would
        // pass while proving nothing about whether a declared contract is wired.
        conformance: fermi::native_evaluators::contract_conformance(pool).await,
    }
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_why_every_panel_is_empty() {
    let pool = pool().await;
    let o = snapshot(&pool).await;
    let absences = resolve_all(&o);

    println!("\n  Panel absences, live\n");
    for a in &absences {
        let glyph = match a.reading {
            Reading::Idle => "·",
            Reading::Fault => "✕",
            Reading::Unknown => "⊘",
        };
        println!(
            "  {glyph} {:<28} {:<10} {:<16} {}",
            a.panel,
            a.reading.label(),
            a.token,
            a.answered_by
        );
        println!("      {}", a.detail);
        if let Some(r) = a.remediation {
            println!("      → {r}");
        }
        println!();
    }

    let faults = absences
        .iter()
        .filter(|a| a.reading == Reading::Fault)
        .count();
    let unknown = absences
        .iter()
        .filter(|a| a.reading == Reading::Unknown)
        .count();
    println!(
        "  {} panel(s): {} idle, {faults} fault, {unknown} unexplained.\n",
        absences.len(),
        absences.len() - faults - unknown
    );
}

/// Every declared panel must get an answer.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_panel_resolves_to_a_stamped_answer() {
    let pool = pool().await;
    let o = snapshot(&pool).await;
    let absences = resolve_all(&o);

    assert_eq!(
        absences.len(),
        PANELS.len(),
        "the resolver dropped a panel on the floor"
    );
    for a in &absences {
        assert!(
            !a.detail.trim().is_empty(),
            "{}: resolved to an empty sentence, which is the blank this module \
             exists to replace",
            a.panel
        );
        assert!(
            a.answered_by == "none" || !a.token.is_empty(),
            "{}: answered by {} with no token",
            a.panel,
            a.answered_by
        );
    }
}

/// Every scoped probe must actually run against the live schema.
///
/// The failure this catches is the one static tests cannot: a probe naming a
/// column that does not exist resolves to `probe_failed` for ever, which is an
/// honest reading and a permanently useless one. A probe that has never been
/// executed is a probe that does not work.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_scoped_probe_runs() {
    let pool = pool().await;
    let zero = uuid::Uuid::nil();
    let mut broken = Vec::new();

    for (id, probe) in SCOPED_PROBES {
        for (label, sql) in [
            ("writes", probe.writes_sql),
            ("opportunities", probe.opportunities_sql),
        ] {
            // The nil UUID matches nothing, so this asserts the query is
            // *runnable* — columns, types and joins — without depending on data.
            if let Err(e) = sqlx::query_scalar::<_, i64>(sql)
                .bind(zero)
                .fetch_one(&pool)
                .await
            {
                broken.push(format!("{id}.{label}: {e}"));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "{} scoped probe quer(ies) do not run:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// Report the scoped readings for the busiest real agent.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_scoped_absences_for_a_real_agent() {
    let pool = pool().await;
    let o = snapshot(&pool).await;

    let Ok(Some((agent_id, name, episodes))) = sqlx::query_as::<_, (uuid::Uuid, String, i64)>(
        "SELECT a.agent_id, a.agent_name, count(e.episode_id)::bigint
           FROM agents a JOIN episodes e ON e.agent_id = a.agent_id
          GROUP BY a.agent_id, a.agent_name
          ORDER BY 3 DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    else {
        println!("\n  No agent has episodes; nothing to scope against.\n");
        return;
    };

    println!("\n  Scoped absences for `{name}` ({episodes} episodes)\n");
    for p in PANELS {
        if p.scope != Scope::Agent || scoped_probe(p.id).is_none() {
            continue;
        }
        let a = resolve_for_subject(&pool, p, agent_id, &o).await;
        let glyph = match a.reading {
            Reading::Idle => "·",
            Reading::Fault => "✕",
            Reading::Unknown => "⊘",
        };
        println!("  {glyph} {:<28} {:<8} {}", a.panel, a.token, a.detail);
    }
    println!();
}

/// What the three densities actually say, against live data.
///
/// Reported rather than asserted: the budgets are unit-tested, and what a human
/// needs from this is to read the glance column and judge whether two words on
/// a waveguide are worth having.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_every_density() {
    use fermi::panel_contract::{stamp_absence, Density};

    let pool = pool().await;
    let o = snapshot(&pool).await;

    println!("\n  GLANCE — one line, ≤60 chars, the waveguide budget\n");
    for p in PANELS {
        let a = resolve_all(&o)
            .into_iter()
            .find(|x| x.panel == p.id)
            .expect("resolved");
        let s = stamp_absence(p, &a, Density::Glance);
        println!("  |{:<60}|", s.lines[0]);
    }

    // One panel at all three, so the ladder is visible as a ladder.
    let p = PANELS
        .iter()
        .find(|p| p.id == "observatory.anomalies")
        .expect("declared");
    let a = resolve_all(&o)
        .into_iter()
        .find(|x| x.panel == p.id)
        .expect("resolved");

    for d in [Density::Glance, Density::Scan, Density::Study] {
        let s = stamp_absence(p, &a, d);
        println!("\n  {} — {} ({})", p.id, d.as_str(), s.marker_word);
        for l in &s.lines {
            println!("  |{l:<60}|");
        }
    }
    println!();
}

/// An `Unknown` must say why it is unknown.
///
/// This is the failure mode the module could most easily develop: a resolver
/// that shrugs in a structured way is still a shrug, and it would read as
/// progress on every dashboard.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn no_unknown_arrives_without_a_reason() {
    let pool = pool().await;
    let o = snapshot(&pool).await;

    for a in resolve_all(&o) {
        if a.reading != Reading::Unknown {
            continue;
        }
        assert!(
            a.detail.len() > 60,
            "{} is unexplained and says only: {:?}. Either name the contract \
             that should answer it, or say what would make it answerable.",
            a.panel,
            a.detail
        );
    }
}
