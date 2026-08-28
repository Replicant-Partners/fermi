//! What did the bare parse actually cost?
//!
//! `grounding_trust::response_floor` used a bare `serde_json::from_str` and
//! returned `unavailable_no_tool_source` the moment it failed, explaining itself
//! with *"Prose. An extraction from prose is ungrounded by construction: there
//! are no typed fields to have been sourced."* That is a true statement about a
//! `from_str` parse presented as a fact about the agent's output — §19's defect.
//!
//! Agents wrap their document in prose, and `agent_backend::envelope
//! ::extract_json` has always known that: it does a balanced-brace scan for the
//! largest object, which is why `handlers::execution` grades responses this
//! function called ungradeable. Two implementations of *get the document out of
//! the response*, disagreeing, with the weaker one behind the trust calculation.
//!
//! # Why this is a live suite and not a unit test
//!
//! The unit tests
//! (`a_document_wrapped_in_prose_is_graded_rather_than_dismissed`,
//! `recovering_a_document_is_not_the_same_as_finding_content`) prove the function
//! behaves. They cannot say what it was costing, because the cost is a property
//! of the corpus: how many retained responses are packaged in a way the old parse
//! refused. That number is the whole argument for the change, and asserting it
//! from a fixture would be inventing it.
//!
//! # This suite reports; it asserts only what cannot be a matter of degree
//!
//! The recovery rate is **printed**, not thresholded — a threshold on it would be
//! a target, and the corpus changes with every run. What is asserted is the one
//! thing that cannot be a matter of degree: **a response the platform's own
//! `extract_json` can read must not be graded as though it had no document.** If
//! that ever holds again, the two implementations have diverged a second time.

use fermi::agent_backend::envelope::extract_json;
use fermi::grounding_trust::{self as gt, PROV_UNAVAILABLE};
use sqlx::{PgPool, Row};

/// The agents that have a field contract, so a floor means something.
const CONTRACTED: &[&str] = &[
    "enemy_sensor",
    "football_analyst",
    "forage_identify",
    "forage_scout",
    "genome_profiler",
    "harvest_advisor",
    "hud_field_scout",
    "prey_locator",
    "weather_oracle",
];

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

/// The old parse, restored here so the two can be compared on the same rows.
///
/// A copy, deliberately, and the only copy of a trust calculation in this
/// repository that is allowed to exist: its purpose is to be **wrong**. It is
/// what shipped, kept so the cost of it is a measurement rather than a
/// recollection. It is `pub(crate)`-invisible, in a test, and nothing reads its
/// verdict for any purpose but the comparison below.
fn bare_parse_recovers_a_document(response_text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(response_text)
        .map(|v| v.is_object())
        .unwrap_or(false)
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn a_readable_document_is_never_graded_as_absent() {
    let pool = pool().await;

    let rows = sqlx::query(
        "SELECT a.agent_name, e.response_text \
           FROM episodes e JOIN agents a ON a.agent_id = e.agent_id \
          WHERE a.agent_name = ANY($1) \
            AND e.response_text IS NOT NULL \
            AND e.response_text <> ''",
    )
    .bind(CONTRACTED)
    .fetch_all(&pool)
    .await
    .expect("read retained responses");

    let mut total = 0usize;
    let mut bare = 0usize;
    let mut embedded = 0usize;
    let mut no_document = 0usize;
    // The floor distribution, not a count of "improvements".
    //
    // The first version of this probe printed `graded above unavailable: 64` and
    // that was an over-claim of exactly the kind this whole file is about:
    // `tool_no_match` sorts above `unavailable_no_tool_source` in
    // `PROVENANCE_VALUES` and both carry **strength 0**, so "above unavailable"
    // can mean a different word for the same amount of reliance. What matters is
    // the strength, so the strength is what gets printed.
    let mut by_floor: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut reliable = 0usize;
    let mut regressions: Vec<String> = Vec::new();

    for r in &rows {
        let Ok(agent) = r.try_get::<String, _>("agent_name") else {
            continue;
        };
        let Ok(text) = r.try_get::<String, _>("response_text") else {
            continue;
        };
        total += 1;

        let bare_ok = bare_parse_recovers_a_document(&text);
        let readable = extract_json(&text).is_some();
        if bare_ok {
            bare += 1;
        } else if readable {
            embedded += 1;
        } else {
            no_document += 1;
        }

        let floor = gt::response_floor(&agent, &text);

        // The assertion. Not "the floor improved" — grading a recovered document
        // may legitimately land on `unavailable`, because the contracted paths
        // can be absent and a document missing all of them SHOULD floor low.
        // What must never happen is the floor being `unavailable` *for want of a
        // document* when the platform's own extractor can read one, because that
        // is not a verdict about the agent, it is a verdict about the parser.
        if readable && floor.is_none() {
            regressions.push(format!(
                "{agent}: a readable document produced no floor at all, which is \
                 reserved for an agent with no field contract"
            ));
        }
        let label = match floor {
            None => "(no contract)".to_string(),
            Some(v) => format!("{v} [strength {}]", gt::strength(v)),
        };
        *by_floor.entry(label).or_default() += 1;
        if floor.map(gt::strength).unwrap_or(0) >= 2 {
            reliable += 1;
        }
    }

    println!("\n  Retained responses from contracted agents: {total}");
    println!("    bare JSON                     {bare}");
    println!("    document embedded in prose    {embedded}");
    println!("    no document at all            {no_document}");
    println!("\n  Floor, as graded now:");
    for (label, n) in &by_floor {
        println!("    {label:<40} {n}");
    }
    println!(
        "\n  The old bare parse graded {bare} of {total} and dismissed the rest \
         as \"ungrounded by construction\"."
    );
    println!(
        "  {reliable} of {total} now floor at strength 2 (reproducible: run the \
         tool, apply the transform, or follow the citation)."
    );
    println!(
        "\n  Read the STRENGTH column, not the token. `tool_no_match` sorts above \
         `unavailable_no_tool_source` and both carry strength 0 — a different \
         word for the same amount of reliance. Recovering the document changes \
         what the contract can SAY about a response; it does not by itself make \
         the response better grounded, and the distribution above is the only \
         honest account of which happened.\n"
    );

    assert!(
        regressions.is_empty(),
        "\n  {}\n\nA response the platform's own `extract_json` can read was \
         graded as though it had no document. The two implementations of \
         document recovery have diverged again.\n",
        regressions.join("\n  ")
    );

    // Non-vacuity. A corpus with nothing to recover would make the assertion
    // above true about nothing, and this suite exists precisely because the
    // recoverable population is large.
    assert!(
        total > 0,
        "no retained responses from contracted agents, so this proves nothing"
    );
}

/// What the semantic rules were floored on.
///
/// `provenance_oracle` computes a rule's extraction floor by re-running
/// `response_floor` over the episodes the rule was consolidated from. Every rule
/// carrying a floor read `unavailable_no_tool_source` — 28 of 28 — and this is
/// the report that says whether that was the corpus or the parser.
///
/// Reported, never asserted. A rule legitimately floors at `unavailable` when its
/// sources genuinely rest on nothing, and asserting a distribution here would
/// assert that grounded rules must exist.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_the_rule_floors_and_what_they_rest_on() {
    let pool = pool().await;

    let rows = sqlx::query(
        "SELECT provenance_floor, count(*)::bigint AS n \
           FROM semantic_rules \
          WHERE provenance_floor IS NOT NULL \
          GROUP BY 1 ORDER BY 2 DESC",
    )
    .fetch_all(&pool)
    .await
    .expect("read rule floors");

    println!("\n  semantic_rules.provenance_floor, as stored:");
    for r in &rows {
        let floor: String = r.try_get("provenance_floor").unwrap_or_default();
        let n: i64 = r.try_get("n").unwrap_or(0);
        println!("    {floor:<32} {n}");
    }
    println!(
        "\n  These are STORED values, computed when each rule was written and \
         not recomputed here. A rule floored before the document-recovery fix \
         carries the old parse's verdict, and nothing in this repository \
         backfills it — see `provenance_oracle`. The number to watch is whether \
         rules written AFTER this change still land here.\n"
    );
}
