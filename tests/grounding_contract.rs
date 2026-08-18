//! Grounding contract — the empirical tier.
//!
//! # What the offline tier cannot do
//!
//! `cargo test --lib -p fermi grounding_trust` checks the contract's *shape*:
//! every field classified, every justification written, every `Sourced` field
//! either cross-checkable or explicitly admitting it is not. All of that
//! passes without ever looking at something an agent produced.
//!
//! That is exactly how `Antaxius beieri` — a bush-cricket (Orthoptera /
//! Tettigoniidae) — was profiled as a cerambycid beetle (Coleoptera /
//! Cerambycidae) with every check green. The field was present, non-null,
//! correctly typed, and declared `Sourced`. `Sourced` asserts *a tool could
//! supply this*; nothing compared the value to anything. The GBIF-verified
//! answer sat on the creature row one `JOIN` away for the entire time.
//!
//! # What this tier does
//!
//! Runs every `cross_check_sql` declared in
//! [`fermi::grounding_trust::FIELD_CONTRACTS`] against a real database. Each
//! returns one row with one `bigint` column `mismatches`: the number of
//! places where an agent's output disagrees with an independently-held copy
//! of the same fact. Non-zero means fabrication, in production, now.
//!
//! Directly modelled on `tests/rollup_contract.rs`, which exists because
//! `agents.total_executions` was present, correctly typed, declared in the
//! schema contract, and permanently zero. Same disease, one layer up: a
//! content failure invisible to every check that reasons about shape.
//!
//! # Running it
//!
//! ```sh
//! scripts/grounding_contract_live.sh          # both tiers
//! cargo test --test grounding_contract -- --ignored --nocapture
//! ```
//!
//! `#[ignore]` because it needs `DATABASE_URL`. Read-only: every query is a
//! bare SELECT and the offline tier asserts that at the unit level.

use fermi::grounding_trust::{cross_check_exempt, cross_checks, FIELD_CONTRACTS};
use sqlx::Row;

/// Offline: the completeness claim, restated at the integration level so it
/// fails in CI even if someone runs only the integration suite.
#[test]
fn no_sourced_field_is_silently_unverified() {
    let gaps: Vec<String> = FIELD_CONTRACTS
        .iter()
        .filter(|c| {
            matches!(
                c.grounding,
                fermi::grounding_trust::Grounding::Sourced { .. }
            )
        })
        .filter(|c| c.cross_check_sql.is_none() && !cross_check_exempt(c.agent_id, c.path))
        .map(|c| format!("{}.{}", c.agent_id, c.path))
        .collect();
    assert!(
        gaps.is_empty(),
        "unverified and unexplained: {}",
        gaps.join(", ")
    );
}

#[test]
fn the_empirical_tier_is_not_entirely_exemptions() {
    assert!(
        cross_checks().count() >= 2,
        "fewer than two real cross-checks — the completeness claim would be \
         satisfied by exemptions alone, which is the shape of a check that \
         cannot fail"
    );
}

/// The migration 203 CHECK and `PROVENANCE_VALUES` must name the same set.
///
/// This module has already had one vocabulary drift: cards said
/// `gbif_verified` where the runtime emitted `tool_verified`, and nothing
/// noticed until a guard was written for it. A CHECK constraint is a worse
/// place for the same bug, because the failure surfaces as a rejected INSERT
/// inside a dream cycle — in a background worker, at whatever hour the cycle
/// runs, on a path whose errors are logged rather than raised. The rule that
/// would have been written is simply lost.
///
/// Parses the SQL rather than restating the list, so the test fails when the
/// two disagree instead of when someone forgets to update a third copy.
#[test]
fn the_migration_check_matches_the_runtime_vocabulary() {
    // 205, not 203: it redefines the CHECK to include the pending tier, and
    // the LATEST definition is the one the database ends up holding. Pointing
    // this test at the older migration would have it pass against a vocabulary
    // that no longer applies — a check measuring a superseded declaration.
    let sql = std::fs::read_to_string("migrations/205_assertion_layer.sql")
        .expect("migration 205 must exist; it is registered in run_migrations");

    // The CHECK body, between `provenance_floor IN (` and its closing paren.
    let start = sql
        .find("provenance_floor IN (")
        .expect("migration 205 must constrain provenance_floor to a closed vocabulary");
    let body = &sql[start..];
    let end = body.find(')').expect("unterminated IN list");
    let declared: std::collections::BTreeSet<String> = body[..end]
        .split('\'')
        .skip(1)
        .step_by(2)
        .map(|s| s.to_string())
        .collect();

    let runtime: std::collections::BTreeSet<String> = fermi::grounding_trust::PROVENANCE_VALUES
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert_eq!(
        declared,
        runtime,
        "migration 203's CHECK and PROVENANCE_VALUES disagree.\n  \
         only in SQL:     {:?}\n  only in Rust:    {:?}\n\
         A value the runtime emits but the constraint rejects loses the rule; \
         a value the constraint permits but the runtime cannot emit is dead \
         vocabulary that will be read as meaningful by whoever finds it next.",
        declared.difference(&runtime).collect::<Vec<_>>(),
        runtime.difference(&declared).collect::<Vec<_>>(),
    );
}

/// A floor column that consumers read as clean when it is NULL is worse than
/// no column, so the migration has to say so where a DBA will see it.
#[test]
fn the_migration_documents_null_as_unknown_rather_than_clean() {
    let sql = std::fs::read_to_string("migrations/203_semantic_rule_provenance_floor.sql").unwrap();
    let comment_start = sql
        .find("COMMENT ON COLUMN public.semantic_rules.provenance_floor IS")
        .expect(
            "the column must carry a COMMENT: `\\d semantic_rules` is where \
                 the next person will look, and a bare TEXT column named \
                 `provenance_floor` invites exactly the wrong default reading",
        );
    let comment = &sql[comment_start..comment_start + 900];
    assert!(
        comment.contains("NULL means UNKNOWN") && comment.contains("not a pass"),
        "the COMMENT must state that NULL is unknown and not a pass"
    );
}

// ─── live tier ─────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "needs DATABASE_URL; run via scripts/grounding_contract_live.sh"]
async fn agent_output_agrees_with_independently_held_truth() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");

    let mut failures: Vec<String> = Vec::new();
    let mut ran = 0usize;

    for (agent, path, sql) in cross_checks() {
        ran += 1;
        let mismatches: i64 = match sqlx::query_scalar(sql).fetch_one(&pool).await {
            Ok(n) => n,
            Err(e) => {
                // A query that cannot run is not a pass. It is the same
                // failure as a guard that always returns true.
                failures.push(format!(
                    "{agent}.{path}: cross-check could not run ({e}). An \
                     unrunnable check reports healthy forever."
                ));
                continue;
            }
        };
        if mismatches > 0 {
            failures.push(format!(
                "{agent}.{path}: {mismatches} row(s) disagree with the \
                 independently-held source of truth. The field is declared \
                 Sourced, so every value should have come from its tool."
            ));
        } else {
            println!("  ok   {agent}.{path}");
        }
    }

    assert!(ran > 0, "no cross-checks declared — this tier is inert");
    assert!(
        failures.is_empty(),
        "\n{} cross-check(s) failed:\n  {}\n",
        failures.len(),
        failures.join("\n  ")
    );
}

/// The tier is only trustworthy if it can go red. Proves the taxonomy
/// cross-check detects a contradiction rather than merely returning zero.
#[tokio::test]
#[ignore = "needs DATABASE_URL; run via scripts/grounding_contract_live.sh"]
async fn the_taxonomy_cross_check_can_actually_fail() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");

    // Same predicate as the declared cross-check, inverted: count rows where
    // profile and creature taxonomy AGREE. If the corpus has agreeing rows,
    // the comparison is live — a query returning zero for both agreement and
    // disagreement would mean the JOIN never matches and the check is inert.
    let agreeing: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint \
           FROM creature_conditions cc \
           JOIN creatures c ON c.creature_id = cc.creature_id \
          WHERE cc.genome_profile IS NOT NULL \
            AND cc.genome_profile->'taxonomy'->>'order' IS NOT NULL \
            AND lower(c.taxonomy->>'order') \
                  = lower(cc.genome_profile->'taxonomy'->>'order')",
    )
    .fetch_one(&pool)
    .await
    .expect("agreement probe");

    assert!(
        agreeing > 0,
        "no profile agrees with its creature's taxonomy either — the JOIN \
         matches nothing, so a zero mismatch count means the check is inert \
         rather than clean. This is the `fermi_leaderboard` matview failure \
         mode: a probe that cannot distinguish healthy from unrunnable."
    );
    println!("  comparison is live: {agreeing} profile(s) agree");
}

// ─── live tier: the provenance floor ───────────────────────────────────
//
// Runs the real oracle against the real corpus. Read-only: it computes floors
// and prints them, and writes nothing. The point is to establish that the
// floor is COMPUTABLE on production data, not merely defined — a floor that
// resolves to unknown for every rule on the platform would be a column nobody
// can act on, indistinguishable from not having built it.

#[tokio::test]
#[ignore = "needs DATABASE_URL; run via scripts/grounding_contract_live.sh"]
async fn the_provenance_floor_resolves_against_the_real_corpus() {
    use agent_bestiary_memory::provenance::ProvenanceOracle;
    use fermi::provenance_oracle::DbProvenanceOracle;

    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");
    let oracle = DbProvenanceOracle::new(pool.clone());

    // Every active rule, with the agent it is FOR and the episodes it cites.
    let rows: Vec<(uuid::Uuid, Option<String>, Vec<uuid::Uuid>)> = sqlx::query_as(
        "SELECT r.rule_id, a.agent_name, coalesce(r.source_episode_cluster, '{}') \
           FROM semantic_rules r \
           LEFT JOIN agents a ON a.agent_id = r.agent_id \
          WHERE r.is_active",
    )
    .fetch_all(&pool)
    .await
    .expect("read rules");

    assert!(
        !rows.is_empty(),
        "no active rules — this tier cannot say anything, which is not the \
         same as saying everything is fine"
    );

    let mut by_floor: std::collections::BTreeMap<String, usize> = Default::default();
    let mut graded_subjects: std::collections::BTreeSet<String> = Default::default();

    for (rule_id, subject, episodes) in &rows {
        let f = oracle
            .extraction_floor(episodes)
            .await
            .unwrap_or_else(|e| panic!("oracle failed on {rule_id}: {e}"));

        // The invariant that cannot be checked offline: nothing in the real
        // corpus may come back stronger than the extraction ceiling. If a
        // production episode ever produced `tool_verified` here, the ceiling
        // is not being applied on the path that matters.
        assert_ne!(
            f.floor.as_deref(),
            Some(fermi::grounding_trust::PROV_TOOL),
            "rule {rule_id} claims tool_verified. Extraction is judgement; a \
             rule reporting its sources' provenance as its own is the exact \
             laundering this column exists to stop.\nbasis: {}",
            f.basis
        );

        let key = f.floor.clone().unwrap_or_else(|| "UNKNOWN".to_string());
        if f.floor.is_some() {
            if let Some(s) = subject {
                graded_subjects.insert(s.clone());
            }
        }
        *by_floor.entry(key).or_default() += 1;
    }

    println!("\n  provenance floor over {} active rules:", rows.len());
    for (floor, n) in &by_floor {
        println!("    {n:>4}  {floor}");
    }

    // Liveness. At least one real rule must resolve to a definite floor,
    // otherwise the oracle is answering "unknown" to everything and this test
    // would keep passing after the grading logic was deleted.
    let graded: usize = by_floor
        .iter()
        .filter(|(k, _)| k.as_str() != "UNKNOWN")
        .map(|(_, n)| *n)
        .sum();
    assert!(
        graded > 0,
        "every rule in the corpus resolved to UNKNOWN. The floor is then \
         inert: it distinguishes nothing, and this assertion would survive \
         the grading logic being removed entirely. Expected the \
         `genome_profiler` rules to grade, since their source episodes retain \
         response text (migration 199) and the agent has a field contract."
    );
    println!(
        "  {graded} of {} rules graded; subjects graded: {:?}",
        rows.len(),
        graded_subjects
    );
}

/// A rule that cites episodes which no longer exist must not read as clean.
///
/// The corpus contains one: a rule whose `source_episode_cluster` names three
/// episodes with no rows behind them. The tempting implementation skips
/// unresolvable ids, which would leave the accumulator empty and — depending
/// on how the empty case is written — return either the strongest value or
/// nothing at all. Both are wrong: a citation that cannot be followed is
/// evidence that cannot be inspected.
#[tokio::test]
#[ignore = "needs DATABASE_URL; run via scripts/grounding_contract_live.sh"]
async fn a_dangling_citation_is_unknown_rather_than_clean() {
    use agent_bestiary_memory::provenance::ProvenanceOracle;
    use fermi::provenance_oracle::DbProvenanceOracle;

    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");
    let oracle = DbProvenanceOracle::new(pool.clone());

    // Three ids that certainly do not exist, which is the same situation as
    // the real dangling rule without depending on that row surviving cleanup.
    let ghosts = vec![
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    ];
    let f = oracle.extraction_floor(&ghosts).await.expect("oracle");

    assert_eq!(
        f.floor, None,
        "unfollowable citations must resolve to UNKNOWN, not to a value.\n\
         basis: {}",
        f.basis
    );
    assert_eq!(
        f.basis.get("missing_sources").and_then(|v| v.as_u64()),
        Some(3),
        "the basis must record that the sources could not be found, so the \
         gap is visible rather than inferable: {}",
        f.basis
    );
}

/// Is the football consistency check comparing anything?
///
/// `advanced_metrics.xgd` is the first cross-check here that needs no external
/// source of truth — it asks whether `xgd` equals `xg - xga` inside the agent's
/// own document. That makes it cheap enough to run always, and it introduces a
/// failure mode the other cross-checks do not have.
///
/// The other checks read a cached column that is either populated or not. This
/// one reads `episodes.response_text`, which is **prose for every episode this
/// agent has ever produced**. A query looking for JSON fields in prose matches
/// nothing, counts zero mismatches, and reports clean — and "clean" and "there
/// was nothing to look at" are the same number.
///
/// So the population is measured separately and the three states are named, the
/// same way `liveness_trust` refuses to let *inert* be spelled *pass*. This test
/// does not fail while the population is zero, because the agent emitting prose
/// is the current known state of the world rather than a defect. What it will not
/// do is let that state masquerade as a passing comparison.
#[tokio::test]
#[ignore = "needs DATABASE_URL; run via scripts/grounding_contract_live.sh"]
async fn the_football_consistency_check_is_live_or_says_it_is_inert() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");

    // How many episodes could the check possibly have looked at? Same guard as
    // the cross-check itself: `CASE` short-circuits, so the cast never sees
    // prose.
    let row = sqlx::query(
        "SELECT count(*)::bigint AS total, \
                count(*) FILTER (WHERE e.response_text IS JSON OBJECT)::bigint AS json_docs, \
                count(*) FILTER (WHERE jsonb_typeof(j.doc #> '{advanced_metrics,xgd}') = 'number' \
                                   AND jsonb_typeof(j.doc #> '{advanced_metrics,xg}')  = 'number' \
                                   AND jsonb_typeof(j.doc #> '{advanced_metrics,xga}') = 'number' \
                                )::bigint AS comparable \
           FROM episodes e \
           JOIN agents a ON a.agent_id = e.agent_id, \
           LATERAL (SELECT CASE WHEN e.response_text IS JSON OBJECT \
                                THEN e.response_text::jsonb END AS doc) j \
          WHERE a.agent_name = 'football_analyst' \
            AND e.response_text IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("population probe");

    let total: i64 = row.get("total");
    let json_docs: i64 = row.get("json_docs");
    let comparable: i64 = row.get("comparable");

    println!(
        "\n  football_analyst: {total} retained response(s), {json_docs} structured, \
         {comparable} carrying all three of xg/xga/xgd"
    );

    if comparable == 0 {
        println!(
            "  INERT — the check is watching and has compared nothing. This is the \
             expected state until the agent runs under the `fermi/football_evidence` \
             contract; its zero mismatches mean 'nothing to look at', NOT 'clean'."
        );
        // Deliberately not an assertion failure. The agent emitting prose is a
        // known state of the corpus, not a regression, and a permanently-red
        // check is one people learn to scroll past. The print is the point: a
        // zero that cannot be mistaken for a pass.
        return;
    }

    // Live. Now the same discipline as the taxonomy probe: confirm the
    // comparison can succeed, so a zero mismatch count means agreement rather
    // than a predicate that never matches.
    let agreeing: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint \
           FROM episodes e \
           JOIN agents a ON a.agent_id = e.agent_id, \
           LATERAL (SELECT CASE WHEN e.response_text IS JSON OBJECT \
                                THEN e.response_text::jsonb END AS doc) j \
          WHERE a.agent_name = 'football_analyst' \
            AND jsonb_typeof(j.doc #> '{advanced_metrics,xgd}') = 'number' \
            AND jsonb_typeof(j.doc #> '{advanced_metrics,xg}')  = 'number' \
            AND jsonb_typeof(j.doc #> '{advanced_metrics,xga}') = 'number' \
            AND abs( (j.doc #>> '{advanced_metrics,xgd}')::numeric \
                     - ( (j.doc #>> '{advanced_metrics,xg}')::numeric \
                       - (j.doc #>> '{advanced_metrics,xga}')::numeric ) ) \
                <= 0.15",
    )
    .fetch_one(&pool)
    .await
    .expect("agreement probe");

    println!("  LIVE — {agreeing} of {comparable} document(s) are internally consistent");
    assert!(
        agreeing > 0,
        "{comparable} document(s) carry all three fields and NONE of them is \
         internally consistent. Either every report is computing xgd from \
         something other than its own xg and xga, or the tolerance is wrong — \
         and a check that can only fail is as useless as one that can only pass."
    );
}

/// Are the weather checks watching anything, and can they go red?
///
/// The seven `weather_oracle` cross-checks all report zero mismatches, and until
/// there is a structured document in the corpus that means *nothing to look at*
/// rather than *clean*. This is the same inertness the football probe reports,
/// with one addition football does not have: a **falsifiability probe** per
/// check, confirming the predicate fires on a document that violates it.
///
/// That addition exists because six of the seven weather checks are range
/// assertions — a probability in `[0,1]`, a positive sd, a multiplier inside the
/// declared `[0.1, 10.0]`. A range check on an absent field is exactly the shape
/// that passes forever: `jsonb_typeof(NULL)` is NULL, the row drops, the count
/// is zero, and the report is green. Proving each predicate can return a row
/// against a synthetic violation is the only way to distinguish "no violations"
/// from "no query".
#[tokio::test]
#[ignore = "needs DATABASE_URL; run via scripts/grounding_contract_live.sh"]
async fn the_weather_checks_are_live_or_say_they_are_inert() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");

    // ── 1. Is there anything to look at? ────────────────────────────────
    let row = sqlx::query(
        "SELECT count(*)::bigint AS episodes, \
                count(*) FILTER (WHERE e.response_text IS NOT NULL)::bigint AS retained, \
                count(*) FILTER (WHERE e.response_text IS JSON OBJECT)::bigint AS structured, \
                count(*) FILTER (WHERE j.doc #>> '{settlement_target,station}' IS NOT NULL \
                                )::bigint AS with_station \
           FROM episodes e \
           JOIN agents a ON a.agent_id = e.agent_id, \
           LATERAL (SELECT CASE WHEN e.response_text IS JSON OBJECT \
                                THEN e.response_text::jsonb END AS doc) j \
          WHERE a.agent_name = 'weather_oracle'",
    )
    .fetch_one(&pool)
    .await
    .expect("population probe");

    let episodes: i64 = row.get("episodes");
    let retained: i64 = row.get("retained");
    let structured: i64 = row.get("structured");
    let with_station: i64 = row.get("with_station");

    println!(
        "\n  weather_oracle: {episodes} episode(s), {retained} with retained text, \
         {structured} structured, {with_station} naming a settlement station"
    );

    // ── 2. Can each predicate actually return a row? ────────────────────
    //
    // Run every check's WHERE clause against a synthetic violating document
    // rather than against the corpus. If a predicate cannot match a document
    // built to violate it, the check is decorative and its zero is meaningless.
    //
    // A station of 'XXXX' is not in the 50-entry registry; the sd is negative;
    // the probabilities are out of range; the multiplier is outside [0.1, 10];
    // and the book is untradeable while the recommendation says to buy.
    let violating = serde_json::json!({
        "settlement_target": { "station": "XXXX" },
        "stages": {
            "forecast":    { "n_members": 0, "ensemble_sd": -1.0 },
            "calibration": { "predictive_sd": -0.5, "calibrated_probability": 1.4,
                             "climatology_base_rate": -0.2 },
            "pricing":     { "implied_probability": 2.0, "book_tradeable": false }
        },
        "final_probability": 1.9,
        "multiplier": 42.0,
        "recommendation": { "action": "buy_yes" }
    })
    .to_string();

    // Each entry: (label, the check's discriminating predicate).
    // Written against a `doc` CTE so the same SQL shape as the real check runs
    // over one synthetic row instead of the episodes table.
    let probes: &[(&str, &str)] = &[
        (
            "settlement_target",
            "upper(doc #>> '{settlement_target,station}') NOT IN ('EGLC','KLGA','KORD')",
        ),
        (
            "stages.forecast",
            "(doc #>> '{stages,forecast,n_members}')::numeric < 1 \
             OR (doc #>> '{stages,forecast,ensemble_sd}')::numeric < 0",
        ),
        (
            "stages.calibration",
            "(doc #>> '{stages,calibration,predictive_sd}')::numeric <= 0 \
             OR (doc #>> '{stages,calibration,calibrated_probability}')::numeric > 1",
        ),
        (
            "stages.pricing",
            "(doc #>> '{stages,pricing,implied_probability}')::numeric > 1 \
             OR (doc #> '{stages,pricing,book_tradeable}' = 'false'::jsonb \
                 AND doc #>> '{recommendation,action}' <> 'no_trade')",
        ),
        (
            "climatology_base_rate",
            "(doc #>> '{stages,calibration,climatology_base_rate}')::numeric < 0",
        ),
        (
            "final_probability",
            "(doc ->> 'final_probability')::numeric > 1",
        ),
        (
            "multiplier",
            "(doc ->> 'multiplier')::numeric < 0.1 OR (doc ->> 'multiplier')::numeric > 10.0",
        ),
    ];

    let mut inert_predicates = Vec::new();
    for (label, predicate) in probes {
        let sql = format!(
            "SELECT count(*)::bigint FROM (SELECT $1::jsonb AS doc) t WHERE {predicate}"
        );
        let fires: i64 = sqlx::query_scalar(&sql)
            .bind(&violating)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("falsifiability probe for {label} did not run: {e}"));
        if fires == 1 {
            println!("    can fail   {label}");
        } else {
            println!("    INERT      {label}  <-- predicate did not match a violating document");
            inert_predicates.push(*label);
        }
    }

    assert!(
        inert_predicates.is_empty(),
        "{} weather check(s) cannot match a document built to violate them, so \
         their zero mismatch counts mean nothing: {}. A check that can only \
         pass is worse than no check, because it occupies the place where a \
         real one would go.",
        inert_predicates.len(),
        inert_predicates.join(", ")
    );

    // ── 3. State the corpus honestly ────────────────────────────────────
    if structured == 0 {
        println!(
            "  INERT ON CORPUS — all seven predicates can fail, and none has been \
             applied to anything. {episodes} episode(s) exist and {retained} carry \
             retained text; every one predates migration 199 or is prose, so the \
             zero mismatches mean 'nothing to look at', NOT 'clean'. Goes live on \
             the first run that emits the JSON document the card now specifies."
        );
        return;
    }

    println!(
        "  LIVE — {structured} structured document(s), {with_station} carrying a \
         settlement station"
    );
    assert!(
        with_station > 0,
        "{structured} structured document(s) and not one names a settlement \
         station. The station is the single largest error source in these \
         markets, so a corpus that never records it cannot be checked at all."
    );
}
