//! Loop 1 maturity — has an agent actually been dreaming, and did it learn
//! anything?
//!
//! ## Why this exists
//!
//! Loop 1 is the consolidation ("dreaming") loop: episodes accumulate, the
//! ConsolidationWorker clusters them, the `ontologist` extracts durable entities
//! and semantic rules, and the agent's knowledge graph grows. The loop is
//! *closed* only if that last part is true.
//!
//! Until now there was no way to see it. The dashboard could say "77 of 100 need
//! dreaming attention" but nothing answered the operator's actual question:
//! *has this agent matured, and has the ontologist genuinely developed its
//! ontology?* Worse, a consolidation cycle that ran perfectly and extracted
//! **nothing** looked identical to one that built a rich graph — both simply
//! advanced `last_consolidated_at`.
//!
//! ## Mechanism vs. outcome, again
//!
//! The same distinction that governs the Brier probe governs this one, and it is
//! the reason `yield` is reported separately from `cycles`:
//!
//! - **Cycles** say the machinery ran. Necessary, not sufficient.
//! - **Yield** says the ontologist produced entities, facts and rules. This is
//!   what "matured" means.
//! - **Ontology** is the accumulated result actually persisted.
//!
//! Those three can disagree, and each disagreement is a distinct diagnosis:
//!
//! | cycles | yield | ontology | Diagnosis |
//! |---|---|---|---|
//! | 0 | — | — | never dreamt |
//! | >0 | 0 | 0 | **running but learning nothing** — the silent failure |
//! | >0 | >0 | 0 | extraction works, persistence does not |
//! | >0 | >0 | >0 | healthy |
//!
//! The second row is the one worth building this for. A loop that runs on
//! schedule, charges credits, advances its timestamp and extracts zero rules is
//! indistinguishable from a healthy one on every dashboard the platform had.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use sqlx::Row;

use crate::{resolve_agent, AppState};
use fermi_auth::AuthPrincipal;

/// Coarse maturity band for an agent's dreaming loop.
///
/// Deliberately keyed on *yield and accumulation*, not on cycle count: an agent
/// that has dreamt fifty times and extracted nothing is not mature, it is
/// broken, and a band derived from cycle count alone would report it as
/// seasoned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DreamMaturity {
    /// No consolidation has ever run.
    Dormant,
    /// Cycles have run but produced no durable knowledge at all.
    Unproductive,
    /// Producing, but too little accumulated to rely on.
    Developing,
    /// A substantive ontology has accumulated across multiple cycles.
    Mature,
}

impl DreamMaturity {
    pub fn as_str(&self) -> &'static str {
        match self {
            DreamMaturity::Dormant => "dormant",
            DreamMaturity::Unproductive => "unproductive",
            DreamMaturity::Developing => "developing",
            DreamMaturity::Mature => "mature",
        }
    }
}

/// Inputs to the maturity judgement, kept as a struct so the classifier is a
/// pure function and can be unit-tested without a database.
#[derive(Debug, Clone, Copy, Default)]
pub struct MaturityInputs {
    pub completed_cycles: i64,
    pub failed_cycles: i64,
    /// Durable knowledge currently in the graph.
    pub entities: i64,
    pub facts: i64,
    pub rules: i64,
    /// Rules the ontologist proposed that verification rejected.
    pub rules_rejected: i64,
    pub unconsolidated_episodes: i64,
}

impl MaturityInputs {
    fn ontology_size(&self) -> i64 {
        self.entities + self.facts + self.rules
    }
}

/// Classify an agent's dreaming maturity, and say why.
///
/// Returns `(band, diagnosis)`. The diagnosis is written to be actionable: it
/// should tell an operator what to do next, not restate the numbers they can
/// already see.
pub fn classify_maturity(i: MaturityInputs) -> (DreamMaturity, String) {
    if i.completed_cycles == 0 {
        // Distinguish "never tried" from "tried and always failed" — very
        // different problems, identical ontology size.
        if i.failed_cycles > 0 {
            return (
                DreamMaturity::Dormant,
                format!(
                    "Never completed a consolidation cycle ({} failed). Check the \
                     dreaming budget and the ontologist's provider credentials.",
                    i.failed_cycles
                ),
            );
        }
        return (
            DreamMaturity::Dormant,
            if i.unconsolidated_episodes > 0 {
                format!(
                    "Never dreamt, with {} episodes waiting. Run a consolidation \
                     cycle to start Loop 1.",
                    i.unconsolidated_episodes
                )
            } else {
                "Never dreamt, and has no episodes to consolidate. The agent needs \
                 to be executed before it has anything to learn from."
                    .to_string()
            },
        );
    }

    if i.ontology_size() == 0 {
        // The silent failure this module exists to surface.
        return (
            DreamMaturity::Unproductive,
            if i.rules_rejected > 0 {
                format!(
                    "{} cycles completed and {} proposed rules were all rejected by \
                     verification — the ontologist is producing, but nothing survives. \
                     Inspect rejected rules before spending more dreaming credits.",
                    i.completed_cycles, i.rules_rejected
                )
            } else {
                format!(
                    "{} cycles completed but extracted nothing at all — no entities, \
                     facts or rules. The loop is running and learning nothing. Check \
                     that the ontologist has a working model and that episodes carry \
                     enough content to cluster.",
                    i.completed_cycles
                )
            },
        );
    }

    // Accumulating. Distinguish "some signal" from "genuinely established".
    // Thresholds are deliberately modest: the point is to separate an ontology
    // you can reason over from a handful of incidental rows.
    if i.completed_cycles >= 3 && i.ontology_size() >= 25 && i.rules > 0 {
        return (
            DreamMaturity::Mature,
            format!(
                "{} entities, {} facts and {} rules across {} cycles. The ontology is \
                 established enough to inform reasoning.",
                i.entities, i.facts, i.rules, i.completed_cycles
            ),
        );
    }

    (
        DreamMaturity::Developing,
        format!(
            "{} entities, {} facts and {} rules from {} cycle(s) — real but thin. \
             More consolidation cycles will thicken it.",
            i.entities, i.facts, i.rules, i.completed_cycles
        ),
    )
}

/// GET /api/agents/:agent_id/dreaming
///
/// Per-agent Loop 1 maturity: backlog, cycle history, extraction yield,
/// accumulated ontology, and a plain-language diagnosis.
pub async fn agent_dreaming_maturity_handler(
    State(state): State<AppState>,
    _principal: Option<AuthPrincipal>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let aid = db_agent.agent_id;
    let db = &state.db;

    // ── Episode backlog ──────────────────────────────────────────────────────
    let ep = sqlx::query(
        "SELECT COUNT(*) AS total,
                COUNT(*) FILTER (WHERE NOT consolidated) AS unconsolidated
           FROM episodes WHERE agent_id = $1",
    )
    .bind(aid)
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total_episodes: i64 = ep.as_ref().and_then(|r| r.try_get("total").ok()).unwrap_or(0);
    let unconsolidated: i64 = ep
        .as_ref()
        .and_then(|r| r.try_get("unconsolidated").ok())
        .unwrap_or(0);

    // ── Cycle history + cumulative extraction yield ──────────────────────────
    //
    // `zero_yield_cycles` is the headline diagnostic: completed cycles that
    // produced no rules, entities or facts. A high count next to a healthy
    // completion count is a loop that runs without learning.
    let cyc = sqlx::query(
        "SELECT COUNT(*) AS total,
                COUNT(*) FILTER (WHERE status = 'completed') AS completed,
                COUNT(*) FILTER (WHERE status = 'failed')    AS failed,
                COUNT(*) FILTER (WHERE status = 'running')   AS running,
                COUNT(*) FILTER (WHERE status = 'completed'
                                   AND rules_extracted = 0
                                   AND entities_created = 0
                                   AND facts_created = 0)    AS zero_yield,
                COALESCE(SUM(episodes_processed), 0)  AS episodes_processed,
                COALESCE(SUM(clusters_identified), 0) AS clusters,
                COALESCE(SUM(rules_extracted), 0)     AS rules_extracted,
                COALESCE(SUM(rules_verified), 0)      AS rules_verified,
                COALESCE(SUM(rules_rejected), 0)      AS rules_rejected,
                COALESCE(SUM(entities_created), 0)    AS entities_created,
                COALESCE(SUM(facts_created), 0)       AS facts_created,
                MAX(completed_at)                     AS last_completed_at
           FROM consolidation_jobs WHERE agent_id = $1",
    )
    .bind(aid)
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let g = |k: &str| -> i64 {
        cyc.as_ref()
            .and_then(|r| r.try_get::<i64, _>(k).ok())
            .unwrap_or(0)
    };
    let (completed, failed, running) = (g("completed"), g("failed"), g("running"));
    let rules_verified = g("rules_verified");
    let rules_rejected = g("rules_rejected");

    // Most recent cycle, so a failure is visible with its reason rather than
    // just decrementing a counter.
    let last = sqlx::query(
        "SELECT status, error_message, started_at, completed_at,
                rules_extracted, entities_created, facts_created
           FROM consolidation_jobs WHERE agent_id = $1
          ORDER BY started_at DESC LIMIT 1",
    )
    .bind(aid)
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ── Accumulated ontology (what the ontologist actually built) ────────────
    let onto = sqlx::query(
        "SELECT (SELECT COUNT(*) FROM entities       WHERE agent_id = $1) AS entities,
                (SELECT COUNT(*) FROM facts          WHERE agent_id = $1) AS facts,
                (SELECT COUNT(*) FROM semantic_rules WHERE agent_id = $1) AS rules,
                (SELECT COUNT(*) FROM semantic_rules WHERE agent_id = $1
                   AND verification_status = 'verified') AS verified_rules",
    )
    .bind(aid)
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let og = |k: &str| -> i64 {
        onto.as_ref()
            .and_then(|r| r.try_get::<i64, _>(k).ok())
            .unwrap_or(0)
    };
    let (entities, facts, rules) = (og("entities"), og("facts"), og("rules"));

    // Latest snapshot: proof the graph was committed, plus whether the dream
    // narrator ran (dream_synopsis). A snapshot without a synopsis means
    // consolidation succeeded but narration didn't — cosmetic, but it is the
    // thing an operator notices missing first.
    let snap = sqlx::query(
        "SELECT version, entity_count, fact_count, rule_count, created_at,
                dream_synopsis IS NOT NULL AND dream_synopsis <> '' AS has_synopsis,
                dream_synopsis
           FROM ontology_snapshots WHERE agent_id = $1
          ORDER BY version DESC LIMIT 1",
    )
    .bind(aid)
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (band, diagnosis) = classify_maturity(MaturityInputs {
        completed_cycles: completed,
        failed_cycles: failed,
        entities,
        facts,
        rules,
        rules_rejected,
        unconsolidated_episodes: unconsolidated,
    });

    let remaining = db_agent.dreaming_budget_credits - db_agent.dreaming_credits_used;

    Ok(Json(json!({
        "agent_id": aid,
        "agent_name": db_agent.agent_name,
        "maturity": band.as_str(),
        "diagnosis": diagnosis,

        "backlog": {
            "total_episodes": total_episodes,
            "unconsolidated_episodes": unconsolidated,
        },
        "last_consolidated_at": db_agent.last_consolidated_at,
        "budget": {
            "used": db_agent.dreaming_credits_used,
            "total": db_agent.dreaming_budget_credits,
            "remaining": remaining,
            "exhausted": remaining <= 0,
        },

        "cycles": {
            "total": g("total"),
            "completed": completed,
            "failed": failed,
            // A cycle stuck in `running` is normal for seconds and suspicious
            // for hours — it means a worker died mid-consolidation.
            "running": running,
            "zero_yield": g("zero_yield"),
            "last_completed_at": cyc.as_ref()
                .and_then(|r| r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_completed_at").ok())
                .flatten(),
            "last": last.as_ref().map(|r| json!({
                "status": r.try_get::<String, _>("status").unwrap_or_default(),
                "error": r.try_get::<Option<String>, _>("error_message").ok().flatten(),
                "started_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at").ok().flatten(),
                "completed_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at").ok().flatten(),
                "rules_extracted": r.try_get::<i32, _>("rules_extracted").unwrap_or(0),
                "entities_created": r.try_get::<i32, _>("entities_created").unwrap_or(0),
                "facts_created": r.try_get::<i32, _>("facts_created").unwrap_or(0),
            })),
        },

        // What the ontologist produced, cumulatively across all cycles.
        "yield": {
            "episodes_processed": g("episodes_processed"),
            "clusters_identified": g("clusters"),
            "rules_extracted": g("rules_extracted"),
            "rules_verified": rules_verified,
            "rules_rejected": rules_rejected,
            "entities_created": g("entities_created"),
            "facts_created": g("facts_created"),
            // Of the rules verification actually adjudicated, how many survived.
            "verification_rate": if rules_verified + rules_rejected > 0 {
                Some(rules_verified as f64 / (rules_verified + rules_rejected) as f64)
            } else { None },
        },

        // What is actually in the knowledge graph now.
        "ontology": {
            "entities": entities,
            "facts": facts,
            "semantic_rules": rules,
            "verified_rules": og("verified_rules"),
            "snapshot_version": snap.as_ref().and_then(|r| r.try_get::<i32, _>("version").ok()),
            "last_snapshot_at": snap.as_ref()
                .and_then(|r| r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at").ok())
                .flatten(),
            "has_dream_synopsis": snap.as_ref()
                .and_then(|r| r.try_get::<Option<bool>, _>("has_synopsis").ok())
                .flatten()
                .unwrap_or(false),
            "dream_synopsis": snap.as_ref()
                .and_then(|r| r.try_get::<Option<String>, _>("dream_synopsis").ok())
                .flatten(),
        },

        "note": "`cycles` says the machinery ran; `yield` says the ontologist produced something; `ontology` is what persisted. They can disagree, and each disagreement is a different fault — see `maturity` and `diagnosis`.",
    })))
}

/// GET /api/observatory/loops/dreaming/maturity
///
/// Fleet rollup of the same judgement, for the Loop 1 dashboard card. Scoped to
/// the caller's agents.
pub async fn fleet_dreaming_maturity_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // One pass over the fleet. Correlated subqueries rather than a pile of
    // LEFT JOINs so per-agent counts cannot be inflated by row multiplication —
    // the classic way a "facts" number ends up multiplied by the entity count.
    let rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.display_alias, a.last_consolidated_at,
                (SELECT COUNT(*) FROM episodes e
                  WHERE e.agent_id = a.agent_id AND NOT e.consolidated) AS unconsolidated,
                (SELECT COUNT(*) FROM consolidation_jobs j
                  WHERE j.agent_id = a.agent_id AND j.status = 'completed') AS completed,
                (SELECT COUNT(*) FROM consolidation_jobs j
                  WHERE j.agent_id = a.agent_id AND j.status = 'failed') AS failed,
                (SELECT COALESCE(SUM(j.rules_rejected), 0) FROM consolidation_jobs j
                  WHERE j.agent_id = a.agent_id) AS rules_rejected,
                (SELECT COUNT(*) FROM entities x       WHERE x.agent_id = a.agent_id) AS entities,
                (SELECT COUNT(*) FROM facts x          WHERE x.agent_id = a.agent_id) AS facts,
                (SELECT COUNT(*) FROM semantic_rules x WHERE x.agent_id = a.agent_id) AS rules
           FROM agents a
          WHERE a.user_id = $1 AND a.status != 'archived'
          ORDER BY a.agent_name",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut agents: Vec<Value> = Vec::with_capacity(rows.len());
    let (mut dormant, mut unproductive, mut developing, mut mature) = (0, 0, 0, 0);

    for r in &rows {
        let gi = |k: &str| -> i64 { r.try_get::<i64, _>(k).unwrap_or(0) };
        let inputs = MaturityInputs {
            completed_cycles: gi("completed"),
            failed_cycles: gi("failed"),
            entities: gi("entities"),
            facts: gi("facts"),
            rules: gi("rules"),
            rules_rejected: gi("rules_rejected"),
            unconsolidated_episodes: gi("unconsolidated"),
        };
        let (band, diagnosis) = classify_maturity(inputs);
        match band {
            DreamMaturity::Dormant => dormant += 1,
            DreamMaturity::Unproductive => unproductive += 1,
            DreamMaturity::Developing => developing += 1,
            DreamMaturity::Mature => mature += 1,
        }
        agents.push(json!({
            "agent_id": r.try_get::<uuid::Uuid, _>("agent_id").ok(),
            "agent_name": r.try_get::<String, _>("agent_name").unwrap_or_default(),
            "display_alias": r.try_get::<Option<String>, _>("display_alias").unwrap_or(None),
            "maturity": band.as_str(),
            "diagnosis": diagnosis,
            "unconsolidated_episodes": inputs.unconsolidated_episodes,
            "completed_cycles": inputs.completed_cycles,
            "failed_cycles": inputs.failed_cycles,
            "entities": inputs.entities,
            "facts": inputs.facts,
            "semantic_rules": inputs.rules,
            "last_consolidated_at": r
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_consolidated_at")
                .ok().flatten(),
        }));
    }

    Ok(Json(json!({
        "loop": "1",
        "label": "Dreaming / consolidation maturity",
        "counts": {
            "dormant": dormant,
            "unproductive": unproductive,
            "developing": developing,
            "mature": mature,
            "total": rows.len(),
        },
        // The number that matters most: cycles ran, nothing was learned.
        "needs_attention": unproductive,
        "status": if unproductive > 0 { "amber" } else if mature > 0 { "green" } else { "grey" },
        "agents": agents,
        "note": "`unproductive` counts agents whose consolidation cycles completed but produced no entities, facts or rules — a loop that runs and learns nothing. That is a fault, not immaturity.",
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_dreamt_is_dormant() {
        let (band, why) = classify_maturity(MaturityInputs {
            unconsolidated_episodes: 12,
            ..Default::default()
        });
        assert_eq!(band, DreamMaturity::Dormant);
        assert!(why.contains("12 episodes"), "{why}");
    }

    /// "Never tried" and "tried and always failed" have identical ontology
    /// sizes but completely different remedies, so they must not collapse into
    /// one message.
    #[test]
    fn only_failures_is_dormant_but_says_so() {
        let (band, why) = classify_maturity(MaturityInputs {
            failed_cycles: 4,
            ..Default::default()
        });
        assert_eq!(band, DreamMaturity::Dormant);
        assert!(why.contains("4 failed"), "{why}");
        assert!(why.contains("credential") || why.contains("budget"), "{why}");
    }

    #[test]
    fn no_episodes_at_all_says_execute_the_agent_first() {
        let (band, why) = classify_maturity(MaturityInputs::default());
        assert_eq!(band, DreamMaturity::Dormant);
        assert!(why.contains("executed"), "{why}");
    }

    /// The failure this module exists for: the loop runs, charges credits,
    /// advances its timestamp, and learns nothing. It must never be reported as
    /// maturity just because the cycle count is high.
    #[test]
    fn cycles_without_yield_are_unproductive_not_mature() {
        let (band, why) = classify_maturity(MaturityInputs {
            completed_cycles: 50,
            ..Default::default()
        });
        assert_eq!(band, DreamMaturity::Unproductive);
        assert!(why.contains("learning nothing"), "{why}");
    }

    /// Producing-but-all-rejected is a different fault from producing nothing,
    /// and points at verification rather than at the model.
    #[test]
    fn all_rules_rejected_names_verification_as_the_culprit() {
        let (band, why) = classify_maturity(MaturityInputs {
            completed_cycles: 6,
            rules_rejected: 14,
            ..Default::default()
        });
        assert_eq!(band, DreamMaturity::Unproductive);
        assert!(why.contains("rejected"), "{why}");
    }

    #[test]
    fn small_ontology_is_developing() {
        let (band, _) = classify_maturity(MaturityInputs {
            completed_cycles: 2,
            entities: 4,
            facts: 3,
            rules: 1,
            ..Default::default()
        });
        assert_eq!(band, DreamMaturity::Developing);
    }

    #[test]
    fn substantial_ontology_across_cycles_is_mature() {
        let (band, why) = classify_maturity(MaturityInputs {
            completed_cycles: 5,
            entities: 20,
            facts: 30,
            rules: 6,
            ..Default::default()
        });
        assert_eq!(band, DreamMaturity::Mature);
        assert!(why.contains("20 entities"), "{why}");
    }

    /// A big graph built in a single cycle is not yet evidence of a working
    /// loop — maturity is about repeated productive consolidation.
    #[test]
    fn one_big_cycle_is_not_yet_mature() {
        let (band, _) = classify_maturity(MaturityInputs {
            completed_cycles: 1,
            entities: 40,
            facts: 60,
            rules: 9,
            ..Default::default()
        });
        assert_eq!(band, DreamMaturity::Developing);
    }

    /// Entities and facts without a single surviving rule means nothing
    /// generalised; that is still developing, not mature.
    #[test]
    fn no_rules_caps_at_developing() {
        let (band, _) = classify_maturity(MaturityInputs {
            completed_cycles: 9,
            entities: 60,
            facts: 90,
            rules: 0,
            ..Default::default()
        });
        assert_eq!(band, DreamMaturity::Developing);
    }
}
